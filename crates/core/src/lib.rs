//! The headless app core: all app behaviour, driven by events.
//!
//! It takes events in (audio, Send Plugins, displays, user actions) and puts a
//! scene description and persistence writes out, so it can be tested without
//! windows, a GPU or audio devices. This crate must not depend on platform, GPU
//! or audio-device code (checked by `scripts/check-headless-deps.sh`).
//!
//! Time comes in with every call as the time since the app started, so tests
//! drive the core with a fake clock.

pub mod meters;
pub mod scene;
pub mod sources;
pub mod theme;

use std::time::Duration;

pub use meters::{
    CursorReadout, LoudnessMeterSettings, LufsBar, MeterSettings, MeterView, SpectrumMeterSettings,
    StereoDrawing, StereometerMeterSettings, WaveformColouring, WaveformMeterSettings,
};
pub use scene::{
    ChannelDisplay, Frame, Level, LoudnessDisplay, MeterScene, MeterState, Note, Scene, SourceItem,
    SourceLabel, WindowScene,
};
pub use sources::{ListenTo, Pick, SendPlugin, SendPluginState};
pub use theme::{Colour, Palette, Role};

use meters::Meter;
use sources::Resolved;

/// How long a note such as "Output changed: reset" stays on screen.
pub const NOTE_DURATION: Duration = Duration::from_secs(3);

/// The default frame-rate cap: 60 fps.
pub const DEFAULT_FRAME_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 60);

