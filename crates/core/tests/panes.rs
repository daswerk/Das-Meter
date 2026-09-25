//! Window mode: one window split into panes, one Meter per pane.

use std::time::Duration;

use dasmeter_analysis::signals::{both, frames, sine};
use dasmeter_core::{
    AppCore, Direction, Divider, Event, Frame, LayoutMode, MeterKind, MeterState, MeterView, Rect,
    Scene, WindowKey, WindowScene,
};

const RATE: u32 = 48_000;
const WAVEFORM: usize = 0;
const SPECTRUM: usize = 1;
const STEREOMETER: usize = 2;
const LOUDNESS: usize = 3;

struct App {
    core: AppCore,
    now: Duration,
}

impl App {
    fn window_mode() -> App {
        let mut app = App {
            core: AppCore::new(),
            now: Duration::ZERO,
        };
        app.send(Event::CaptureStarted { sample_rate: RATE });
        app.send(Event::SetMode(LayoutMode::Window));
        app
    }

    fn send(&mut self, event: Event) {
        self.core.handle(event, self.now);
    }

    fn feed(&mut self, seconds: f64) {
        let audio = both(&sine(RATE, 1_000.0, -12.0, 0.0, frames(RATE, seconds)));
        for block in audio.chunks(1024) {
            self.now += Duration::from_secs_f64(512.0 / f64::from(RATE));
            self.core.handle(Event::Audio(block), self.now);
        }
    }

    fn scene(&mut self) -> &Scene {
        self.now += Duration::from_millis(40);
        self.core.decide(self.now);
        self.core.scene().unwrap()
    }

    fn main(&mut self) -> WindowScene {
        let scene = self.scene();
        assert_eq!(scene.windows.len(), 1, "Window mode has one window");
        scene.windows[0].clone()
    }

    /// Each pane's Meter and frame.
    fn panes(&mut self) -> Vec<(usize, Frame)> {
        self.main()
            .meters
            .iter()
            .map(|m| (m.meter, m.frame))
            .collect()
    }

    fn frame_of(&mut self, meter: usize) -> Frame {
        self.panes()
            .into_iter()
            .find(|(m, _)| *m == meter)
            .unwrap()
            .1
    }

