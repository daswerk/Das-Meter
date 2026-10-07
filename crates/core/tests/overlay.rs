//! The Phase Scope's Overlay Source: a second Send Plugin from the same DAW,
//! drawn over the main trace on the same Cycle.

use std::time::Duration;

use dasmeter_core::{
    AppCore, Colour, Event, ListenTo, MeterKind, MeterScene, MeterSettings, MeterState, MeterView,
    PhaseScopeMeterSettings, PhaseScopeView, ScopeTrace, SendPlugin, SendPluginState, Timing,
};

const RATE: u32 = 48_000;
const BLOCK: u64 = 512;
const KICK: u64 = 7;
const BASS: u64 = 8;

fn plugin(id: u64, name: &str, colour: u32, host_pid: u32) -> SendPlugin {
    SendPlugin {
        id,
        name: name.to_owned(),
        colour,
        mono: false,
        sample_rate: RATE,
        state: SendPluginState::Live,
        outdated: false,
        host_pid,
    }
}

/// Kick and Bass in one DAW, Lead in another, and an outdated Pad in the first.
fn listed() -> Vec<SendPlugin> {
    let mut pad = plugin(10, "Pad", 0x80_80_80, 1);
    pad.outdated = true;
    vec![
        plugin(KICK, "Kick", 0xff_40_20, 1),
        plugin(BASS, "Bass", 0x20_80_ff, 1),
        plugin(9, "Lead", 0x20_ff_80, 2),
        pad,
    ]
}

struct App {
    core: AppCore,
    now: Duration,
    listed: Vec<SendPlugin>,
}

impl App {
    /// One Phase Scope showing Kick.
    fn new() -> App {
        let mut app = App {
            core: AppCore::with_meters(vec![MeterSettings::default_of(MeterKind::PhaseScope)]),
            now: Duration::ZERO,
            listed: listed(),
        };
        app.send(Event::StartListening);
        app.send(Event::SetListenTo(ListenTo::SendPlugins));
        app.list();
        app.send(Event::PickSendPlugin { meter: 0, id: KICK });
        app
    }

    fn send(&mut self, event: Event) {
        self.core.handle(event, self.now);
    }

    fn list(&mut self) {
        let listed = self.listed.clone();
        self.send(Event::SendPlugins(&listed));
    }

    fn set(&mut self, change: impl FnOnce(&mut PhaseScopeMeterSettings)) {
        let Some(MeterSettings::PhaseScope(mut settings)) = self.core.meter_settings(0) else {
            panic!("not a Phase Scope")
        };
        change(&mut settings);
        self.send(Event::SetMeter {
            meter: 0,
            settings: MeterSettings::PhaseScope(settings),
        });
    }

    /// Plays `seconds` of the DAW at `tempo` from song position 0. Each
    /// source sends a click `phase` into every beat, only while listened to;
    /// `bass_from` is the song position the Bass starts sending at (a track
    /// whose plugin started later counts its frames from there). The Bass's
    /// blocks reach the app two blocks late, as audio from two tracks
    /// needn't arrive together.
    fn play(&mut self, seconds: f64, tempo: f64, kick_phase: f64, bass_phase: f64, bass_from: f64) {
        let blocks = (seconds * f64::from(RATE) / BLOCK as f64).round() as u64;
        let mut late: std::collections::VecDeque<(Vec<f32>, Option<Timing>)> =
            std::collections::VecDeque::new();
        for b in 0..blocks {
            let first = b * BLOCK;
            let beats = |f: u64| f as f64 / f64::from(RATE) * tempo / 60.0;
            let timing = Some(Timing {
                tempo,
                beats: beats(first),
                bar_start: (beats(first) / 4.0).floor() * 4.0,
                signature: (4, 4),
                playing: true,
            });
            self.now += Duration::from_secs_f64(BLOCK as f64 / f64::from(RATE));
            let listened = self.core.listened();
            for (id, phase) in [(KICK, kick_phase), (BASS, bass_phase)] {
                if !listened.contains(&id) || (id == BASS && beats(first) < bass_from) {
                    continue;
                }
                let stereo: Vec<f32> = (first..first + BLOCK)
                    .flat_map(|f| {
                        let into = (beats(f) - phase).rem_euclid(1.0);
                        let x = if into < 0.0015 { 0.5 } else { 0.0 };
                        [x, x]
                    })
                    .collect();
                if id == BASS {
                    late.push_back((stereo, timing));
                    if late.len() > 2 {
                        let (stereo, timing) = late.pop_front().expect("queued");
                        self.send(Event::SendPluginAudio {
                            id,
                            frames: &stereo,
                            timing,
                        });
                    }
                } else {
                    self.send(Event::SendPluginAudio {
                        id,
                        frames: &stereo,
                        timing,
                    });
                }
            }
        }
    }

