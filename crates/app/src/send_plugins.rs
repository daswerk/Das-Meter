//! Send Plugins feeding the app core: lists the transport's slots, sets the
//! listened-to flags the core asks for, and reads their audio.
//!
//! Shared memory has no way to wake the app, so the shell calls
//! [`SendPluginInput::pump`] on a timer while listening to Send Plugins: every
//! frame while audio is listened to, every [`LIST_EVERY`] otherwise.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use dasmeter_core::{AppCore, Event, SendPlugin, SendPluginState, Timing};
use dasmeter_transport::{
    RING_FRAMES, Reader, SlotInfo, SlotRef, SlotState, TABLE_NAME, TABLE_NAME_V1,
};

/// How often the Send Plugins are listed (and the app's heartbeat bumped).
pub const LIST_EVERY: Duration = Duration::from_millis(250);

/// A Send Plugin being listened to.
struct Listening {
    slot: SlotRef,
    /// Where the next read starts.
    cursor: u64,
    sample_rate: u32,
}

pub struct SendPluginInput {
    table: String,
    /// The table older Send Plugins write, read too.
    v1_table: Option<String>,
    /// Opened on first use, and again if that failed.
    reader: Option<Reader>,
    slots: Vec<SlotInfo>,
    listened: HashMap<u64, Listening>,
    listed_at: Option<Instant>,
    buffer: Vec<f32>,
}

impl SendPluginInput {
    pub fn new() -> SendPluginInput {
        let mut input = SendPluginInput::with_table(TABLE_NAME);
        input.v1_table = Some(TABLE_NAME_V1.to_owned());
        input
    }

    /// On a table with another name (and no v1 table), for tests.
    pub fn with_table(table: &str) -> SendPluginInput {
        SendPluginInput {
            table: table.to_owned(),
            v1_table: None,
            reader: None,
            slots: Vec::new(),
            listened: HashMap::new(),
            listed_at: None,
            buffer: vec![0.0; 2 * RING_FRAMES],
        }
    }

