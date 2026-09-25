//! Draw or sleep: the app draws only while something changes, and nothing
//! while hidden, but the Loudness Meter keeps counting.

use std::time::Duration;

use dasmeter_analysis::signals::{both, frames, silence, sine};
use dasmeter_core::{AppCore, Decision, Event, Level, MeterState, MeterView, Scene};

const RATE: u32 = 48_000;
const WAVEFORM: usize = 0;
const LOUDNESS: usize = 3;

struct App {
    core: AppCore,
    now: Duration,
}

impl App {
    fn new() -> App {
        let mut app = App {
            core: AppCore::new(),
            now: Duration::ZERO,
        };
        app.core
            .handle(Event::CaptureStarted { sample_rate: RATE }, app.now);
        app
    }

    /// Feeds `seconds` of stereo audio in real time, deciding after each
    /// block as the shell does. Returns how many frames were drawn.
    fn play(&mut self, audio: &[f32]) -> usize {
        let mut drawn = 0;
        for block in audio.chunks(2 * 480) {
            self.now += Duration::from_millis(10);
            self.core.handle(Event::Audio(block), self.now);
            if self.core.decide(self.now) == Decision::Draw {
                drawn += 1;
            }
        }
        drawn
    }

    fn scene(&self) -> &Scene {
        self.core.scene().unwrap()
    }
}

fn tone(seconds: f64) -> Vec<f32> {
    both(&sine(RATE, 1_000.0, -12.0, 0.0, frames(RATE, seconds)))
}

fn quiet(seconds: f64) -> Vec<f32> {
    both(&silence(frames(RATE, seconds)))
}

fn integrated(scene: &Scene) -> Level {
    match &scene.windows[0].meters[LOUDNESS].state {
        MeterState::Live(MeterView::Loudness { display, .. }) => display.integrated,
        other => panic!("{other:?}"),
    }
}

#[test]
fn once_everything_settles_in_silence_nothing_is_drawn() {
    let mut app = App::new();
    app.play(&tone(2.0));
    // Peak holds (2 s) and decays run out within a few seconds of silence.
    app.play(&quiet(6.0));
    assert_eq!(app.play(&quiet(2.0)), 0, "settled: no frames at all");
    assert_eq!(
        app.core.decide(app.now + Duration::from_millis(10)),
        Decision::Sleep { until: None },
        "and no timer: the next audible block wakes it"
    );
}

#[test]
fn a_non_silent_block_wakes_it() {
    let mut app = App::new();
    app.play(&tone(1.0));
    app.play(&quiet(8.0));
    let drawn = app.play(&tone(0.1));
    assert!(drawn >= 1, "the first audible block draws again");
}

#[test]
fn draws_are_capped_at_the_frame_rate() {
    let mut app = App::new();
    // 100 blocks a second in; at most 60 frames a second out.
    let drawn = app.play(&tone(2.0));
    assert!((100..=121).contains(&drawn), "{drawn} frames in 2 s");
}

#[test]
fn nothing_is_drawn_while_hidden_but_loudness_keeps_counting() {
    let mut app = App::new();
    app.play(&quiet(0.5));
    app.core.handle(Event::Visible(false), app.now);
    // The music only plays while the window is hidden.
    assert_eq!(app.play(&tone(5.0)), 0, "hidden: nothing drawn");
    app.core.handle(Event::Visible(true), app.now);
    app.now += Duration::from_millis(20);
    assert_eq!(app.core.decide(app.now), Decision::Draw);
    let scene = app.scene();
    // Integrated LUFS kept integrating over all 5 s: a 1 kHz sine at
    // −12 dBFS on both channels reads −12 LUFS (EBU Tech 3341's reference).
    let lufs = integrated(scene).db().expect("integrated after 5 s");
    assert!((lufs + 12.0).abs() < 0.2, "{lufs}");
    // The Waveform wasn't fed while hidden: it shows none of the music.
    let MeterState::Live(MeterView::Waveform { traces, .. }) =
        &scene.windows[0].meters[WAVEFORM].state
    else {
        panic!()
    };
    assert!(traces[0].iter().all(|c| c.max < 0.01));
}

#[test]
fn peak_holds_still_release_before_it_sleeps() {
    let mut app = App::new();
    app.play(&tone(1.0));
    app.play(&quiet(8.0));
    let scene = app.scene();
    let MeterState::Live(MeterView::Loudness { display, .. }) =
        &scene.windows[0].meters[LOUDNESS].state
    else {
        panic!()
    };
    assert_eq!(display.left.peak_hold, Level::Silent, "the 2 s hold let go");
    assert_eq!(app.play(&quiet(1.0)), 0);
}

#[test]
fn it_settles_only_once_silence_can_change_nothing() {
    let mut app = App::new();
    app.play(&tone(1.0));
    app.play(&quiet(0.05));
    // Between frames nothing may change, but the peak still has to fall.
    assert!(
        !app.core.settled(),
        "the peak and its hold haven't let go yet"
    );
    app.play(&quiet(8.0));
    assert!(app.core.settled());
    app.play(&tone(0.1));
    assert!(!app.core.settled(), "sound wakes it");
}
