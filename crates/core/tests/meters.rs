//! The four Meters in the scene: each carries its settings and draw data, and
//! changing a setting changes the scene.

use std::time::Duration;

use dasmeter_analysis::signals::{both, frames, sine, stereo};
use dasmeter_analysis::{ChannelView, SpectrumStyle, StereoView};
use dasmeter_core::{
    AppCore, Decision, Event, Frame, LufsBar, MeterSettings, MeterState, MeterView, Role,
    StereoDrawing, WindowKey,
};

const RATE: u32 = 48_000;

/// The default row: Waveform, Spectrum, Stereometer, Loudness Meter.
const WAVEFORM: usize = 0;
const SPECTRUM: usize = 1;
const STEREOMETER: usize = 2;
const LOUDNESS: usize = 3;

struct App {
    core: AppCore,
    now: Duration,
}

impl App {
    /// The default core, started and fed two seconds of a stereo 1 kHz sine at −12 dBFS.
    fn playing() -> App {
        let mut app = App {
            core: AppCore::new(),
            now: Duration::ZERO,
        };
        app.core
            .handle(Event::CaptureStarted { sample_rate: RATE }, app.now);
        let audio = both(&sine(RATE, 1_000.0, -12.0, 0.0, frames(RATE, 2.0)));
        app.feed(&audio);
        app
    }

    fn feed(&mut self, audio: &[f32]) {
        for block in audio.chunks(1024) {
            self.now += Duration::from_secs_f64(512.0 / f64::from(RATE));
            self.core.handle(Event::Audio(block), self.now);
        }
    }

    fn send(&mut self, event: Event) {
        self.core.handle(event, self.now);
    }

    /// Draws a fresh scene (a frame later) and returns each Meter's view.
    fn draw(&mut self) -> Vec<MeterView> {
        self.now += Duration::from_millis(20);
        assert_eq!(self.core.decide(self.now), Decision::Draw);
        self.views()
    }

    fn views(&self) -> Vec<MeterView> {
        self.core.scene().unwrap().windows[0]
            .meters
            .iter()
            .map(|meter| match &meter.state {
                MeterState::Live(view) => view.clone(),
                other => panic!("expected a live Meter, got {other:?}"),
            })
            .collect()
    }

    fn frames(&self) -> Vec<Frame> {
        self.core.scene().unwrap().windows[0]
            .meters
            .iter()
            .map(|meter| meter.frame)
            .collect()
    }

    fn set(&mut self, meter: usize, change: impl FnOnce(&mut MeterSettings)) {
        let mut settings = self.core.meter_settings(meter).unwrap();
        change(&mut settings);
        self.send(Event::SetMeter { meter, settings });
    }
}

#[test]
fn the_default_window_is_a_row_of_the_four_meters() {
    let mut app = App::playing();
    let views = app.draw();
    assert!(matches!(views[WAVEFORM], MeterView::Waveform { .. }));
    assert!(matches!(views[SPECTRUM], MeterView::Spectrum { .. }));
    assert!(matches!(views[STEREOMETER], MeterView::Stereometer { .. }));
    assert!(matches!(views[LOUDNESS], MeterView::Loudness { .. }));

    let frames = app.frames();
    for (i, frame) in frames.iter().enumerate() {
        assert_eq!(frame.x, i as f32 * 0.25);
        assert_eq!((frame.width, frame.y, frame.height), (0.25, 0.0, 1.0));
    }
}