    /// On tables with other names, the v1 one too, for tests.
    #[cfg(test)]
    fn with_tables(table: &str, v1_table: &str) -> SendPluginInput {
        let mut input = SendPluginInput::with_table(table);
        input.v1_table = Some(v1_table.to_owned());
        input
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
            self.reader = match &self.v1_table {
                Some(v1) => Reader::open_named_with_v1(&self.table, v1).ok(),
                None => Reader::open_named(&self.table).ok(),
            };
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
                        sample_rate: info.details.sample_rate,
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
                // Where the DAW was at the first frame read.
                let first = read.next - read.frames as u64;
                let timing = reader
                    .timing(listening.slot)
                    .map(|said| timing(said.at(first, listening.sample_rate)));
                core.handle(Event::SendPluginAudio { id, frames, timing }, now);
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

fn timing(t: dasmeter_transport::Timing) -> Timing {
    Timing {
        tempo: t.tempo,
        beats: t.beats,
        bar_start: t.bar_start,
        signature: t.signature,
        playing: t.playing,
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
        // The app reads both layouts there are (ADR 0003).
        outdated: false,
        host_pid: info.host_pid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dasmeter_core::{
        Level, ListenTo, LoudnessMeterSettings, MeterKind, MeterSettings, MeterState, MeterView,
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

    /// An older Send Plugin (v1 table) and a new one feed a Meter each.
    #[test]
    fn older_and_newer_send_plugins_both_reach_their_meters() {
        let table = format!("dmapp-v2-{}", std::process::id());
        let old_table = format!("dmapp-v1-{}", std::process::id());
        let details = |name: &str| Details {
            name: name.into(),
            colour: 0x00_ff_00,
            mono: false,
            sample_rate: 48_000,
        };
        let new = Writer::claim_named(&table, Some(1), &details("Kick")).unwrap();
        let old = Writer::claim_named_v1(&old_table, Some(2), &details("Bass")).unwrap();
        let (mut new_audio, mut old_audio) = (new.writer.audio(), old.writer.audio());

        let loudness = MeterSettings::Loudness(LoudnessMeterSettings::default());
        let mut core = AppCore::with_meters(vec![loudness, loudness]);
        let mut input = SendPluginInput::with_tables(&table, &old_table);
        let start = Instant::now();
        core.handle(Event::SetListenTo(ListenTo::SendPlugins), Duration::ZERO);
        input.pump(&mut core, Duration::ZERO, start);
        assert_eq!(input.listed().len(), 2);
        core.handle(Event::PickSendPlugin { meter: 0, id: 1 }, Duration::ZERO);
        core.handle(Event::PickSendPlugin { meter: 1, id: 2 }, Duration::ZERO);

        // −20 dBFS into the new one, −30 into the old one.
        let block = 480;
        for step in 0..200 {
            let now = Duration::from_millis(10 * step);
            new.writer.heartbeat();
            old.writer.heartbeat();
            let tone = |level: f64| -> Vec<f32> {
                (0..block)
                    .map(|i| {
                        let t = (step as usize * block + i) as f64 / 48_000.0;
                        (level * (std::f64::consts::TAU * 1_000.0 * t).sin()) as f32
                    })
                    .collect()
            };
            let (loud, quiet) = (tone(0.1), tone(0.031_6));
            new_audio.push(&loud, &loud);
            old_audio.push(&quiet, &quiet);
            input.pump(&mut core, now, start + now);
        }
        core.decide(Duration::from_secs(3));
        let scene = core.scene().expect("a scene");
        let momentary = |meter: usize| {
            let MeterState::Live(MeterView::Loudness { display, .. }) =
                &scene.windows[0].meters[meter].state
            else {
                panic!("{:?}", scene.windows[0].meters[meter].state);
            };
            let Level::Tenths(tenths) = display.momentary else {
                panic!("meter {meter} is silent");
            };
            tenths
        };
        assert!((momentary(0) - -200).abs() <= 3, "{}", momentary(0));
        assert!((momentary(1) - -300).abs() <= 3, "{}", momentary(1));

        drop(input);
        drop((new_audio, old_audio, new, old));
        remove_table(&table);
        remove_table(&old_table);
    }

    #[test]
    fn the_daws_tempo_reaches_a_phase_scope() {
        let table = format!("dmapp-timing-{}", std::process::id());
        let details = Details {
            name: "Kick".into(),
            colour: 0x00_ff_00,
            mono: false,
            sample_rate: 48_000,
        };
        let kick = Writer::claim_named(&table, Some(1), &details).unwrap();
        let mut audio = kick.writer.audio();
        let mut core = AppCore::with_meters(vec![MeterSettings::default_of(MeterKind::PhaseScope)]);
        let mut input = SendPluginInput::with_table(&table);
        let start = Instant::now();
        core.handle(Event::SetListenTo(ListenTo::SendPlugins), Duration::ZERO);
        input.pump(&mut core, Duration::ZERO, start);

        // 96 BPM, two blocks per pump.
        let block = 240;
        let silence = vec![0.0f32; block];
        for step in 0..300u64 {
            let now = Duration::from_millis(10 * step);
            kick.writer.heartbeat();
            for half in 0..2 {
                let frame = (step * 2 + half) * block as u64;
                let beats = frame as f64 / 48_000.0 * 96.0 / 60.0;
                audio.set_timing(Some(&dasmeter_transport::Timing {
                    tempo: 96.0,
                    beats,
                    bar_start: (beats / 4.0).floor() * 4.0,
                    signature: (4, 4),
                    playing: true,
                }));
                audio.push(&silence, &silence);
            }
            input.pump(&mut core, now, start + now);
        }
        core.decide(Duration::from_secs(4));
        let scene = core.scene().expect("a scene");
        let MeterState::Live(MeterView::PhaseScope { scope, .. }) =
            &scene.windows[0].meters[0].state
        else {
            panic!("{:?}", scene.windows[0].meters[0].state);
        };
        assert!(scope.following_daw);
        assert_eq!(scope.tempo, 96.0);
        assert!(scope.note.is_none());

        drop(input);
        drop((audio, kick));
        remove_table(&table);
    }
}
