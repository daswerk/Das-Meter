//! The Phase Scope: the waveform over one Cycle, held still against the beat.

use std::time::Duration;

use dasmeter_core::{
    AppCore, CycleLength, Decision, Event, ListenTo, MeterKind, MeterSettings, MeterState,
    MeterView, PhaseScopeView, SendPlugin, SendPluginState, Timing,
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

    /// One Phase Scope on a Send Plugin called Kick.
    fn on_send_plugin() -> App {
        let mut app = App {
            core: AppCore::with_meters(vec![MeterSettings::default_of(MeterKind::PhaseScope)]),
            now: Duration::ZERO,
        };
        app.core.handle(Event::StartListening, app.now);
        app.core
            .handle(Event::SetListenTo(ListenTo::SendPlugins), app.now);
        let kick = [SendPlugin {
            id: 7,
            name: "Kick".to_owned(),
            colour: 0xff_40_20,
            mono: false,
            sample_rate: RATE,
            state: SendPluginState::Live,
            outdated: false,
            host_pid: 1,
        }];
        app.core.handle(Event::SendPlugins(&kick), app.now);
        app
    }

    /// Plays `seconds` from the Send Plugin, 512 frames at a time from
    /// `frame`: `audio` gives each frame's sample, `timing` what the DAW
    /// says at each block's first frame. Returns the frame after the last.
    fn play(
        &mut self,
        frame: u64,
        seconds: f64,
        audio: impl Fn(u64) -> f32,
        timing: impl Fn(u64) -> Option<Timing>,
    ) -> u64 {
        let blocks = (seconds * f64::from(RATE) / 512.0).round() as u64;
        for b in 0..blocks {
            let first = frame + b * 512;
            let stereo: Vec<f32> = (first..first + 512)
                .flat_map(|f| [audio(f), audio(f)])
                .collect();
            self.now += Duration::from_secs_f64(512.0 / f64::from(RATE));
            self.core.handle(
                Event::SendPluginAudio {
                    id: 7,
                    frames: &stereo,
                    timing: timing(first),
                },
                self.now,
            );
        }
        frame + blocks * 512
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

/// A click wherever the song position is `phase` (0–1) into a beat.
fn click_at(beats: f64, phase: f64) -> f32 {
    let into = (beats - phase).rem_euclid(1.0);
    if into < 0.0015 { 0.5 } else { 0.0 }
}

/// The DAW playing at `tempo` from song position `start` at frame `from`.
fn daw(tempo: f64, start: f64, from: u64) -> impl Fn(u64) -> f64 {
    move |frame| start + (frame as f64 - from as f64) / f64::from(RATE) * tempo / 60.0
}

fn playing(beats: f64, tempo: f64, signature: (u16, u16)) -> Option<Timing> {
    let bar = 4.0 / f64::from(signature.1) * f64::from(signature.0);
    Some(Timing {
        tempo,
        beats,
        bar_start: (beats / bar).floor() * bar,
        signature,
        playing: true,
    })
}

/// The column a phase (0–1) into the Cycle falls in.
fn column(phase: f64) -> usize {
    (phase * dasmeter_core::phase_scope::COLUMNS as f64) as usize
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

#[test]
fn it_follows_the_daws_tempo_and_beat() {
    let mut app = App::on_send_plugin();
    // The DAW at 100 BPM; the typed-in tempo (120) is ignored. A click a
    // quarter into each beat.
    let at = daw(100.0, 8.0, 0);
    app.play(
        0,
        4.0,
        |f| click_at(at(f), 0.25),
        |f| playing(at(f), 100.0, (4, 4)),
    );
    let scope = app.scope();
    assert!(scope.following_daw);
    assert_eq!(scope.tempo, 100.0);
    let (got, want) = (loudest_column(&scope), column(0.25));
    assert!(got.abs_diff(want) <= 1, "{got} vs {want}");
}

#[test]
fn a_jump_in_the_song_locks_on_again() {
    let mut app = App::on_send_plugin();
    let at = daw(100.0, 0.0, 0);
    let frame = app.play(
        0,
        3.0,
        |f| click_at(at(f), 0.25),
        |f| playing(at(f), 100.0, (4, 4)),
    );
    // The DAW loops back to 2.6 quarter notes; its clicks stay on its beat.
    let after = daw(100.0, 2.6, frame);
    app.play(
        frame,
        3.0,
        |f| click_at(after(f), 0.25),
        |f| playing(after(f), 100.0, (4, 4)),
    );
    let scope = app.scope();
    let (got, want) = (loudest_column(&scope), column(0.25));
    assert!(got.abs_diff(want) <= 1, "{got} vs {want}");
    // Only Cycles from after the jump: the trail holds no clicks elsewhere.
    for cycle in &scope.trail {
        let max = &cycle[0].max;
        let loudest = (0..max.len())
            .max_by(|&a, &b| max[a].total_cmp(&max[b]))
            .unwrap();
        assert!(loudest.abs_diff(column(0.25)) <= 1);
    }
}

#[test]
fn a_tempo_change_is_followed() {
    let mut app = App::on_send_plugin();
    let at = daw(100.0, 0.0, 0);
    let frame = app.play(
        0,
        3.0,
        |f| click_at(at(f), 0.5),
        |f| playing(at(f), 100.0, (4, 4)),
    );
    let after = daw(140.0, at(frame), frame);
    app.play(
        frame,
        3.0,
        |f| click_at(after(f), 0.5),
        |f| playing(after(f), 140.0, (4, 4)),
    );
    let scope = app.scope();
    assert_eq!(scope.tempo, 140.0);
    let (got, want) = (loudest_column(&scope), column(0.5));
    assert!(got.abs_diff(want) <= 1, "{got} vs {want}");
}

#[test]
fn when_the_daw_stops_it_runs_on_and_locks_on_again_at_play() {
    let mut app = App::on_send_plugin();
    let at = daw(100.0, 0.0, 0);
    let frame = app.play(
        0,
        3.0,
        |f| click_at(at(f), 0.25),
        |f| playing(at(f), 100.0, (4, 4)),
    );

    // Stopped at 5 quarter notes; a synth keeps clicking at 100 BPM.
    let stopped = Timing {
        tempo: 100.0,
        beats: 5.0,
        bar_start: 4.0,
        signature: (4, 4),
        playing: false,
    };
    let frame = app.play(frame, 3.0, |f| click_at(at(f), 0.25), |_| Some(stopped));
    let scope = app.scope();
    assert!(!scope.following_daw);
    assert_eq!(scope.tempo, 100.0, "the last tempo, not the typed-in one");
    let (got, want) = (loudest_column(&scope), column(0.25));
    assert!(got.abs_diff(want) <= 1, "{got} vs {want}");

    // Play from 1.0 at 90 BPM: the Cycle locks to the DAW's beat again.
    let again = daw(90.0, 1.0, frame);
    app.play(
        frame,
        3.0,
        |f| click_at(again(f), 0.75),
        |f| playing(again(f), 90.0, (4, 4)),
    );
    let scope = app.scope();
    assert!(scope.following_daw);
    assert_eq!(scope.tempo, 90.0);
    let (got, want) = (loudest_column(&scope), column(0.75));
    assert!(got.abs_diff(want) <= 1, "{got} vs {want}");
}

#[test]
fn a_bar_cycle_follows_the_daws_time_signature() {
    let mut app = App::on_send_plugin();
    app.set(|s| s.cycle = CycleLength::Bar);
    // 3/4 at 150 BPM, a click on each bar's second beat.
    let at = daw(150.0, 0.0, 0);
    let in_bar = |f: u64| click_at(at(f) / 3.0, 1.0 / 3.0);
    app.play(0, 5.0, in_bar, |f| playing(at(f), 150.0, (3, 4)));
    let scope = app.scope();
    assert_eq!(scope.beat_lines, vec![1.0 / 3.0, 2.0 / 3.0]);
    let (got, want) = (loudest_column(&scope), column(1.0 / 3.0));
    assert!(got.abs_diff(want) <= 1, "{got} vs {want}");
}

#[test]
fn a_send_plugin_without_a_tempo_says_so_and_uses_the_typed_in_one() {
    let mut app = App::on_send_plugin();
    // An older Send Plugin, or a host that gives no transport.
    app.play(0, 3.0, |f| click_at(f as f64 / 24_000.0, 0.5), |_| None);
    let scope = app.scope();
    assert!(!scope.following_daw);
    assert_eq!(scope.tempo, 120.0);
    let note = scope.note.clone().expect("a note");
    assert!(note.contains("No tempo from Kick"), "{note}");
    let (got, want) = (loudest_column(&scope), column(0.5));
    assert!(got.abs_diff(want) <= 1, "{got} vs {want}");
    assert_eq!(
        scope.colour,
        Some(dasmeter_core::Colour::rgb(0xff, 0x40, 0x20)),
        "in the Send Plugin's colour"
    );
}

impl App {
    fn tap(&mut self, after: f64) {
        self.now += Duration::from_secs_f64(after);
        self.core.handle(Event::TapTempo { meter: 0 }, self.now);
    }
}

#[test]
fn tapping_sets_the_tempo() {
    let mut app = App::new();
    app.tap(0.0);
    assert_eq!(app.settings().tempo, 120.0, "one tap alone changes nothing");
    for _ in 0..3 {
        app.tap(0.6);
    }
    assert_eq!(app.settings().tempo, 100.0);
    // A pause starts a fresh count.
    app.tap(3.0);
    app.tap(0.4);
    app.tap(0.4);
    assert_eq!(app.settings().tempo, 150.0);
    // Kept within the limits.
    app.tap(3.0);
    app.tap(0.1);
    let (_, high) = dasmeter_core::settings::PHASE_SCOPE_TEMPO;
    assert_eq!(app.settings().tempo, high);
}

#[test]
fn a_bar_cycle_holds_a_bar_long_pattern_still() {
    let mut app = App::new();
    app.set(|s| s.cycle = CycleLength::Bar);
    // 120 BPM in 4/4 (no DAW to say otherwise): a bar is 96 000 frames. A
    // click a beat and a half into each bar.
    let bar = 96_000;
    let audio = clicks(36_000, bar, bar * 6, 0.5);
    app.feed(&audio[..bar * 3]);
    let scope = app.scope();
    assert_eq!(scope.beat_lines, vec![0.25, 0.5, 0.75]);
    let first = loudest_column(&scope);
    assert!(first.abs_diff(column(1.5 / 4.0)) <= 1, "{first}");
    app.feed(&audio[bar * 3..]);
    assert_eq!(loudest_column(&app.scope()), first);
}

#[test]
fn the_trail_holds_the_previous_few_cycles() {
    let mut app = App::new();
    app.feed(&clicks(1_000, 24_000, 24_000 * 8, 0.5));
    let scope = app.scope();
    assert_eq!(scope.trail.len(), dasmeter_core::phase_scope::TRAIL);
    for cycle in &scope.trail {
        assert_eq!(cycle.len(), 1);
        assert!(cycle[0].max.iter().any(|&v| v > 0.5));
    }
}

#[test]
fn averaging_steadies_a_pattern_and_smooths_a_one_off() {
    let beat = 24_000;
    // A click on every beat, and one extra click 0.7 into the tenth beat.
    let mut audio = clicks(1_000, beat, beat * 11, 0.5);
    let extra = beat * 9 + (0.7 * beat as f64) as usize;
    for sample in &mut audio[extra..extra + 48] {
        *sample = 0.5;
    }
    let one_off = column(0.7);

    let mut sharp = App::new();
    sharp.feed(&audio);
    let scope = sharp.scope();
    assert!(scope.traces[0].max[one_off] > 0.8, "shown sharp");

    let mut steady = App::new();
    steady.set(|s| s.steadiness = dasmeter_core::Steadiness::Average);
    steady.feed(&audio);
    let scope = steady.scope();
    assert!(scope.trail.is_empty());
    let regular = scope.traces[0].max[loudest_column(&scope)];
    assert!(regular > 0.8, "the pattern stays full: {regular}");
    let smoothed = scope.traces[0].max[one_off];
    assert!(
        smoothed < regular / 4.0,
        "the one-off is smoothed: {smoothed}"
    );
}

#[test]
fn every_option_is_kept_within_its_limits() {
    let mut app = App::new();
    app.set(|s| {
        s.gain = 500.0;
        s.cutoff = 5.0;
        s.overlay_offset = -900.0;
    });
    let s = app.settings();
    use dasmeter_core::settings::{PHASE_SCOPE_CUTOFF, PHASE_SCOPE_GAIN, PHASE_SCOPE_OFFSET};
    assert_eq!(s.gain, PHASE_SCOPE_GAIN.1);
    assert_eq!(s.cutoff, PHASE_SCOPE_CUTOFF.0);
    assert_eq!(s.overlay_offset, PHASE_SCOPE_OFFSET.0);
}

#[test]
fn left_and_right_show_two_traces() {
    let mut app = App::new();
    app.set(|s| s.channel_view = dasmeter_analysis::ChannelView::LeftRight);
    app.feed(&clicks(1_000, 24_000, 24_000 * 3, 0.5));
    assert_eq!(app.scope().traces.len(), 2);
}

#[test]
fn mid_and_side_is_not_offered() {
    let mut app = App::new();
    app.set(|s| s.channel_view = dasmeter_analysis::ChannelView::MidSide);
    assert_eq!(
        app.settings().channel_view,
        dasmeter_analysis::ChannelView::Mono
    );
}

#[test]
fn a_cycle_too_long_to_keep_says_so() {
    let mut app = App::new();
    // A 4/4 bar at 30 BPM is 8 seconds: more than the audio kept.
    app.set(|s| {
        s.cycle = CycleLength::Bar;
        s.tempo = 30.0;
    });
    app.feed(&clicks(1_000, 24_000, 24_000 * 2, 0.5));
    let note = app.scope().note.expect("a note");
    assert!(note.contains("too long"), "{note}");
}
