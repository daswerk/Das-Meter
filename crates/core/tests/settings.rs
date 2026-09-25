//! Meter menus, the settings panel and app settings: what the egui layer sends,
//! and what the scene shows back. The defaults are the spec's.

use std::time::Duration;

use dasmeter_analysis::signals::{both, frames, sine};
use dasmeter_analysis::{
    ChannelView, PeakHold, RmsMode, SpectrumStyle, StereoScaling, StereoView, WaveformScale,
    WindowFunction,
};
use dasmeter_core::{
    AppCore, AppSettings, Decision, Event, Level, LufsBar, MeterMenu, MeterSettings, MeterState,
    MeterView, Scene, StereoDrawing, WaveformColouring,
};

const RATE: u32 = 48_000;

/// The default row: Waveform, Spectrum, Stereometer, Loudness Meter.
const WAVEFORM: usize = 0;
const SPECTRUM: usize = 1;
const STEREOMETER: usize = 2;
const LOUDNESS: usize = 3;

/// A point inside each Meter of the default row (four equal columns).
fn inside(meter: usize) -> [f32; 2] {
    [(meter as f32 + 0.5) / 4.0, 0.5]
}

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
        app.send(Event::CaptureStarted { sample_rate: RATE });
        app
    }

    /// Started and fed two seconds of a 1 kHz sine at −12 dBFS.
    fn playing() -> App {
        let mut app = App::new();
        app.feed(&both(&sine(RATE, 1_000.0, -12.0, 0.0, frames(RATE, 2.0))));
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

    /// Draws a fresh scene, a frame later.
    fn draw(&mut self) -> &Scene {
        self.now += Duration::from_millis(40);
        assert_eq!(self.core.decide(self.now), Decision::Draw);
        self.core.scene().unwrap()
    }

    fn settings(&self, meter: usize) -> MeterSettings {
        self.core.meter_settings(meter).unwrap()
    }

    /// Changes one Meter's settings the way the menus do: the whole value, edited.
    fn set(&mut self, meter: usize, change: impl FnOnce(&mut MeterSettings)) {
        let mut settings = self.settings(meter);
        change(&mut settings);
        self.send(Event::SetMeter { meter, settings });
    }
}

#[test]
fn every_default_matches_the_spec() {
    let mut app = App::new();
    let scene = app.draw().clone();
    let settings: Vec<MeterSettings> = scene.windows[0].meters.iter().map(|m| m.settings).collect();

    let MeterSettings::Waveform(w) = settings[WAVEFORM] else {
        panic!("{settings:?}")
    };
    assert_eq!(w.analysis.span, Duration::from_secs(4));
    assert_eq!(w.analysis.channel_view, ChannelView::Mono);
    assert_eq!(w.colouring, WaveformColouring::Rgb);
    assert_eq!(w.analysis.scale, WaveformScale::Linear);
    assert_eq!(
        (w.analysis.low_crossover, w.analysis.high_crossover),
        (200.0, 2_000.0)
    );

    let MeterSettings::Spectrum(s) = settings[SPECTRUM] else {
        panic!()
    };
    assert_eq!(s.analysis.channel_view, ChannelView::Mono);
    assert_eq!(s.analysis.slope, 4.5);
    assert!(matches!(s.analysis.style, SpectrumStyle::Line { .. }));
    assert!(s.show_peak_hold);
    assert_eq!(s.analysis.fft_size, 8_192);
    assert_eq!(s.analysis.window, WindowFunction::Hann);
    assert_eq!(s.analysis.frequency_range, (20.0, 20_000.0));
    assert_eq!(s.analysis.db_range, (-90.0, 0.0));

    let MeterSettings::Stereometer(st) = settings[STEREOMETER] else {
        panic!()
    };
    assert_eq!(st.view, StereoView::Polar);
    assert!(!st.show_balance);
    assert_eq!(st.analysis.scaling, StereoScaling::Auto);
    assert_eq!(st.analysis.correlation_time, Duration::from_millis(300));
    assert_eq!(st.correlation_threshold, 0.0);

    let MeterSettings::Loudness(l) = settings[LOUDNESS] else {
        panic!()
    };
    assert_eq!(l.target, Some(-14.0));
    assert_eq!(l.lufs_bar, LufsBar::ShortTerm);
    assert_eq!(l.analysis.rms_window, Duration::from_millis(300));
    assert_eq!(l.analysis.rms_mode, RmsMode::Aes17);
    assert_eq!(l.bar_range, (-60.0, 0.0));
    assert!(l.show_true_peak);
    assert!(l.show_range);

    assert_eq!(
        scene.app,
        AppSettings {
            frame_rate_cap: 60,
            check_for_updates: true
        }
    );
    assert_eq!(scene.menu, None);
    assert!(!scene.settings_open);
}

