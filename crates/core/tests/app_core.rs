//! The app core, run headless with a fake clock: events in, scene and
//! draw-or-sleep decision out.

use std::time::Duration;

use dasmeter_analysis::signals::{both, frames, silence, sine};
use dasmeter_core::{
    AppCore, Decision, Event, Level, LoudnessDisplay, MeterScene, MeterState, MeterView,
    NOTE_DURATION, Note,
};

const RATE: u32 = 48_000;
/// Frames per audio block, as a capture callback delivers them.
const BLOCK: usize = 512;

/// A fake clock plus the core, feeding audio in real-time-sized blocks.
struct Harness {
    core: AppCore,
    now: Duration,
}

impl Harness {
    fn new() -> Harness {
        Harness {
            core: AppCore::new(),
            now: Duration::ZERO,
        }
    }

    fn at(&mut self, now: Duration) -> &mut Harness {
        self.now = now;
        self
    }

    fn send(&mut self, event: Event) -> &mut Harness {
        self.core.handle(event, self.now);
        self
    }

    fn decide(&mut self) -> Decision {
        self.core.decide(self.now)
    }

    /// Feeds interleaved stereo audio block by block, advancing the clock and
    /// asking for a decision after each block and at each wake-up time, as the
    /// shell does. Returns the decisions.
    fn play(&mut self, rate: u32, audio: &[f32]) -> Vec<Decision> {
        let block_time = Duration::from_secs_f64(BLOCK as f64 / f64::from(rate));
        let mut decisions = Vec::new();
        let mut wake_at = None;
        for block in audio.chunks(2 * BLOCK) {
            let next_block = self.now + block_time;
            if let Some(until) = wake_at.filter(|&until| until < next_block) {
                self.now = until;
                decisions.push(self.core.decide(self.now));
            }
            self.now = next_block;
            self.core.handle(Event::Audio(block), self.now);
            let decision = self.core.decide(self.now);
            wake_at = match decision {
                Decision::Sleep { until } => until,
                Decision::Draw => None,
            };
            decisions.push(decision);
        }
        if let Some(until) = wake_at {
            self.now = until;
            decisions.push(self.core.decide(self.now));
        }
        decisions
    }

    fn meters(&self) -> Vec<MeterScene> {
        let scene = self.core.scene().expect("a scene was drawn");
        assert_eq!(scene.windows.len(), 1, "one window");
        scene.windows[0].meters.clone()
    }

    /// The Loudness Meter's state.
    fn loudness(&self) -> MeterState {
        self.meters()
            .into_iter()
            .map(|meter| meter.state)
            .find(|state| match state {
                MeterState::Live(view) => matches!(view, MeterView::Loudness { .. }),
                _ => true,
            })
            .expect("a Loudness Meter")
    }

    fn live(&self) -> LoudnessDisplay {
        match self.loudness() {
            MeterState::Live(MeterView::Loudness { display, .. }) => display,
            other => panic!("expected live readings, got {other:?}"),
        }
    }

    fn notes(&self) -> Vec<Note> {
        self.core.scene().expect("a scene was drawn").notes.clone()
    }
}

/// A stereo 1 kHz sine at `dbfs` on both channels.
fn tone(rate: u32, dbfs: f64, seconds: f64) -> Vec<f32> {
    both(&sine(rate, 1_000.0, dbfs, 0.0, frames(rate, seconds)))
}

fn quiet(rate: u32, seconds: f64) -> Vec<f32> {
    both(&silence(frames(rate, seconds)))
}

#[track_caller]
fn assert_level(what: &str, level: Level, want: f64, tolerance: f64) {
    let got = level.db().unwrap_or_else(|| panic!("{what} is silent"));
    assert!(
        (got - want).abs() <= tolerance,
        "{what}: got {got}, want {want} ±{tolerance}"
    );
}

#[test]
fn before_capture_starts_the_meter_says_so() {
    let mut app = Harness::new();
    assert_eq!(app.decide(), Decision::Draw);
    assert_eq!(app.loudness(), MeterState::Starting);
}

#[test]
fn a_capture_failure_is_shown_in_place_of_the_meter() {
    let mut app = Harness::new();
    app.send(Event::CaptureFailed("No output device"));
    assert_eq!(app.decide(), Decision::Draw);
    assert_eq!(
        app.loudness(),
        MeterState::Unavailable("No output device".to_owned())
    );
}

#[test]
fn audio_in_puts_loudness_readings_in_the_scene() {
    let mut app = Harness::new();
    app.send(Event::CaptureStarted { sample_rate: RATE });
    app.play(RATE, &tone(RATE, -20.0, 4.0));

    let display = app.live();
    assert_eq!(display.sample_rate, RATE);
    // A 1 kHz sine at −20 dBFS on both channels reads −20 LUFS (BS.1770).
    assert_level("momentary", display.momentary, -20.0, 0.2);
    assert_level("short-term", display.short_term, -20.0, 0.2);
    assert_level("integrated", display.integrated, -20.0, 0.2);
    assert_level("true peak", display.true_peak_max, -20.0, 0.2);
    for channel in [display.left, display.right] {
        assert_level("sample peak", channel.peak, -20.0, 0.1);
        assert_level("RMS (AES17)", channel.rms, -20.0, 0.1);
        assert_level("peak hold", channel.peak_hold, -20.0, 0.1);
    }
    assert!(app.notes().is_empty(), "the first start is not a change");
}