/// Something that happened, fed into the core by the shell.
#[derive(Clone, Copy, Debug)]
pub enum Event<'a> {
    /// System Capture started delivering audio at this sample rate. When it was
    /// already running, the output device or its rate changed.
    CaptureStarted { sample_rate: u32 },
    /// System Capture couldn't start (or stopped); the reason is shown.
    CaptureFailed(&'a str),
    /// Interleaved stereo frames from the active Source, at its sample rate.
    Audio(&'a [f32]),
    /// Whether any of the app's windows can be seen (not minimised or covered).
    Visible(bool),
    /// The pointer moved to this point of the window (fractions, 0–1 from the
    /// top-left), or left it.
    Pointer(Option<[f32; 2]>),
    /// A Meter's settings changed. `meter` is its index in the window.
    SetMeter {
        meter: usize,
        settings: MeterSettings,
    },
    /// The user switched Listen to. The shell pauses System Capture while it
    /// is Send Plugins.
    SetListenTo(ListenTo),
    /// The Send Plugins the transport lists now, gone ones included. The shell
    /// sends this every few hundred milliseconds while listening to Send Plugins.
    SendPlugins(&'a [SendPlugin]),
    /// Interleaved stereo frames from one Send Plugin, at its sample rate.
    SendPluginAudio { id: u64, frames: &'a [f32] },
    /// The user picked a Send Plugin for a Meter (its Source item, or its list).
    PickSendPlugin { meter: usize, id: u64 },
    /// "Use for all Meters": every Meter takes the Send Plugin this one shows.
    UseForAllMeters { meter: usize },
    /// The Source label on a Meter was switched on or off.
    ShowSourceLabel { meter: usize, shown: bool },
    /// A click at this point of the window (fractions, 0–1 from the top-left).
    Click([f32; 2]),
}

/// What the shell should do next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Draw [`AppCore::scene`] now.
    Draw,
    /// Draw nothing. Ask again at `until` (time since start), or on the next
    /// event if `None`: the scene has settled.
    Sleep { until: Option<Duration> },
}

/// System Capture's state.
enum Capture {
    Starting,
    Failed(String),
    Live,
}

/// A Meter with its Source.
struct MeterSlot {
    meter: Meter,
    /// The Send Plugin the user picked. Kept while on System Capture.
    pick: Option<Pick>,
    show_source_label: bool,
    /// On Send Plugins: the ID and sample rate of the Send Plugin whose audio
    /// the analyser has, and when it was last fed (audio or idle silence).
    showing: Option<Showing>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Showing {
    id: u64,
    sample_rate: u32,
    fed_at: Duration,
}

/// Most silence fed at once for an idle Send Plugin, in seconds: enough for
/// every Meter to fall quiet, however long the gap since the last feed.
const MAX_IDLE_FEED_SECONDS: f64 = 1.0;

/// The app core. Owns Listen to, the Send Plugin picks and the Meters with their analysers.
pub struct AppCore {
    listen_to: ListenTo,
    capture: Capture,
    /// The Send Plugins last listed by the transport.
    send_plugins: Vec<SendPlugin>,
    meters: Vec<MeterSlot>,
    pointer: Option<[f32; 2]>,
    note_until: Option<Duration>,
    visible: bool,
    frame_interval: Duration,
    palette: Palette,
    /// Something happened since the last scene was built.
    changed: bool,
    /// The scene last handed to the shell, and when.
    drawn: Option<Scene>,
    drawn_at: Option<Duration>,
}

impl Default for AppCore {
    fn default() -> Self {
        AppCore::new()
    }
}

impl AppCore {
    /// The default layout: one window with a row of Waveform, Spectrum,
    /// Stereometer and Loudness Meter, all on default settings.
    pub fn new() -> AppCore {
        AppCore::with_meters(vec![
            MeterSettings::Waveform(WaveformMeterSettings::default()),
            MeterSettings::Spectrum(SpectrumMeterSettings::default()),
            MeterSettings::Stereometer(StereometerMeterSettings::default()),
            MeterSettings::Loudness(LoudnessMeterSettings::default()),
        ])
    }

    /// One window with these Meters in a row, left to right.
    pub fn with_meters(meters: Vec<MeterSettings>) -> AppCore {
        AppCore {
            listen_to: ListenTo::SystemCapture,
            capture: Capture::Starting,
            send_plugins: Vec::new(),
            meters: meters
                .into_iter()
                .map(|settings| MeterSlot {
                    meter: Meter::new(settings),
                    pick: None,
                    show_source_label: false,
                    showing: None,
                })
                .collect(),
            pointer: None,
            note_until: None,
            visible: true,
            frame_interval: DEFAULT_FRAME_INTERVAL,
            palette: Palette::dark(),
            changed: true,
            drawn: None,
            drawn_at: None,
        }
    }

    /// Changes the frame-rate cap (the shortest time between two draws).
    pub fn set_frame_interval(&mut self, interval: Duration) {
        self.frame_interval = interval;
    }

    /// The settings of the Meter at `meter`, if there is one.
    pub fn meter_settings(&self, meter: usize) -> Option<MeterSettings> {
        self.meters.get(meter).map(|slot| slot.meter.settings())
    }

    pub fn listen_to(&self) -> ListenTo {
        self.listen_to
    }

    /// The Send Plugin a Meter picked, if it did.
    pub fn pick(&self, meter: usize) -> Option<&Pick> {
        self.meters.get(meter)?.pick.as_ref()
    }

    /// The Send Plugins to listen to: the ones the Meters show. The shell sets
    /// their listened-to flags and clears the others'. Empty on System Capture.
    pub fn listened(&self) -> Vec<u64> {
        let mut ids: Vec<u64> = self
            .meters
            .iter()
            .filter_map(|slot| slot.showing.map(|showing| showing.id))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    pub fn handle(&mut self, event: Event, now: Duration) {
        match event {
            Event::CaptureStarted { sample_rate } => {
                let restarted = matches!(self.capture, Capture::Live);
                self.capture = Capture::Live;
                if self.listen_to != ListenTo::SystemCapture {
                    return;
                }
                if restarted {
                    self.note_until = Some(now + NOTE_DURATION);
                }
                for slot in &mut self.meters {
                    slot.meter.start(sample_rate);
                }
            }
            Event::CaptureFailed(reason) => {
                self.capture = Capture::Failed(reason.to_owned());
                if self.listen_to != ListenTo::SystemCapture {
                    return;
                }
                for slot in &mut self.meters {
                    slot.meter.stop();
                }
            }
            Event::Audio(frames) => {
                if self.listen_to != ListenTo::SystemCapture {
                    return;
                }
                for slot in &mut self.meters {
                    slot.meter.process(frames);
                }
            }
            Event::Visible(visible) => self.visible = visible,
            Event::Pointer(pointer) => {
                if pointer == self.pointer {
                    return;
                }
                self.pointer = pointer;
                for slot in &mut self.meters {
                    slot.meter.pointer_changed();
                }
            }
            Event::SetMeter { meter, settings } => match self.meters.get_mut(meter) {
                Some(slot) => slot.meter.set_settings(settings),
                None => return,
            },
            Event::SetListenTo(listen_to) => {
                if listen_to == self.listen_to {
                    return;
                }
                self.listen_to = listen_to;
                // System Capture starts over when it comes back; Send Plugins
                // are routed from the last list until the next one arrives.
                self.capture = Capture::Starting;
                for slot in &mut self.meters {
                    slot.meter.stop();
                    slot.showing = None;
                }
                self.route(now);
            }
            Event::SendPlugins(listed) => {
                self.send_plugins = listed.to_vec();
                self.follow_renames();
                self.route(now);
            }
            Event::SendPluginAudio { id, frames } => {
                let mut fed = false;
                for slot in &mut self.meters {
                    if let Some(showing) = slot.showing.as_mut().filter(|s| s.id == id) {
                        slot.meter.process(frames);
                        showing.fed_at = now;
                        fed = true;
                    }
                }
                if !fed {
                    return;
                }
            }
            Event::PickSendPlugin { meter, id } => {
                let Some(plugin) = self.send_plugins.iter().find(|p| p.id == id) else {
                    return;
                };
                if plugin.outdated || meter >= self.meters.len() {
                    return;
                }
                self.meters[meter].pick = Some(Pick::of(plugin));
                self.route(now);
            }
            Event::UseForAllMeters { meter } => {
                let Some(pick) = self.shown_pick(meter) else {
                    return;
                };
                for slot in &mut self.meters {
                    slot.pick = Some(pick.clone());
                }
                self.route(now);
            }
            Event::ShowSourceLabel { meter, shown } => match self.meters.get_mut(meter) {
                Some(slot) => slot.show_source_label = shown,
                None => return,
            },
            Event::Click(point) => {
                let Some((meter, id)) = self.item_at(point) else {
                    return;
                };
                self.handle(Event::PickSendPlugin { meter, id }, now);
                return;
            }
        }
        self.changed = true;
    }

    /// The pick a Meter shows: its own, or the one it follows, or the only
    /// Send Plugin there is.
    fn shown_pick(&self, meter: usize) -> Option<Pick> {
        match self.resolve(meter) {
            Resolved::Plugin(plugin) => Some(Pick::of(plugin)),
            _ => self.meters.get(meter)?.pick.clone(),
        }
    }

    fn resolve(&self, meter: usize) -> Resolved<'_> {
        let own = self.meters[meter].pick.as_ref();
        let followed = self
            .meters
            .iter()
            .enumerate()
            .find_map(|(i, slot)| slot.pick.as_ref().filter(|_| i != meter));
        sources::resolve(own, followed, &self.send_plugins)
    }

    /// Picks follow their Send Plugin: a rename updates the stored name, and a
    /// pick found only by name (a new ID) takes the new ID.
    fn follow_renames(&mut self) {
        for slot in &mut self.meters {
            if let Some(pick) = &mut slot.pick {
                if let Some(plugin) = sources::find(pick, &self.send_plugins) {
                    *pick = Pick::of(plugin);
                }
            }
        }
    }

    /// Points each Meter's analyser at the Send Plugin it should show, and
    /// feeds idle Send Plugins' Meters silence. Does nothing on System Capture.
    fn route(&mut self, now: Duration) {
        if self.listen_to != ListenTo::SendPlugins {
            return;
        }
        // With exactly one Send Plugin and no picks, every Meter takes it, and
        // keeps it when more appear.
        if self.meters.iter().all(|slot| slot.pick.is_none()) {
            let mut usable = self.send_plugins.iter().filter(|p| p.usable());
            if let (Some(only), None) = (usable.next(), usable.next()) {
                let pick = Pick::of(only);
                for slot in &mut self.meters {
                    slot.pick = Some(pick.clone());
                }
            }
        }
        for meter in 0..self.meters.len() {
            let target = match self.resolve(meter) {
                Resolved::Plugin(plugin) => {
                    Some((plugin.id, plugin.sample_rate, plugin.mono, plugin.state))
                }
                _ => None,
            };
            let slot = &mut self.meters[meter];
            let Some((id, sample_rate, mono, state)) = target else {
                if slot.showing.take().is_some() {
                    slot.meter.stop();
                }
                continue;
            };
            match slot.showing {
                Some(showing) if showing.id == id && showing.sample_rate == sample_rate => {}
                previous => {
                    // Same Send Plugin at a new rate: reset like an output change.
                    if previous.is_some_and(|p| p.id == id) {
                        self.note_until = Some(now + NOTE_DURATION);
                    }
                    slot.meter.start(sample_rate);
                    slot.showing = Some(Showing {
                        id,
                        sample_rate,
                        fed_at: now,
                    });
                }
            }
            slot.meter.set_mono(mono);
            if state == SendPluginState::Idle {
                // An idle Send Plugin sends nothing: its Meters fall silent as
                // they would on quiet audio.
                if let Some(showing) = &mut slot.showing {
                    let seconds = now
                        .saturating_sub(showing.fed_at)
                        .as_secs_f64()
                        .min(MAX_IDLE_FEED_SECONDS);
                    let frames = (seconds * f64::from(sample_rate)) as usize;
                    feed_silence(&mut slot.meter, frames);
                    showing.fed_at = now;
                }
            }
        }
    }

    /// The pickable item in a "Pick a Send Plugin" list on screen at `point`.
    fn item_at(&self, point: [f32; 2]) -> Option<(usize, u64)> {
        let window = self.drawn.as_ref()?.windows.first()?;
        window
            .meters
            .iter()
            .enumerate()
            .find_map(|(meter, scene)| match &scene.state {
                MeterState::PickSendPlugin(items) => items
                    .iter()
                    .find(|item| item.pickable && item.frame.locate(point).is_some())
                    .map(|item| (meter, item.id)),
                _ => None,
            })
    }

    /// Decides whether to draw now. On [`Decision::Draw`], the shell draws
    /// [`AppCore::scene`]; the core counts that as drawn at `now`.
    pub fn decide(&mut self, now: Duration) -> Decision {
        if !self.visible {
            return Decision::Sleep { until: None };
        }
        let note_until = self.note_until.filter(|&until| until > now);
        let note_expired = self.note_until.is_some() && note_until.is_none();
        if !self.changed && !note_expired && self.drawn.is_some() {
            return Decision::Sleep { until: note_until };
        }
        // Wait for the next frame before building a scene at all.
        if let Some(drawn_at) = self.drawn_at {
            let next_frame = drawn_at + self.frame_interval;
            if now < next_frame {
                return Decision::Sleep {
                    until: Some(next_frame),
                };
            }
        }
        if note_expired {
            self.note_until = None;
        }
        self.changed = false;
        let scene = self.build_scene(note_until.is_some());
        if self.drawn.as_ref() == Some(&scene) {
            return Decision::Sleep { until: note_until };
        }
        self.drawn = Some(scene);
        self.drawn_at = Some(now);
        Decision::Draw
    }

    /// The scene to draw: the one from the last [`Decision::Draw`], or `None`
    /// before the first.
    pub fn scene(&self) -> Option<&Scene> {
        self.drawn.as_ref()
    }

    fn build_scene(&mut self, note: bool) -> Scene {
        let count = self.meters.len().max(1) as f32;
        let pointer = self.pointer;
        let mut meters = Vec::with_capacity(self.meters.len());
        for i in 0..self.meters.len() {
            let frame = Frame {
                x: i as f32 / count,
                y: 0.0,
                width: 1.0 / count,
                height: 1.0,
            };
            let (state, source) = match self.listen_to {
                ListenTo::SystemCapture => {
                    let state = match &self.capture {
                        Capture::Starting => MeterState::Starting,
                        Capture::Failed(reason) => MeterState::Unavailable(reason.clone()),
                        Capture::Live => self.live_state(i, frame, pointer),
                    };
                    let source = SourceLabel {
                        name: "System Capture".to_owned(),
                        colour: None,
                    };
                    (state, Some(source))
                }
                ListenTo::SendPlugins => match self.resolve(i) {
                    Resolved::Plugin(plugin) => {
                        let source = SourceLabel {
                            name: plugin.name.clone(),
                            colour: Some(colour(plugin.colour)),
                        };
                        (self.live_state(i, frame, pointer), Some(source))
                    }
                    Resolved::Waiting(name) => {
                        let source = SourceLabel {
                            name: name.clone(),
                            colour: None,
                        };
                        (MeterState::WaitingFor(name), Some(source))
                    }
                    Resolved::Choose => (MeterState::PickSendPlugin(self.items(frame)), None),
                    Resolved::Nothing => (MeterState::NoSendPlugins, None),
                },
            };
            let source = source.filter(|_| self.meters[i].show_source_label);
            meters.push(MeterScene {
                frame,
                state,
                source,
            });
        }
        Scene {
            windows: vec![WindowScene {
                title: "Das-Meter".to_owned(),
                meters,
            }],
            notes: if note {
                vec![Note::OutputChanged]
            } else {
                Vec::new()
            },
            palette: self.palette.clone(),
        }
    }

    fn live_state(&mut self, meter: usize, frame: Frame, pointer: Option<[f32; 2]>) -> MeterState {
        let pointer = pointer.and_then(|p| frame.locate(p));
        match self.meters[meter].meter.view(pointer) {
            Some(view) => MeterState::Live(view.clone()),
            None => MeterState::Starting,
        }
    }

    /// The "Pick a Send Plugin" list for a Meter in `frame`: one row per
    /// listed Send Plugin that isn't gone, below a title row.
    fn items(&self, frame: Frame) -> Vec<SourceItem> {
        let listed: Vec<&SendPlugin> = self
            .send_plugins
            .iter()
            .filter(|p| p.state != SendPluginState::Gone)
            .collect();
        let row = (1.0 / (listed.len() as f32 + 1.0)).min(PICK_ROW);
        listed
            .iter()
            .enumerate()
            .map(|(i, plugin)| SourceItem {
                id: plugin.id,
                label: plugin.label(),
                colour: colour(plugin.colour),
                pickable: !plugin.outdated,
                frame: Frame {
                    x: frame.x,
                    y: frame.y + frame.height * row * (i as f32 + 1.0),
                    width: frame.width,
                    height: frame.height * row,
                },
            })
            .collect()
    }
}

/// The height of a row in the "Pick a Send Plugin" list, as a fraction of the Meter.
const PICK_ROW: f32 = 0.14;

fn colour(rgb: u32) -> Colour {
    Colour::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn feed_silence(meter: &mut Meter, frames: usize) {
    const CHUNK: usize = 4_096;
    let silence = [0.0f32; 2 * CHUNK];
    let mut left = frames;
    while left > 0 {
        let n = left.min(CHUNK);
        meter.process(&silence[..2 * n]);
        left -= n;
    }
}