#[test]
fn right_clicking_a_meter_opens_its_menu_and_a_click_closes_it() {
    let mut app = App::new();
    app.draw();
    app.send(Event::OpenMenu(inside(SPECTRUM)));
    assert_eq!(
        app.draw().menu,
        Some(MeterMenu {
            meter: SPECTRUM,
            at: inside(SPECTRUM)
        })
    );

    // Right-clicking another Meter moves the menu there.
    app.send(Event::OpenMenu(inside(LOUDNESS)));
    assert_eq!(app.draw().menu.map(|m| m.meter), Some(LOUDNESS));

    // A click on the Meters only closes the menu (it doesn't reset the Loudness Meter).
    app.send(Event::Click(inside(LOUDNESS)));
    assert_eq!(app.draw().menu, None);

    app.send(Event::OpenMenu(inside(WAVEFORM)));
    app.draw();
    app.send(Event::CloseMenu);
    assert_eq!(app.draw().menu, None);
}

#[test]
fn the_settings_panel_opens_and_closes_the_menu() {
    let mut app = App::new();
    app.draw();
    app.send(Event::OpenMenu(inside(WAVEFORM)));
    app.draw();
    app.send(Event::ShowSettings(true));
    let scene = app.draw();
    assert!(scene.settings_open);
    assert_eq!(scene.menu, None);
    app.send(Event::ShowSettings(false));
    assert!(!app.draw().settings_open);
}

