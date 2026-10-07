//! The Smooth Look's live feel: a Meter dims gently in silence and brightens
//! when sound comes back, then holds still so the app can sleep.

use std::time::Duration;

use dasmeter_core::{AppCore, Decision, Event, MeterKind, MeterSettings, Styling};

const RATE: u32 = 48_000;
const BLOCK: usize = 480;

struct App {
    core: AppCore,
    now: Duration,
}

impl App {
    /// One Spectrum on System Capture, started.
    fn new() -> App {
        let mut app = App {
            core: AppCore::with_meters(vec![MeterSettings::default_of(MeterKind::Spectrum)]),
            now: Duration::ZERO,
        };
        app.core
            .handle(Event::CaptureStarted { sample_rate: RATE }, app.now);
        app.core.handle(Event::StartListening, app.now);
        app
    }

    /// Plays `seconds` of a tone (or silence), drawing as the shell would.
    /// Returns the activity at each frame drawn.
    fn play(&mut self, seconds: f64, level: f32) -> Vec<f32> {
        let blocks = (seconds * f64::from(RATE) / BLOCK as f64).round() as usize;
        let mut seen = Vec::new();
        for b in 0..blocks {
            let frames: Vec<f32> = (0..BLOCK)
                .flat_map(|i| {
                    let t = (b * BLOCK + i) as f32 / RATE as f32;
                    let x = level * (t * 440.0 * std::f32::consts::TAU).sin();
                    [x, x]
                })
                .collect();
            self.now += Duration::from_secs_f64(BLOCK as f64 / f64::from(RATE));
            self.core.handle(Event::Audio(&frames), self.now);
            if self.core.decide(self.now) == Decision::Draw {
                seen.push(self.activity());
            }
        }
        seen
    }

    fn activity(&self) -> f32 {
        self.core.scene().unwrap().windows[0].meters[0].activity
    }
}

#[test]
fn a_meter_dims_gently_in_silence_and_brightens_with_sound() {
    let mut app = App::new();
    app.play(1.0, 0.5);
    assert_eq!(app.activity(), 1.0, "lit while sound plays");

    // Silence: a short gap changes nothing, then it dims over about a second.
    let fading = app.play(2.5, 0.0);
    assert_eq!(fading[0], 1.0);
    assert_eq!(app.activity(), 0.0, "dim after a while");
    for pair in fading.windows(2) {
        assert!(pair[1] <= pair[0] && pair[0] - pair[1] < 0.1, "{pair:?}");
    }
    let steps = fading.windows(2).filter(|p| p[1] < p[0]).count();
    assert!(steps >= 20, "a gentle fade, not a jump: {steps} steps");

    // Sound comes back: lit again within a fraction of a second.
    app.play(0.3, 0.5);
    assert_eq!(app.activity(), 1.0);
}

#[test]
fn once_dim_it_holds_still_so_the_app_sleeps() {
    let mut app = App::new();
    app.play(0.5, 0.5);
    app.play(30.0, 0.0);
    assert!(matches!(
        app.core.decide(app.now + Duration::from_millis(50)),
        Decision::Sleep { .. }
    ));
}

#[test]
fn without_dimming_the_meter_stays_lit() {
    let mut app = App::new();
    app.play(0.1, 0.5);
    let current = app.core.scene().unwrap().theme.current.unwrap();
    app.core.handle(
        Event::SetThemeStyling {
            theme: current,
            styling: Styling {
                dim_when_silent: false,
                ..Styling::default()
            },
        },
        app.now,
    );
    app.play(0.5, 0.5);
    app.play(3.0, 0.0);
    assert_eq!(app.activity(), 1.0);
}
