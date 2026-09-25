//! Send Plugins feeding the app core: lists the transport's slots, sets the
//! listened-to flags the core asks for, and reads their audio.
//!
//! Shared memory has no way to wake the app, so the shell calls
//! [`SendPluginInput::pump`] on a timer while listening to Send Plugins: every
//! frame while audio is listened to, every [`LIST_EVERY`] otherwise.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use dasmeter_core::{AppCore, Event, SendPlugin, SendPluginState};
use dasmeter_transport::{RING_FRAMES, Reader, SlotInfo, SlotRef, SlotState, TABLE_NAME};

/// How often the Send Plugins are listed (and the app's heartbeat bumped).
pub const LIST_EVERY: Duration = Duration::from_millis(250);

/// A Send Plugin being listened to.
struct Listening {
    slot: SlotRef,
    /// Where the next read starts.
    cursor: u64,
}

pub struct SendPluginInput {
    table: String,
    /// Opened on first use, and again if that failed.
    reader: Option<Reader>,
    slots: Vec<SlotInfo>,
    listened: HashMap<u64, Listening>,
    listed_at: Option<Instant>,
    buffer: Vec<f32>,
}

impl SendPluginInput {
    pub fn new() -> SendPluginInput {
        SendPluginInput::with_table(TABLE_NAME)
    }

    /// On a table with another name, for tests.
    pub fn with_table(table: &str) -> SendPluginInput {
        SendPluginInput {
            table: table.to_owned(),
            reader: None,
            slots: Vec::new(),
            listened: HashMap::new(),
            listed_at: None,
            buffer: vec![0.0; 2 * RING_FRAMES],
        }
    }

    /// The Send Plugins as last listed.
    pub fn listed(&self) -> Vec<SendPlugin> {
        self.slots.iter().map(send_plugin).collect()
    }

    /// Whether audio is being read: the shell then pumps every frame.
    pub fn listening(&self) -> bool {
        !self.listened.is_empty()
    }

    /// Lists the Send Plugins when due, follows the core's choice of which to
    /// listen to, and feeds their new audio into the core.
    pub fn pump(&mut self, core: &mut AppCore, now: Duration, instant: Instant) {
        if self.reader.is_none() {
            self.reader = Reader::open_named(&self.table).ok();
        }
        let Some(reader) = &mut self.reader else {
            return;
        };
        if self
            .listed_at
            .is_none_or(|at| instant.duration_since(at) >= LIST_EVERY)
        {
            self.listed_at = Some(instant);
            reader.heartbeat();
            self.slots = reader.slots(instant);
            let listed: Vec<SendPlugin> = self.slots.iter().map(send_plugin).collect();
            core.handle(Event::SendPlugins(&listed), now);
        }

        // Follow the core: start listening to the Send Plugins it shows, stop the rest.
        let wanted = core.listened();
        self.listened.retain(|id, listening| {
            let keep = wanted.contains(id);
            if !keep {
                reader.set_listened(listening.slot, false);
            }
            keep
        });
        for id in wanted {
            let Some(info) = self.slots.iter().find(|info| info.id == id) else {
                continue;
            };
            let current = self.listened.get(&id).map(|l| l.slot);
            if current == Some(info.slot) {
                continue;
            }
            // New, or the Send Plugin came back in another slot.
            reader.set_listened(info.slot, true);
            if let Some(cursor) = reader.cursor(info.slot) {
                self.listened.insert(
                    id,
                    Listening {
                        slot: info.slot,
                        cursor,
                    },
                );
            }
        }

        for (&id, listening) in &mut self.listened {
            let Some(read) = reader.read(listening.slot, listening.cursor, &mut self.buffer) else {
                continue;
            };
            listening.cursor = read.next;
            if read.frames > 0 {
                let frames = &self.buffer[..2 * read.frames];
                core.handle(Event::SendPluginAudio { id, frames }, now);
            }
        }
    }

    /// Stops listening to everything: the app switched to System Capture.
    pub fn stop(&mut self) {
        if let Some(reader) = &self.reader {
            for listening in self.listened.values() {
                reader.set_listened(listening.slot, false);
            }
        }
        self.listened.clear();
    }
}

impl Drop for SendPluginInput {
    fn drop(&mut self) {
        self.stop();
    }
}

fn send_plugin(info: &SlotInfo) -> SendPlugin {
    SendPlugin {
        id: info.id,
        name: info.details.name.clone(),
        colour: info.details.colour,
        mono: info.details.mono,
        sample_rate: info.details.sample_rate,
        state: match info.state {
            SlotState::Live => SendPluginState::Live,
            SlotState::Idle => SendPluginState::Idle,
            SlotState::Gone => SendPluginState::Gone,
        },
        // Only one layout exists so far; an older one would come from a
        // second table the app also reads (ADR 0003).
        outdated: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dasmeter_core::{
        Level, ListenTo, LoudnessMeterSettings, MeterSettings, MeterState, MeterView,
    };
    use dasmeter_transport::{Details, Writer, remove_table};

    /// A transport writer in this process feeds a Loudness Meter end to end:
    /// claim, listen, read, analyse, scene.
    #[test]
    fn a_send_plugin_in_this_process_reaches_a_meter() {
        let table = format!("dmapp-e2e-{}", std::process::id());
        let details = Details {
            name: "Kick".into(),
            colour: 0xff_00_00,
            mono: false,
            sample_rate: 48_000,
        };
        let claimed = Writer::claim_named(&table, None, &details).expect("claim a slot");
        let writer = claimed.writer;
        let mut audio = writer.audio();

        let mut core = AppCore::with_meters(vec![MeterSettings::Loudness(
            LoudnessMeterSettings::default(),
        )]);
        let mut input = SendPluginInput::with_table(&table);
        let start = Instant::now();
        core.handle(Event::SetListenTo(ListenTo::SendPlugins), Duration::ZERO);

        // A −20 dBFS 1 kHz sine, 2 s in 10 ms blocks, as a DAW would process it.
        let block = 480;
        let mut phase = 0usize;
        for step in 0..200 {
            let now = Duration::from_millis(10 * step);
            let instant = start + now;
            writer.heartbeat();
            let tone: Vec<f32> = (0..block)
                .map(|i| {
                    let t = (phase + i) as f64 / 48_000.0;
                    (0.1 * (2.0 * std::f64::consts::PI * 1_000.0 * t).sin()) as f32
                })
                .collect();
            phase += block;
            audio.push(&tone, &tone);
            input.pump(&mut core, now, instant);
        }
        assert!(input.listening(), "the only Send Plugin is listened to");

        let now = Duration::from_secs(3);
        core.decide(now);
        let scene = core.scene().expect("a scene");
        let MeterState::Live(MeterView::Loudness { display, .. }) =
            &scene.windows[0].meters[0].state
        else {
            panic!(
                "expected a live Loudness Meter: {:?}",
                scene.windows[0].meters[0].state
            );
        };
        let Level::Tenths(tenths) = display.momentary else {
            panic!("momentary is silent");
        };
        assert!(
            (tenths - -200).abs() <= 3,
            "momentary {tenths} tenths of a LUFS"
        );

        drop(input);
        drop(audio);
        drop(writer);
        remove_table(&table);
    }
}