/// Each basic and advanced setting, set as the menus do, lands in the Meter's
/// settings and in the scene the Meter draws.
#[test]
fn each_setting_changes_the_meter_and_its_scene() {
    let mut app = App::playing();
    app.draw();

    // Waveform.
    app.set(WAVEFORM, |s| {
        let MeterSettings::Waveform(w) = s else {
            unreachable!()
        };
        w.analysis.span = Duration::from_secs(10);
        w.analysis.channel_view = ChannelView::MidSide;
        w.colouring = WaveformColouring::Rekordbox;
        w.analysis.scale = WaveformScale::Decibels;
        w.analysis.gain = 2.0;
        w.analysis.low_crossover = 150.0;
        w.analysis.high_crossover = 3_000.0;
    });
    // Spectrum.
    app.set(SPECTRUM, |s| {
        let MeterSettings::Spectrum(sp) = s else {
            unreachable!()
        };
        sp.analysis.channel_view = ChannelView::LeftRight;
        sp.analysis.slope = 3.0;
        sp.analysis.style = SpectrumStyle::Bars {
            bands_per_octave: 12,
        };
        sp.show_peak_hold = false;
        sp.analysis.fft_size = 4_096;
        sp.analysis.window = WindowFunction::BlackmanHarris;
        sp.analysis.frequency_range = (30.0, 16_000.0);
        sp.analysis.db_range = (-72.0, 6.0);
        sp.analysis.attack = Duration::from_millis(20);
        sp.analysis.release = Duration::from_millis(800);
        sp.analysis.smoothing_octaves = 1.0 / 6.0;
        sp.analysis.peak_hold = PeakHold::Infinite;
    });
    // Stereometer.
    app.set(STEREOMETER, |s| {
        let MeterSettings::Stereometer(st) = s else {
            unreachable!()
        };
        st.view = StereoView::Lissajous;
        st.show_balance = true;
        st.drawing = StereoDrawing::Lines;
        st.persistence = Duration::from_millis(100);
        st.analysis.scaling = StereoScaling::Fixed { gain: 2.0 };
        st.analysis.correlation_time = Duration::from_millis(600);
        st.correlation_threshold = 0.3;
    });
    // Loudness Meter.
    app.set(LOUDNESS, |s| {
        let MeterSettings::Loudness(l) = s else {
            unreachable!()
        };
        l.target = Some(-23.0);
        l.lufs_bar = LufsBar::Momentary;
        l.analysis.rms_window = Duration::from_millis(1_000);
        l.analysis.rms_mode = RmsMode::Plain;
        l.analysis.peak_hold = PeakHold::For(Duration::from_secs(5));
        l.bar_range = (-48.0, 3.0);
        l.show_true_peak = false;
        l.show_range = false;
    });
    let wanted: Vec<MeterSettings> = (0..4).map(|m| app.settings(m)).collect();

    app.feed(&both(&sine(RATE, 1_000.0, -12.0, 0.0, frames(RATE, 1.0))));
    let scene = app.draw().clone();
    for (meter, scene) in scene.windows[0].meters.iter().enumerate() {
        assert_eq!(scene.settings, wanted[meter], "meter {meter}");
        let MeterState::Live(view) = &scene.state else {
            panic!("meter {meter}: {:?}", scene.state)
        };
        let drawn = match view {
            MeterView::Waveform { settings, .. } => MeterSettings::Waveform(*settings),
            MeterView::Spectrum { settings, .. } => MeterSettings::Spectrum(*settings),
            MeterView::Loudness { settings, .. } => MeterSettings::Loudness(*settings),
            MeterView::Stereometer { settings, .. } => MeterSettings::Stereometer(*settings),
        };
        assert_eq!(drawn, wanted[meter], "meter {meter} draws its new settings");
    }
    let MeterState::Live(MeterView::Waveform { traces, .. }) = &scene.windows[0].meters[0].state
    else {
        unreachable!()
    };
    assert_eq!(traces.len(), 2, "M/S draws two traces");
    let MeterState::Live(MeterView::Spectrum {
        spectrum, range, ..
    }) = &scene.windows[0].meters[1].state
    else {
        unreachable!()
    };
    assert_eq!(spectrum.traces.len(), 2, "L+R draws two traces");
    assert_eq!(*range, (30.0, 16_000.0));
}

#[test]
fn settings_are_kept_within_their_ranges() {
    let mut app = App::new();
    app.set(WAVEFORM, |s| {
        let MeterSettings::Waveform(w) = s else {
            unreachable!()
        };
        w.analysis.span = Duration::from_secs(120);
        w.analysis.gain = f32::NAN;
        w.analysis.low_crossover = 900.0;
        w.analysis.high_crossover = 1_000.0;
    });
    let MeterSettings::Waveform(w) = app.settings(WAVEFORM) else {
        unreachable!()
    };
    assert_eq!(w.analysis.span, Duration::from_secs(30));
    assert_eq!(w.analysis.gain, 1.0);
    assert!(w.analysis.high_crossover >= 2.0 * w.analysis.low_crossover);

    app.set(SPECTRUM, |s| {
        let MeterSettings::Spectrum(sp) = s else {
            unreachable!()
        };
        sp.analysis.slope = 5.0;
        sp.analysis.fft_size = 3_000;
        sp.analysis.db_range = (-10.0, -12.0);
        sp.analysis.frequency_range = (5.0, 50_000.0);
        sp.analysis.smoothing_octaves = 1.0;
        sp.analysis.style = SpectrumStyle::Bars {
            bands_per_octave: 5,
        };
    });
    let MeterSettings::Spectrum(sp) = app.settings(SPECTRUM) else {
        unreachable!()
    };
    assert_eq!(sp.analysis.slope, 4.5);
    assert_eq!(sp.analysis.fft_size, 4_096);
    assert!(sp.analysis.db_range.1 - sp.analysis.db_range.0 >= 6.0);
    assert_eq!(sp.analysis.frequency_range, (20.0, 20_000.0));
    assert_eq!(sp.analysis.smoothing_octaves, 1.0 / 3.0);
    assert_eq!(
        sp.analysis.style,
        SpectrumStyle::Bars {
            bands_per_octave: 6
        }
    );

    app.set(LOUDNESS, |s| {
        let MeterSettings::Loudness(l) = s else {
            unreachable!()
        };
        l.target = Some(-99.0);
        l.analysis.rms_window = Duration::ZERO;
    });
    let MeterSettings::Loudness(l) = app.settings(LOUDNESS) else {
        unreachable!()
    };
    assert_eq!(l.target, Some(-40.0));
    assert_eq!(l.analysis.rms_window, Duration::from_millis(50));
}

