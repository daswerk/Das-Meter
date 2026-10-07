//! The Phase Scope: the waveform over one Cycle, held still against the beat.

use std::time::Duration;

use dasmeter_core::{
    AppCore, Decision, Event, MeterKind, MeterSettings, MeterState, MeterView, PhaseScopeView,
};

const RATE: u32 = 48_000;

struct App {
    core: AppCore,
    now: Duration,
}

impl App {
    /// One Phase Scope on System Capture, started.
    fn new() -> App {
        let mut app = App {
            core: AppCore::with_meters(vec![MeterSettings::default_of(MeterKind::PhaseScope)]),
            now: Duration::ZERO,
        };
        app.core
            .handle(Event::CaptureStarted { sample_rate: RATE }, app.now);
        app.core.handle(Event::StartListening, app.now);
        app
    }

    /// Feeds mono samples as stereo, 512 frames at a time.
    fn feed(&mut self, mono: &[f32]) {
        for block in mono.chunks(512) {
            let stereo: Vec<f32> = block.iter().flat_map(|&x| [x, x]).collect();
            self.now += Duration::from_secs_f64(block.len() as f64 / f64::from(RATE));
            self.core.handle(Event::Audio(&stereo), self.now);
        }
    }

    fn set(&mut self, change: impl FnOnce(&mut dasmeter_core::PhaseScopeMeterSettings)) {
        let Some(MeterSettings::PhaseScope(mut settings)) = self.core.meter_settings(0) else {
            panic!("not a Phase Scope")
        };
        change(&mut settings);
        self.core.handle(
            Event::SetMeter {
                meter: 0,
                settings: MeterSettings::PhaseScope(settings),
            },
            self.now,
        );
    }

    fn settings(&self) -> dasmeter_core::PhaseScopeMeterSettings {
        let Some(MeterSettings::PhaseScope(settings)) = self.core.meter_settings(0) else {
            panic!("not a Phase Scope")
        };
        settings
    }

    /// The scope a frame later, drawn or not.
    fn scope(&mut self) -> PhaseScopeView {
        self.now += Duration::from_millis(20);
        self.core.decide(self.now);
        match &self.core.scene().unwrap().windows[0].meters[0].state {
            MeterState::Live(MeterView::PhaseScope { scope, .. }) => (**scope).clone(),
            other => panic!("expected a live Phase Scope, got {other:?}"),
        }
    }
}

/// A click train: a short pulse every `every` frames, starting at `first`.
fn clicks(first: usize, every: usize, length: usize, level: f32) -> Vec<f32> {
    (0..length)
        .map(|i| {
            if i >= first && (i - first) % every < 48 {
                level
            } else {
                0.0
            }
        })
        .collect()
}

/// The column where the newest Cycle's first trace peaks.
fn loudest_column(scope: &PhaseScopeView) -> usize {
    let max = &scope.traces[0].max;
    (0..max.len())
        .max_by(|&a, &b| max[a].total_cmp(&max[b]))
        .unwrap()
}

#[test]
fn a_click_on_every_beat_stays_in_the_same_place() {
    let mut app = App::new();
    // 120 BPM, the default: a beat is 24 000 frames. A click 1 000 frames in.
    assert_eq!(app.settings().tempo, 120.0);
    let beat = 24_000;
    let audio = clicks(1_000, beat, beat * 8, 0.5);
    app.feed(&audio[..beat * 4]);
    let scope = app.scope();
    assert_eq!(scope.traces.len(), 1, "mono by default");
    let columns = scope.traces[0].max.len();
    assert_eq!(columns, dasmeter_core::phase_scope::COLUMNS);
    let first = loudest_column(&scope);
    let expected = (1_000.0 / beat as f32 * columns as f32) as usize;
    assert!(first.abs_diff(expected) <= 1, "{first} vs {expected}");

    // Cycles later, still the same column.
    app.feed(&audio[beat * 4..]);
    let scope = app.scope();
    assert_eq!(loudest_column(&scope), first);
    assert_eq!(scope.tempo, 120.0);
    assert!(!scope.following_daw);
}

#[test]
fn the_typed_in_tempo_sets_the_cycle() {
    let mut app = App::new();
    app.set(|s| s.tempo = 90.0);
    // At 90 BPM a beat is 32 000 frames: a click every beat stays put, one
    // every 24 000 frames would wander.
    let beat = 32_000;
    let audio = clicks(4_000, beat, beat * 8, 0.5);
    app.feed(&audio[..beat * 4]);
    let first = loudest_column(&app.scope());
    app.feed(&audio[beat * 4..]);
    assert_eq!(loudest_column(&app.scope()), first);
    let expected = (4_000.0 / beat as f32 * dasmeter_core::phase_scope::COLUMNS as f32) as usize;
    assert!(first.abs_diff(expected) <= 1, "{first} vs {expected}");
}

#[test]
fn the_tempo_is_kept_within_its_limits() {
    let mut app = App::new();
    app.set(|s| s.tempo = 1_000.0);
    let (low, high) = dasmeter_core::settings::PHASE_SCOPE_TEMPO;
    assert_eq!(app.settings().tempo, high);
    app.set(|s| s.tempo = f32::NAN);
    assert_eq!(app.settings().tempo, 120.0);
    app.set(|s| s.tempo = 1.0);
    assert_eq!(app.settings().tempo, low);
}

#[test]
fn auto_gain_fills_the_meter_for_quiet_and_loud_audio() {
    for level in [0.01, 0.9] {
        let mut app = App::new();
        app.feed(&clicks(1_000, 24_000, 24_000 * 3, level));
        let scope = app.scope();
        let peak = scope.traces[0].max.iter().copied().fold(0.0, f32::max);
        assert!((0.8..=1.0).contains(&peak), "{level}: {peak}");
    }
}

#[test]
fn it_goes_still_in_silence() {
    let mut app = App::new();
    app.feed(&clicks(1_000, 24_000, 24_000 * 3, 0.5));
    app.scope();
    // Long enough for the trail to empty out too.
    app.feed(&vec![0.0; 24_000 * 12]);
    let scope = app.scope();
    assert!(scope.traces[0].max.iter().all(|&v| v == 0.0));
    // Nothing changes from here on: the app sleeps.
    app.feed(&vec![0.0; 24_000 * 2]);
    app.now += Duration::from_millis(20);
    app.core.decide(app.now);
    app.now += Duration::from_millis(20);
    assert!(matches!(app.core.decide(app.now), Decision::Sleep { .. }));
}

#[test]
fn a_phase_scope_is_saved_in_presets() {
    let settings = MeterSettings::default_of(MeterKind::PhaseScope);
    let text = toml::to_string(&settings).unwrap();
    let back: MeterSettings = toml::from_str(&text).unwrap();
    assert_eq!(back, settings);
}