#[test]
fn each_meter_carries_its_draw_data() {
    let mut app = App::playing();
    let views = app.draw();

    let MeterView::Waveform { traces, .. } = &views[WAVEFORM] else {
        unreachable!()
    };
    assert_eq!(traces.len(), 1, "Mono by default");
    // Two seconds at 200 columns per second.
    assert!(
        (398..=400).contains(&traces[0].len()),
        "{}",
        traces[0].len()
    );
    let newest = traces[0].last().unwrap();
    assert!((newest.max - 0.251).abs() < 0.01, "peak {}", newest.max);
    assert!(
        newest.mid > newest.low && newest.mid > newest.high,
        "1 kHz is mid band"
    );

    let MeterView::Spectrum {
        spectrum, range, ..
    } = &views[SPECTRUM]
    else {
        unreachable!()
    };
    assert_eq!(*range, (20.0, 20_000.0));
    let levels = &spectrum.traces[0].levels;
    let loudest = (0..levels.len())
        .max_by(|&a, &b| levels[a].total_cmp(&levels[b]))
        .unwrap();
    let peak = spectrum.frequencies[loudest];
    assert!(
        (900.0..1_100.0).contains(&peak),
        "Spectrum peaks at {peak} Hz"
    );

    let MeterView::Stereometer {
        readings, points, ..
    } = &views[STEREOMETER]
    else {
        unreachable!()
    };
    assert_eq!(readings.correlation, 1.0, "identical channels");
    assert!(!points.is_empty());
    assert!(
        points.iter().all(|[x, _]| x.abs() < 1e-3),
        "mono points stand straight up"
    );

    let MeterView::Loudness { display, settings } = &views[LOUDNESS] else {
        unreachable!()
    };
    assert_eq!(settings.target, Some(-14.0), "default target");
    assert_eq!(settings.lufs_bar, LufsBar::ShortTerm);
    assert!(display.momentary.db().is_some());
}

#[test]
fn changing_a_setting_changes_the_scene() {
    let mut app = App::playing();
    app.draw();

    app.set(WAVEFORM, |s| {
        if let MeterSettings::Waveform(w) = s {
            w.analysis.channel_view = ChannelView::LeftRight;
        }
    });
    app.set(SPECTRUM, |s| {
        if let MeterSettings::Spectrum(sp) = s {
            sp.analysis.style = SpectrumStyle::Bars {
                bands_per_octave: 3,
            };
        }
    });
    app.set(STEREOMETER, |s| {
        if let MeterSettings::Stereometer(st) = s {
            st.view = StereoView::Lissajous;
            st.drawing = StereoDrawing::Lines;
        }
    });
    app.set(LOUDNESS, |s| {
        if let MeterSettings::Loudness(l) = s {
            l.target = None;
            l.lufs_bar = LufsBar::Momentary;
        }
    });
    // Audio after the change fills the restarted analysers.
    let left_only = stereo(
        &sine(RATE, 1_000.0, -12.0, 0.0, frames(RATE, 1.0)),
        &vec![0.0; frames(RATE, 1.0)],
    );
    app.feed(&left_only);
    let views = app.draw();

    let MeterView::Waveform {
        settings, traces, ..
    } = &views[WAVEFORM]
    else {
        unreachable!()
    };
    assert_eq!(settings.analysis.channel_view, ChannelView::LeftRight);
    assert_eq!(traces.len(), 2, "L above, R below");
    assert!(
        traces[1].last().unwrap().max.abs() < 1e-6,
        "right is silent"
    );

    let MeterView::Spectrum {
        settings, spectrum, ..
    } = &views[SPECTRUM]
    else {
        unreachable!()
    };
    assert!(matches!(
        settings.analysis.style,
        SpectrumStyle::Bars { .. }
    ));
    let bands = spectrum.frequencies.len();
    assert!(
        (28..=31).contains(&bands),
        "{bands} third-octave bands instead of 512 line points"
    );

    let MeterView::Stereometer {
        settings, readings, ..
    } = &views[STEREOMETER]
    else {
        unreachable!()
    };
    assert_eq!(settings.view, StereoView::Lissajous);
    assert_eq!(readings.balance, -1.0, "all left");

    let MeterView::Loudness { settings, .. } = &views[LOUDNESS] else {
        unreachable!()
    };
    assert_eq!(settings.target, None);
    assert_eq!(settings.lufs_bar, LufsBar::Momentary);
}

#[test]
fn a_setting_change_alone_redraws() {
    let mut app = App::playing();
    app.draw();
    app.now += Duration::from_secs(1);
    assert_eq!(app.core.decide(app.now), Decision::Sleep { until: None });
    app.set(LOUDNESS, |s| {
        if let MeterSettings::Loudness(l) = s {
            l.target = Some(-23.0);
        }
    });
    let views = app.draw();
    let MeterView::Loudness { settings, .. } = &views[LOUDNESS] else {
        unreachable!()
    };
    assert_eq!(settings.target, Some(-23.0));
}