fn integrated(scene: &Scene) -> Level {
    match &scene.windows[0].meters[LOUDNESS].state {
        MeterState::Live(MeterView::Loudness { display, .. }) => display.integrated,
        other => panic!("{other:?}"),
    }
}

#[test]
fn clicking_the_loudness_meter_resets_integrated_and_the_maxima() {
    let mut app = App::playing();
    assert!(integrated(app.draw()) != Level::Silent);

    // A click on another Meter changes nothing, so nothing is redrawn.
    app.send(Event::Click(inside(SPECTRUM)));
    app.now += Duration::from_millis(40);
    assert!(matches!(app.core.decide(app.now), Decision::Sleep { .. }));

    app.send(Event::Click(inside(LOUDNESS)));
    let scene = app.draw().clone();
    assert_eq!(integrated(&scene), Level::Silent);
    let MeterState::Live(MeterView::Loudness { display, .. }) =
        &scene.windows[0].meters[LOUDNESS].state
    else {
        unreachable!()
    };
    assert_eq!(display.true_peak_max, Level::Silent);
}

#[test]
fn reset_from_the_menu_does_the_same() {
    let mut app = App::playing();
    app.draw();
    app.send(Event::ResetLoudness { meter: LOUDNESS });
    assert_eq!(integrated(app.draw()), Level::Silent);
}

#[test]
fn the_frame_rate_cap_goes_up_to_the_displays_refresh_rate() {
    let mut app = App::playing();
    app.draw();
    // Before the display is known, 60 is the most.
    app.send(Event::SetApp(AppSettings {
        frame_rate_cap: 120,
        check_for_updates: false,
    }));
    let scene = app.draw().clone();
    assert_eq!(scene.app.frame_rate_cap, 60);
    assert!(!scene.app.check_for_updates);
    assert_eq!(scene.max_frame_rate_cap, 60);

    app.send(Event::DisplayRefreshRate(120));
    app.send(Event::SetApp(AppSettings {
        frame_rate_cap: 120,
        check_for_updates: false,
    }));
    assert_eq!(app.draw().max_frame_rate_cap, 120);
    assert_eq!(app.core.app_settings().frame_rate_cap, 120);

    // At 120 fps, draws may come 8.3 ms apart.
    let drawn = app.now;
    let audio = both(&sine(RATE, 1_000.0, -6.0, 0.0, 480));
    app.core.handle(Event::Audio(&audio), drawn);
    assert!(matches!(
        app.core.decide(drawn + Duration::from_millis(8)),
        Decision::Sleep { .. }
    ));
    assert_eq!(
        app.core.decide(drawn + Duration::from_millis(9)),
        Decision::Draw
    );

    // A slower display lowers the cap with it.
    app.send(Event::DisplayRefreshRate(60));
    assert_eq!(app.core.app_settings().frame_rate_cap, 60);
}