    fn meter(&mut self) -> MeterScene {
        self.now += Duration::from_millis(20);
        self.core.decide(self.now);
        self.core.scene().unwrap().windows[0].meters[0].clone()
    }

    fn scope(&mut self) -> PhaseScopeView {
        match self.meter().state {
            MeterState::Live(MeterView::PhaseScope { scope, .. }) => *scope,
            other => panic!("expected a live Phase Scope, got {other:?}"),
        }
    }
}

fn column(phase: f64) -> usize {
    (phase * dasmeter_core::phase_scope::COLUMNS as f64) as usize
}

fn loudest(trace: &ScopeTrace) -> usize {
    let max = &trace.max;
    (0..max.len())
        .max_by(|&a, &b| max[a].total_cmp(&max[b]))
        .unwrap()
}

#[test]
fn the_overlay_list_offers_only_send_plugins_from_the_same_daw() {
    let mut app = App::new();
    let overlay = app
        .meter()
        .overlay
        .expect("a Phase Scope offers an overlay");
    let offered: Vec<u64> = overlay.choices.iter().map(|c| c.id).collect();
    assert_eq!(
        offered,
        [BASS],
        "not itself, another DAW's or an outdated one"
    );
    assert_eq!(overlay.picked, None);

    // Other kinds of Meter offer none.
    app.send(Event::SetMeter {
        meter: 0,
        settings: MeterSettings::default_of(MeterKind::Waveform),
    });
    assert!(app.meter().overlay.is_none());
}

#[test]
fn kick_and_bass_line_up_by_song_position() {
    let mut app = App::new();
    assert_eq!(app.core.listened(), [KICK]);
    app.send(Event::PickOverlay {
        meter: 0,
        id: Some(BASS),
    });
    assert_eq!(app.core.listened(), [KICK, BASS]);
    // The Bass starts sending a few beats in and arrives late, so its
    // frames are counted from a different place: only the song position
    // lines them up.
    app.play(6.0, 100.0, 0.25, 0.5, 2.3);
    let scope = app.scope();
    let overlay = scope.overlay.expect("the overlay");
    assert!(loudest(&scope.traces[0]).abs_diff(column(0.25)) <= 1);
    let at = loudest(&overlay.trace);
    assert!(at.abs_diff(column(0.5)) <= 1, "{at}");
    assert_eq!(overlay.colour, Some(Colour::rgb(0x20, 0x80, 0xff)));
    assert_eq!(scope.colour, Some(Colour::rgb(0xff, 0x40, 0x20)));
    // The sum holds both clicks.
    assert!(
        overlay.sum.max[column(0.25)..column(0.25) + 2]
            .iter()
            .any(|&v| v > 0.3)
    );
    assert!(
        overlay.sum.max[column(0.5)..column(0.5) + 2]
            .iter()
            .any(|&v| v > 0.3)
    );
    assert_eq!(app.meter().overlay.unwrap().picked, Some(BASS));
    // Never more than both together.
    for i in 0..overlay.sum.max.len() {
        let both = scope.traces[0].max[i].max(0.0) + overlay.trace.max[i].max(0.0);
        assert!(
            overlay.sum.max[i] <= both + 0.02,
            "column {i}: {} > {both}",
            overlay.sum.max[i]
        );
    }
}