#[test]
fn the_spectrum_shows_frequency_and_note_under_the_cursor() {
    let mut app = App::playing();
    app.draw();
    // Halfway across the Spectrum (the second quarter of the window) on a
    // 20 Hz–20 kHz log axis is √(20 · 20000) ≈ 632 Hz, nearest D#5.
    app.send(Event::Pointer(Some((WindowKey::Bar, [0.375, 0.5]))));
    let views = app.draw();
    let MeterView::Spectrum { cursor, .. } = &views[SPECTRUM] else {
        unreachable!()
    };
    let cursor = cursor.expect("a readout under the cursor");
    assert!((cursor.x - 0.5).abs() < 1e-6);
    assert!(
        (cursor.frequency - 632.5).abs() < 1.0,
        "{}",
        cursor.frequency
    );
    assert_eq!(cursor.note.unwrap().to_string(), "D#5");

    // Over another Meter, or outside the window: no readout.
    app.send(Event::Pointer(Some((WindowKey::Bar, [0.9, 0.5]))));
    let views = app.draw();
    assert!(matches!(
        views[SPECTRUM],
        MeterView::Spectrum { cursor: None, .. }
    ));
    app.send(Event::Pointer(Some((WindowKey::Bar, [0.375, 0.5]))));
    app.draw();
    app.send(Event::Pointer(None));
    let views = app.draw();
    assert!(matches!(
        views[SPECTRUM],
        MeterView::Spectrum { cursor: None, .. }
    ));
}

#[test]
fn colours_come_from_the_palette_roles() {
    let mut app = App::playing();
    app.draw();
    let palette = &app.core.scene().unwrap().palette;
    // Every role has a colour, and negative correlation stands out from positive.
    assert_ne!(
        palette[Role::CorrelationPositive],
        palette[Role::CorrelationNegative]
    );
    assert_ne!(
        palette[Role::LoudnessBar],
        palette[Role::LoudnessOverTarget]
    );
}

