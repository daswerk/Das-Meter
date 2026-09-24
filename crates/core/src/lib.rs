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
pub mod theme;

use std::time::Duration;

pub use meters::{
    CursorReadout, LoudnessMeterSettings, LufsBar, MeterSettings, MeterView, SpectrumMeterSettings,
    StereoDrawing, StereometerMeterSettings, WaveformColouring, WaveformMeterSettings,
};
pub use scene::{
    ChannelDisplay, Frame, Level, LoudnessDisplay, MeterScene, MeterState, Note, Scene,
    WindowScene,
};
pub use theme::{Colour, Palette, Role};

use meters::Meter;

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

enum Source {
    Starting,
    Failed(String),
    Live,
}

/// The app core. Owns the active Source and the Meters with their analysers.
pub struct AppCore {
    source: Source,
    meters: Vec<Meter>,
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
            source: Source::Starting,
            meters: meters.into_iter().map(Meter::new).collect(),
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
        self.meters.get(meter).map(Meter::settings)
    }

    pub fn handle(&mut self, event: Event, now: Duration) {
        match event {
            Event::CaptureStarted { sample_rate } => {
                if matches!(self.source, Source::Live) {
                    self.note_until = Some(now + NOTE_DURATION);
                }
                self.source = Source::Live;
                for meter in &mut self.meters {
                    meter.start(sample_rate);
                }
            }
            Event::CaptureFailed(reason) => {
                self.source = Source::Failed(reason.to_owned());
                for meter in &mut self.meters {
                    meter.stop();
                }
            }
            Event::Audio(frames) => {
                for meter in &mut self.meters {
                    meter.process(frames);
                }
            }
            Event::Visible(visible) => self.visible = visible,
            Event::Pointer(pointer) => {
                if pointer == self.pointer {
                    return;
                }
                self.pointer = pointer;
                for meter in &mut self.meters {
                    meter.pointer_changed();
                }
            }
            Event::SetMeter { meter, settings } => match self.meters.get_mut(meter) {
                Some(meter) => meter.set_settings(settings),
                None => return,
            },
        }
        self.changed = true;
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
        let meters = self
            .meters
            .iter_mut()
            .enumerate()
            .map(|(i, meter)| {
                let frame = Frame {
                    x: i as f32 / count,
                    y: 0.0,
                    width: 1.0 / count,
                    height: 1.0,
                };
                let state = match &self.source {
                    Source::Starting => MeterState::Starting,
                    Source::Failed(reason) => MeterState::Unavailable(reason.clone()),
                    Source::Live => match meter.view(pointer.and_then(|p| frame.locate(p))) {
                        Some(view) => MeterState::Live(view.clone()),
                        None => MeterState::Starting,
                    },
                };
                MeterScene { frame, state }
            })
            .collect();
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
}