#[test]
fn an_output_change_resets_the_readings_with_a_brief_note() {
    let mut app = Harness::new();
    app.send(Event::CaptureStarted { sample_rate: RATE });
    app.play(RATE, &tone(RATE, -20.0, 4.0));
    assert!(app.live().integrated != Level::Silent);

    app.now += Duration::from_millis(20);
    let changed_at = app.now;
    app.send(Event::CaptureStarted {
        sample_rate: 96_000,
    });
    assert_eq!(app.decide(), Decision::Draw);
    let display = app.live();
    assert_eq!(display.sample_rate, 96_000);
    assert_eq!(display.integrated, Level::Silent, "integrated LUFS reset");
    assert_eq!(display.true_peak_max, Level::Silent, "true peak reset");
    assert_eq!(display.left.peak_hold, Level::Silent, "peak hold reset");
    assert_eq!(app.notes(), vec![Note::OutputChanged]);
    assert_eq!(Note::OutputChanged.text(), "Output changed: reset");

    // Nothing else happens: the core sleeps until the note is due to go.
    app.now += Duration::from_millis(100);
    assert_eq!(
        app.decide(),
        Decision::Sleep {
            until: Some(changed_at + NOTE_DURATION)
        }
    );
    app.at(changed_at + NOTE_DURATION);
    assert_eq!(app.decide(), Decision::Draw);
    assert!(app.notes().is_empty(), "the note is gone");
}

#[test]
fn nothing_new_means_sleep() {
    let mut app = Harness::new();
    app.send(Event::CaptureStarted { sample_rate: RATE });
    assert_eq!(app.decide(), Decision::Draw);
    app.now += Duration::from_secs(1);
    assert_eq!(app.decide(), Decision::Sleep { until: None });
}

#[test]
fn draws_stop_once_the_meter_settles_in_silence() {
    let mut app = Harness::new();
    app.send(Event::CaptureStarted { sample_rate: RATE });
    let decisions = app.play(RATE, &tone(RATE, -12.0, 2.0));
    assert!(decisions.contains(&Decision::Draw), "audio is drawn");

    // Loudness, peak holds and the Spectrum take a few seconds to fall away,
    // and the Waveform its whole 4 s span to scroll the tone out.
    app.play(RATE, &quiet(RATE, 8.0));
    let settled = app.play(RATE, &quiet(RATE, 2.0));
    assert!(
        settled
            .iter()
            .all(|d| *d == Decision::Sleep { until: None }),
        "silence after settling draws nothing: {settled:?}"
    );
    let display = app.live();
    assert_eq!(display.momentary, Level::Silent);
    assert_eq!(display.left.rms, Level::Silent);
    assert_eq!(display.left.peak_hold, Level::Silent);
    // The blocks straddling the tone's end pull integrated LUFS down a little.
    assert_level("integrated", display.integrated, -12.0, 0.5);

    // The next non-silent block wakes it.
    let woken = app.play(RATE, &tone(RATE, -12.0, 0.1));
    assert_eq!(woken[0], Decision::Draw);
}

#[test]
fn draws_are_capped_at_60_fps() {
    let mut app = Harness::new();
    app.send(Event::CaptureStarted { sample_rate: RATE });
    let decisions = app.play(RATE, &tone(RATE, -12.0, 1.0));
    let draws = decisions.iter().filter(|d| **d == Decision::Draw).count();
    // 1 s of 512-frame blocks is about 94 blocks; 60 of them draw.
    assert!((58..=61).contains(&draws), "{draws} draws in 1 s");

    // A block arriving 5 ms after a draw waits for the next frame.
    app.now += Duration::from_millis(50);
    app.send(Event::Audio(&tone(RATE, -6.0, 0.01)));
    assert_eq!(app.decide(), Decision::Draw);
    let drawn_at = app.now;
    app.now += Duration::from_millis(5);
    app.send(Event::Audio(&tone(RATE, 0.0, 0.01)));
    assert_eq!(
        app.decide(),
        Decision::Sleep {
            until: Some(drawn_at + Duration::from_nanos(1_000_000_000 / 60))
        }
    );
}

#[test]
fn nothing_is_drawn_while_hidden() {
    let mut app = Harness::new();
    app.send(Event::CaptureStarted { sample_rate: RATE });
    app.decide();
    app.send(Event::Visible(false));
    let decisions = app.play(RATE, &tone(RATE, -12.0, 0.5));
    assert!(
        decisions
            .iter()
            .all(|d| *d == Decision::Sleep { until: None }),
        "{decisions:?}"
    );

    app.send(Event::Visible(true));
    assert_eq!(app.decide(), Decision::Draw);
    assert_level("momentary", app.live().momentary, -12.0, 1.0);
}
