//! The headless app core: all app behaviour, driven by events.
//!
//! It takes events in (audio, Send Plugins, displays, user actions) and puts a
//! scene description and persistence writes out, so it can be tested without
//! windows, a GPU or audio devices. This crate must not depend on platform, GPU
//! or audio-device code (checked by `scripts/check-headless-deps.sh`).
//!
//! Time comes in with every call as the time since the app started, so tests
//! drive the core with a fake clock.

pub mod scene;

use std::time::Duration;

use dasmeter_analysis::{LoudnessAnalyser, LoudnessSettings};

pub use scene::{
    ChannelDisplay, Level, LoudnessDisplay, LoudnessScene, MeterScene, Note, Scene, WindowScene,
};

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
    Live(Box<LoudnessAnalyser>),
}

/// The app core. Owns the active Source and the Loudness Meter's analyser.
pub struct AppCore {
    source: Source,
    note_until: Option<Duration>,
    visible: bool,
    frame_interval: Duration,
    /// The scene last handed to the shell, and when.
    drawn: Option<Scene>,
    drawn_at: Option<Duration>,
    /// Cached Loudness Meter display, rebuilt only after new audio.
    display: Option<LoudnessDisplay>,
}

impl Default for AppCore {
    fn default() -> Self {
        AppCore::new()
    }
}

impl AppCore {
    pub fn new() -> AppCore {
        AppCore {
            source: Source::Starting,
            note_until: None,
            visible: true,
            frame_interval: DEFAULT_FRAME_INTERVAL,
            drawn: None,
            drawn_at: None,
            display: None,
        }
    }

    /// Changes the frame-rate cap (the shortest time between two draws).
    pub fn set_frame_interval(&mut self, interval: Duration) {
        self.frame_interval = interval;
    }

    pub fn handle(&mut self, event: Event, now: Duration) {
        match event {
            Event::CaptureStarted { sample_rate } => {
                if matches!(self.source, Source::Live(_)) {
                    self.note_until = Some(now + NOTE_DURATION);
                }
                self.source = Source::Live(Box::new(LoudnessAnalyser::new(
                    sample_rate,
                    LoudnessSettings::default(),
                )));
                self.display = None;
            }
            Event::CaptureFailed(reason) => {
                self.source = Source::Failed(reason.to_owned());
                self.display = None;
            }
            Event::Audio(frames) => {
                if let Source::Live(analyser) = &mut self.source {
                    analyser.process(frames);
                    self.display = None;
                }
            }
            Event::Visible(visible) => self.visible = visible,
        }
    }

    /// Decides whether to draw now. On [`Decision::Draw`], the shell draws
    /// [`AppCore::scene`]; the core counts that as drawn at `now`.
    pub fn decide(&mut self, now: Duration) -> Decision {
        if !self.visible {
            return Decision::Sleep { until: None };
        }
        let scene = self.build_scene(now);
        if self.drawn.as_ref() == Some(&scene) {
            // Nothing new. Wake up only to take a note off the screen.
            let until = self.note_until.filter(|&until| until > now);
            return Decision::Sleep { until };
        }
        if let Some(drawn_at) = self.drawn_at {
            let next_frame = drawn_at + self.frame_interval;
            if now < next_frame {
                return Decision::Sleep {
                    until: Some(next_frame),
                };
            }
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

    fn build_scene(&mut self, now: Duration) -> Scene {
        let loudness = match &self.source {
            Source::Starting => LoudnessScene::Starting,
            Source::Failed(reason) => LoudnessScene::Unavailable(reason.clone()),
            Source::Live(analyser) => LoudnessScene::Live(*self.display.get_or_insert_with(|| {
                LoudnessDisplay::new(analyser.sample_rate(), &analyser.readings())
            })),
        };
        let notes = match self.note_until {
            Some(until) if now < until => vec![Note::OutputChanged],
            _ => Vec::new(),
        };
        Scene {
            windows: vec![WindowScene {
                title: "Das-Meter".to_owned(),
                meters: vec![MeterScene::Loudness(loudness)],
            }],
            notes,
        }
    }
}