fn loudness(views: &[MeterView]) -> &dasmeter_core::LoudnessDisplay {
    match &views[LOUDNESS] {
        MeterView::Loudness { display, .. } => display,
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_loudness_graph_shows_the_bars_reading_over_its_span() {
    let mut app = App::playing();
    let views = app.draw();
    let display = loudness(&views);
    // Two seconds of audio: about 20 points. The newest is short-term LUFS
    // over its 3 s window, two thirds full: −12 + 10·log10(2/3) ≈ −13.8.
    assert!(
        (18..=21).contains(&display.history.len()),
        "{}",
        display.history.len()
    );
    let newest = display.history.last().unwrap().db().unwrap();
    assert!((newest + 13.76).abs() < 0.3, "{newest}");

    // Only the span is kept: 10 s of a 40 s run.
    app.set(LOUDNESS, |s| {
        if let MeterSettings::Loudness(s) = s {
            s.history_span = Duration::from_secs(10);
        }
    });
    app.feed(&both(&sine(RATE, 1_000.0, -12.0, 0.0, frames(RATE, 40.0))));
    assert_eq!(loudness(&app.draw()).history.len(), 100);

    // Momentary on the bar: the graph follows it.
    app.set(LOUDNESS, |s| {
        if let MeterSettings::Loudness(s) = s {
            s.lufs_bar = LufsBar::Momentary;
        }
    });
    app.feed(&both(&sine(RATE, 1_000.0, -30.0, 0.0, frames(RATE, 0.5))));
    let newest = loudness(&app.draw()).history.last().unwrap().db().unwrap();
    assert!(newest < -25.0, "momentary drops fast: {newest}");

    // Off: no points in the scene at all.
    app.set(LOUDNESS, |s| {
        if let MeterSettings::Loudness(s) = s {
            s.show_history = false;
        }
    });
    app.feed(&both(&sine(RATE, 1_000.0, -12.0, 0.0, frames(RATE, 0.2))));
    assert!(loudness(&app.draw()).history.is_empty());
}

#[test]
fn a_sine_shows_no_peak_to_loudness_headroom() {
    let mut app = App::playing();
    app.feed(&both(&sine(RATE, 1_000.0, -12.0, 0.0, frames(RATE, 3.0))));
    let views = app.draw();
    let display = loudness(&views);
    for (name, ratio) in [("PLR", display.plr), ("PSR", display.psr)] {
        let lu = ratio.db().unwrap_or_else(|| panic!("{name} is shown"));
        assert!(lu.abs() < 0.5, "{name}: {lu}");
    }
}

#[test]
fn the_graph_span_is_kept_within_its_limits() {
    let mut app = App::playing();
    app.set(LOUDNESS, |s| {
        if let MeterSettings::Loudness(s) = s {
            s.history_span = Duration::from_secs(3_600);
        }
    });
    let Some(MeterSettings::Loudness(s)) = app.core.meter_settings(LOUDNESS) else {
        panic!()
    };
    assert_eq!(s.history_span, Duration::from_secs(120));
}

#[test]
fn the_peak_line_follows_the_loudest_peak_and_can_be_turned_off() {
    let mut app = App::playing();
    let views = app.draw();
    let MeterView::Spectrum { peak, range, .. } = &views[SPECTRUM] else {
        panic!()
    };
    let peak = peak.expect("on by default, and the sine is loud");
    assert!((peak.frequency - 1_000.0).abs() < 1.0, "{}", peak.frequency);
    let note = peak.note.expect("a note");
    assert_eq!(note.to_string(), "B5");
    // Where 1 kHz sits between the edges, on the log axis.
    let x = (1_000.0f32 / range.0).ln() / (range.1 / range.0).ln();
    assert!((peak.x - x).abs() < 0.001);

    app.set(SPECTRUM, |s| {
        if let MeterSettings::Spectrum(s) = s {
            s.show_peak_line = false;
        }
    });
    app.feed(&both(&sine(RATE, 1_000.0, -12.0, 0.0, frames(RATE, 0.2))));
    let views = app.draw();
    let MeterView::Spectrum { peak, .. } = &views[SPECTRUM] else {
        panic!()
    };
    assert_eq!(*peak, None);
}

#[test]
fn a_cepstrum_meter_finds_the_pitch_of_a_harmonic_tone() {
    let mut app = App::playing();
    app.send(Event::SetMeter {
        meter: SPECTRUM,
        settings: MeterSettings::default_of(dasmeter_core::MeterKind::Cepstrum),
    });
    // A band-limited sawtooth at 110 Hz.
    let tone: Vec<f32> = (0..frames(RATE, 1.0))
        .map(|i| {
            let t = i as f64 / f64::from(RATE);
            let x: f64 = (1..=200)
                .map(|k| (std::f64::consts::TAU * 110.0 * f64::from(k) * t).sin() / f64::from(k))
                .sum();
            (0.15 * x) as f32
        })
        .collect();
    app.feed(&both(&tone));
    let views = app.draw();
    let MeterView::Cepstrum {
        values,
        quefrency_range,
        pitch,
        ..
    } = &views[SPECTRUM]
    else {
        panic!("{:?}", views[SPECTRUM])
    };
    assert_eq!(values.len(), dasmeter_analysis::cepstrum::POINTS);
    let pitch = pitch.expect("a pitch");
    assert!((pitch.frequency - 110.0).abs() < 1.0, "{}", pitch.frequency);
    assert_eq!(pitch.note.unwrap().to_string(), "A2");
    // Its line sits at its period between the edges.
    let (short, long) = *quefrency_range;
    let x = (1.0 / 110.0 - short) / (long - short);
    assert!((pitch.x - x).abs() < 0.01, "{} vs {x}", pitch.x);

    // Show pitch off: no readout.
    app.set(SPECTRUM, |s| {
        if let MeterSettings::Cepstrum(s) = s {
            s.show_pitch = false;
        }
    });
    app.feed(&both(&tone[..4_800]));
    let views = app.draw();
    let MeterView::Cepstrum { pitch, .. } = &views[SPECTRUM] else {
        panic!()
    };
    assert_eq!(*pitch, None);
}

#[test]
fn cepstrum_settings_are_kept_within_their_limits() {
    let mut app = App::playing();
    let mut settings = dasmeter_core::CepstrumMeterSettings::default();
    settings.analysis.fft_size = 3_000;
    settings.analysis.pitch_range = (900.0, 1_000.0);
    app.send(Event::SetMeter {
        meter: SPECTRUM,
        settings: MeterSettings::Cepstrum(settings),
    });
    let Some(MeterSettings::Cepstrum(kept)) = app.core.meter_settings(SPECTRUM) else {
        panic!()
    };
    assert_eq!(kept.analysis.fft_size, 4_096, "an FFT size it offers");
    let (low, high) = kept.analysis.pitch_range;
    assert!(high >= 2.0 * low, "at least an octave: {low}–{high}");
}