    fn divider(&mut self, direction: Direction, contains: Frame) -> Divider {
        self.main()
            .dividers
            .into_iter()
            .find(|d| d.direction == direction && d.area == contains)
            .unwrap_or_else(|| panic!("no {direction:?} divider over {contains:?}"))
    }
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[test]
fn window_mode_starts_as_the_mixing_layout() {
    let mut app = App::window_mode();
    let window = app.main();
    assert_eq!(window.key, WindowKey::Main);
    assert!(!window.on_top);
    // The Spectrum on top, full width; Waveform, Loudness Meter and Stereometer below.
    let spectrum = app.frame_of(SPECTRUM);
    assert!(near(spectrum.width, 1.0) && near(spectrum.height, 0.5));
    let below: Vec<usize> = app
        .panes()
        .into_iter()
        .filter(|(_, f)| near(f.y, 0.5))
        .map(|(m, _)| m)
        .collect();
    assert_eq!(below, [WAVEFORM, LOUDNESS, STEREOMETER]);
}

#[test]
fn a_pane_splits_side_by_side_or_stacked() {
    let mut app = App::window_mode();
    app.send(Event::SplitPane {
        meter: SPECTRUM,
        direction: Direction::SideBySide,
    });
    let panes = app.panes();
    assert_eq!(panes.len(), 5);
    // The new pane is a second Spectrum in the right half of the top.
    let (new, frame) = panes
        .iter()
        .copied()
        .find(|(m, f)| *m != SPECTRUM && near(f.y, 0.0))
        .unwrap();
    assert!(near(frame.x, 0.5) && near(frame.width, 0.5));
    let scene = app.scene().clone();
    let settings = |meter: usize| {
        scene.windows[0]
            .meters
            .iter()
            .find(|m| m.meter == meter)
            .unwrap()
            .settings
    };
    assert_eq!(settings(new), settings(SPECTRUM));

    app.send(Event::SplitPane {
        meter: new,
        direction: Direction::Stacked,
    });
    assert_eq!(app.panes().len(), 6);
    assert!(near(app.frame_of(new).height, 0.25));
}

#[test]
fn a_new_pane_shows_live_audio_straight_away() {
    let mut app = App::window_mode();
    app.feed(1.0);
    app.send(Event::SplitPane {
        meter: LOUDNESS,
        direction: Direction::Stacked,
    });
    app.feed(1.0);
    let scene = app.scene().clone();
    let live = scene.windows[0]
        .meters
        .iter()
        .filter(|m| matches!(m.state, MeterState::Live(MeterView::Loudness { .. })))
        .count();
    assert_eq!(live, 2);
}

#[test]
fn dragging_a_divider_sets_its_ratio() {
    let mut app = App::window_mode();
    let whole = Frame {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    };
    let top = app.divider(Direction::Stacked, whole);
    // The split between Spectrum and the row below, dragged to 70 % down.
    app.send(Event::MoveSplit {
        split: top.split,
        at: 0.7,
    });
    assert!(near(app.frame_of(SPECTRUM).height, 0.7));
    assert!(near(app.frame_of(WAVEFORM).height, 0.3));
    // Ratios keep every pane at least 5 % of its split.
    app.send(Event::MoveSplit {
        split: top.split,
        at: 1.0,
    });
    assert!(near(app.frame_of(SPECTRUM).height, 0.95));
}

#[test]
fn closing_a_pane_gives_its_space_to_the_sibling() {
    let mut app = App::window_mode();
    app.send(Event::ClosePane { meter: STEREOMETER });
    let panes = app.panes();
    assert_eq!(panes.len(), 3);
    // The Loudness Meter had shared its half with the Stereometer.
    let loudness = app.frame_of(LOUDNESS);
    assert!(near(loudness.x, 1.0 / 3.0) && near(loudness.width, 2.0 / 3.0));
    // Closing down to one pane, the last one stays.
    app.send(Event::ClosePane { meter: LOUDNESS });
    app.send(Event::ClosePane { meter: WAVEFORM });
    app.send(Event::ClosePane { meter: SPECTRUM });
    let panes = app.panes();
    assert_eq!(panes.len(), 1);
    assert!(near(panes[0].1.width, 1.0) && near(panes[0].1.height, 1.0));
}

#[test]
fn a_pane_can_show_another_meter() {
    let mut app = App::window_mode();
    app.send(Event::AssignMeter {
        meter: STEREOMETER,
        kind: MeterKind::Spectrum,
    });
    let scene = app.scene().clone();
    let pane = scene.windows[0]
        .meters
        .iter()
        .find(|m| m.meter == STEREOMETER)
        .unwrap();
    assert_eq!(pane.settings.kind(), MeterKind::Spectrum);
}

#[test]
fn switching_modes_keeps_each_layout() {
    let mut app = App::window_mode();
    app.send(Event::ClosePane { meter: STEREOMETER });
    let window_panes = app.panes();

    // Back to the Bar: all four Meters, as they were.
    app.send(Event::SetMode(LayoutMode::Bar));
    let scene = app.scene().clone();
    assert_eq!(scene.windows[0].key, WindowKey::Bar);
    let bar: Vec<usize> = scene.windows[0].meters.iter().map(|m| m.meter).collect();
    assert_eq!(bar, [0, 1, 2, 3]);
    app.send(Event::PopOut { meter: SPECTRUM });
    assert_eq!(app.scene().windows.len(), 2);

    // Window mode again: its panes as left; Pop-outs exist only in Bar mode.
    app.send(Event::SetMode(LayoutMode::Window));
    assert_eq!(app.panes(), window_panes);
    app.send(Event::SetMode(LayoutMode::Bar));
    assert_eq!(app.scene().windows.len(), 2, "the Pop-out is still there");
}

#[test]
fn the_window_has_always_on_top_and_keeps_its_frame() {
    let mut app = App::window_mode();
    app.send(Event::ToggleOnTop);
    assert!(app.main().on_top);
    let frame = Rect {
        x: 100.0,
        y: 80.0,
        width: 900.0,
        height: 600.0,
    };
    app.send(Event::WindowMoved(frame));
    assert_eq!(app.main().frame, Some(frame));
    app.send(Event::ToggleOnTop);
    assert!(!app.main().on_top);
}

#[test]
fn clicks_and_menus_name_the_pane_under_the_pointer() {
    let mut app = App::window_mode();
    app.scene();
    app.send(Event::OpenMenu {
        window: WindowKey::Main,
        at: [0.9, 0.9],
    });
    assert_eq!(app.scene().menu.map(|m| m.meter), Some(STEREOMETER));
}

#[test]
fn only_the_shown_meters_are_fed() {
    let mut app = App::window_mode();
    app.send(Event::ClosePane { meter: WAVEFORM });
    app.feed(0.5);
    app.send(Event::SetMode(LayoutMode::Bar));
    let scene = app.scene().clone();
    // The Waveform was out of the layout: it has nothing to show yet.
    let waveform = &scene.windows[0].meters[WAVEFORM].state;
    let MeterState::Live(MeterView::Waveform { traces, .. }) = waveform else {
        panic!("{waveform:?}")
    };
    assert!(traces.iter().all(|t| t.is_empty()));
}