#[test]
fn the_offset_shifts_the_overlay() {
    let mut app = App::new();
    app.send(Event::PickOverlay {
        meter: 0,
        id: Some(BASS),
    });
    // 10 ms later at 100 BPM (a beat is 600 ms).
    app.set(|s| s.overlay_offset = 10.0);
    app.play(6.0, 100.0, 0.25, 0.5, 0.0);
    let overlay = app.scope().overlay.expect("the overlay");
    let at = loudest(&overlay.trace);
    let want = column(0.5 + 10.0 / 600.0);
    assert!(at.abs_diff(want) <= 1, "{at} vs {want}");
}

#[test]
fn a_gone_overlay_is_waited_for_and_comes_back() {
    let mut app = App::new();
    app.send(Event::PickOverlay {
        meter: 0,
        id: Some(BASS),
    });
    app.play(3.0, 100.0, 0.25, 0.5, 0.0);
    app.listed[1].state = SendPluginState::Gone;
    app.list();
    assert_eq!(app.core.listened(), [KICK]);
    app.play(3.0, 100.0, 0.25, 0.5, 0.0);
    let overlay = app.scope().overlay.expect("still picked");
    assert_eq!(overlay.waiting.as_deref(), Some("Bass"));

    app.listed[1].state = SendPluginState::Live;
    app.list();
    app.play(6.0, 100.0, 0.25, 0.5, 0.0);
    let overlay = app.scope().overlay.expect("the overlay");
    assert_eq!(overlay.waiting, None);
    assert!(loudest(&overlay.trace).abs_diff(column(0.5)) <= 1);
}

#[test]
fn the_overlay_never_follows_other_meters_and_can_be_removed() {
    let mut app = App::new();
    app.send(Event::PickOverlay {
        meter: 0,
        id: Some(BASS),
    });
    // A new pane copies this Meter; that's a copy, not following. Removing
    // the overlay here leaves no overlay.
    app.send(Event::PickOverlay { meter: 0, id: None });
    assert_eq!(app.meter().overlay.unwrap().picked, None);
    assert_eq!(app.core.listened(), [KICK]);
    assert!(app.scope().overlay.is_none());
}

#[test]
fn on_system_capture_no_overlay_can_be_picked() {
    let mut app = App::new();
    app.send(Event::SetListenTo(ListenTo::SystemCapture));
    let overlay = app.meter().overlay.expect("still offered, disabled");
    assert!(overlay.choices.is_empty());
    app.send(Event::PickOverlay {
        meter: 0,
        id: Some(BASS),
    });
    assert_eq!(app.meter().overlay.unwrap().picked, None);
}

impl App {
    /// Plays `seconds` of the DAW at 120 BPM with Kick and Bass both
    /// listened to, each frame's samples given by `kick` and `bass`.
    fn play_both(&mut self, seconds: f64, kick: impl Fn(u64) -> f32, bass: impl Fn(u64) -> f32) {
        let blocks = (seconds * f64::from(RATE) / BLOCK as f64).round() as u64;
        for b in 0..blocks {
            let first = b * BLOCK;
            let beats = first as f64 / f64::from(RATE) * 2.0;
            let timing = Some(Timing {
                tempo: 120.0,
                beats,
                bar_start: (beats / 4.0).floor() * 4.0,
                signature: (4, 4),
                playing: true,
            });
            self.now += Duration::from_secs_f64(BLOCK as f64 / f64::from(RATE));
            for (id, audio) in [(KICK, &kick as &dyn Fn(u64) -> f32), (BASS, &bass)] {
                let stereo: Vec<f32> = (first..first + BLOCK)
                    .flat_map(|f| [audio(f), audio(f)])
                    .collect();
                self.send(Event::SendPluginAudio {
                    id,
                    frames: &stereo,
                    timing,
                });
            }
        }
    }

    fn with_bass() -> App {
        let mut app = App::new();
        app.send(Event::PickOverlay {
            meter: 0,
            id: Some(BASS),
        });
        app
    }
}

fn sine(frequency: f64, level: f32) -> impl Fn(u64) -> f32 {
    move |f| (std::f64::consts::TAU * frequency * f as f64 / f64::from(RATE)).sin() as f32 * level
}

#[test]
fn identical_tracks_dont_cancel() {
    let mut app = App::with_bass();
    app.play_both(4.0, sine(55.0, 0.3), sine(55.0, 0.3));
    let scope = app.scope();
    let overlay = scope.overlay.expect("the overlay");
    assert!(overlay.cancel.iter().all(|&c| c == 0.0), "no shading");
    let correlation = overlay.correlation.expect("a correlation");
    assert!(correlation > 0.98, "{correlation}");
    // The sum is the two traces added.
    for i in 0..overlay.sum.max.len() {
        let both = scope.traces[0].max[i] + overlay.trace.max[i];
        assert!((overlay.sum.max[i] - both).abs() <= 0.03, "column {i}");
    }
}

#[test]
fn an_inverted_copy_cancels_completely() {
    let mut app = App::with_bass();
    let kick = sine(55.0, 0.3);
    app.play_both(4.0, &kick, |f| -kick(f));
    let scope = app.scope();
    let overlay = scope.overlay.expect("the overlay");
    let shaded = overlay.cancel.iter().filter(|&&c| c > 0.9).count();
    assert!(
        shaded > overlay.cancel.len() * 9 / 10,
        "{shaded} columns shaded"
    );
    let correlation = overlay.correlation.expect("a correlation");
    assert!(correlation < -0.98, "{correlation}");
    assert!(
        overlay.sum.max.iter().all(|&v| v.abs() <= 0.02),
        "nothing left"
    );
}

#[test]
fn content_above_the_cutoff_doesnt_move_the_number() {
    let mut app = App::with_bass();
    let (low, high) = (sine(55.0, 0.3), sine(2_000.0, 0.3));
    // The highs are inverted between the two, the lows aren't.
    app.play_both(4.0, |f| low(f) + high(f), |f| low(f) - high(f));
    let correlation = app.scope().overlay.unwrap().correlation.expect("a number");
    assert!(correlation > 0.95, "{correlation}");

    // At the lowest cut-off the 55 Hz sine is only partly let through, but
    // it still decides the number.
    let mut app = App::with_bass();
    app.set(|s| s.cutoff = 40.0);
    app.play_both(4.0, |f| low(f) + high(f), |f| low(f) - high(f));
    let correlation = app.scope().overlay.unwrap().correlation.expect("a number");
    assert!(correlation > 0.9, "{correlation}");
}

#[test]
fn the_number_is_blank_in_silence_or_without_an_overlay() {
    let mut app = App::with_bass();
    app.play_both(4.0, sine(55.0, 0.3), |_| 0.0);
    assert_eq!(app.scope().overlay.unwrap().correlation, None);

    let mut app = App::new();
    app.play_both(4.0, sine(55.0, 0.3), sine(55.0, 0.3));
    assert!(app.scope().overlay.is_none());
}

#[test]
fn an_overlay_from_another_daw_is_never_shown() {
    let mut app = App::with_bass();
    // The main Source moves to Lead, in another DAW: Bass can't be compared.
    app.send(Event::PickSendPlugin { meter: 0, id: 9 });
    assert_eq!(app.core.listened(), [9]);
    let overlay = app.meter().overlay.expect("offered");
    assert!(overlay.choices.is_empty());
}

#[test]
fn a_dc_offset_doesnt_count_as_agreement() {
    let mut app = App::with_bass();
    // Two unrelated lows (55 and 87 Hz), each lifted by the same offset.
    let (a, b) = (sine(55.0, 0.2), sine(87.0, 0.2));
    app.play_both(4.0, |f| a(f) + 0.3, |f| b(f) + 0.3);
    let correlation = app.scope().overlay.unwrap().correlation.expect("a number");
    assert!(correlation.abs() < 0.3, "{correlation}");
}

#[test]
fn with_an_overlay_the_main_trace_is_mono() {
    let mut app = App::with_bass();
    app.set(|s| s.channel_view = dasmeter_analysis::ChannelView::LeftRight);
    app.play_both(3.0, sine(55.0, 0.3), sine(55.0, 0.3));
    assert_eq!(app.scope().traces.len(), 1);
}
