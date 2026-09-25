//! Bar mode: edges, thickness, shares, Pop-outs, the screen button and
//! fullscreen apps, as the scene's windows show them.

use std::time::Duration;

use dasmeter_core::{
    AppCore, Decision, Display, Edge, Event, Platform, Rect, Scene, ScreenMode, WindowKey,
    WindowScene,
};

/// A 1512 × 982 laptop display with a 33 px menu bar and a 70 px Dock at the bottom.
const DISPLAY: Display = Display {
    frame: Rect {
        x: 0.0,
        y: 0.0,
        width: 1512.0,
        height: 982.0,
    },
    usable: Rect {
        x: 0.0,
        y: 33.0,
        width: 1512.0,
        height: 879.0,
    },
};

const SPECTRUM: usize = 1;
const LOUDNESS: usize = 3;

struct App {
    core: AppCore,
    now: Duration,
}

impl App {
    fn on(platform: Platform) -> App {
        let mut app = App {
            core: AppCore::new().on_platform(platform),
            now: Duration::ZERO,
        };
        app.send(Event::Display(DISPLAY));
        app
    }

    fn mac() -> App {
        App::on(Platform::MacOs)
    }

    fn send(&mut self, event: Event) {
        self.core.handle(event, self.now);
    }

    /// Draws a fresh scene a frame later, or returns the last one if nothing changed.
    fn scene(&mut self) -> &Scene {
        self.now += Duration::from_millis(40);
        self.core.decide(self.now);
        self.core.scene().unwrap()
    }

    fn window(&mut self, key: WindowKey) -> Option<WindowScene> {
        self.scene().windows.iter().find(|w| w.key == key).cloned()
    }

    fn bar(&mut self) -> WindowScene {
        self.window(WindowKey::Bar).unwrap()
    }

    /// The Meters in the Bar, in order, with their widths (fractions).
    fn bar_meters(&mut self) -> Vec<(usize, f32)> {
        self.bar()
            .meters
            .iter()
            .map(|m| (m.meter, m.frame.width))
            .collect()
    }
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-4
}

#[test]
fn the_bar_docks_to_the_bottom_inside_the_usable_area() {
    let mut app = App::mac();
    let bar = app.bar();
    let frame = bar.frame.unwrap();
    assert_eq!(bar.edge, Some(Edge::Bottom));
    assert_eq!(
        frame,
        Rect {
            x: 0.0,
            y: 912.0 - 180.0,
            width: 1512.0,
            height: 180.0
        },
        "on the Dock's edge, not over it"
    );
    // All four Meters, side by side, equal shares.
    let meters = app.bar_meters();
    assert_eq!(meters.iter().map(|m| m.0).collect::<Vec<_>>(), [0, 1, 2, 3]);
    assert!(meters.iter().all(|m| near(m.1, 0.25)));
}

#[test]
fn before_the_display_is_known_the_bar_has_no_frame() {
    let mut app = App {
        core: AppCore::new(),
        now: Duration::ZERO,
    };
    assert_eq!(app.bar().frame, None);
}

#[test]
fn the_bar_docks_to_any_edge() {
    let mut app = App::mac();
    let cases = [
        (
            Edge::Top,
            Rect {
                x: 0.0,
                y: 33.0,
                width: 1512.0,
                height: 180.0,
            },
        ),
        (
            Edge::Left,
            Rect {
                x: 0.0,
                y: 33.0,
                width: 180.0,
                height: 879.0,
            },
        ),
        (
            Edge::Right,
            Rect {
                x: 1512.0 - 180.0,
                y: 33.0,
                width: 180.0,
                height: 879.0,
            },
        ),
    ];
    for (edge, want) in cases {
        app.send(Event::SetEdge(edge));
        let bar = app.bar();
        assert_eq!(bar.frame, Some(want), "{edge:?}");
        assert_eq!(bar.edge, Some(edge));
    }
    // Along a side edge the Meters stack top to bottom.
    let bar = app.bar();
    assert!(bar.meters.iter().all(|m| m.frame.width == 1.0));
    assert!(near(bar.meters[1].frame.y, 0.25));
}

#[test]
fn the_thickness_is_capped_at_a_third_of_the_display() {
    let mut app = App::mac();
    app.send(Event::SetBarThickness(240.0));
    assert_eq!(app.bar().frame.unwrap().height, 240.0);
    app.send(Event::SetBarThickness(900.0));
    assert!(near(app.bar().frame.unwrap().height, 982.0 / 3.0));
    // Too thin is kept at the minimum.
    app.send(Event::SetBarThickness(5.0));
    assert_eq!(app.bar().frame.unwrap().height, 60.0);
    // On a side edge the cap is a third of the width.
    app.send(Event::SetBarThickness(900.0));
    app.send(Event::SetEdge(Edge::Left));
    assert!(near(app.bar().frame.unwrap().width, 982.0 / 3.0));
}

#[test]
fn dragging_a_divider_changes_two_shares_as_ratios() {
    let mut app = App::mac();
    // The divider between Waveform and Spectrum, dragged to 40 % of the Bar.
    app.send(Event::MoveDivider {
        divider: 0,
        at: 0.4,
    });
    let meters = app.bar_meters();
    assert!(
        near(meters[0].1, 0.4) && near(meters[1].1, 0.1),
        "{meters:?}"
    );
    assert!(near(meters[2].1, 0.25) && near(meters[3].1, 0.25));
    // A Meter keeps at least 5 %.
    app.send(Event::MoveDivider {
        divider: 0,
        at: 0.49,
    });
    let meters = app.bar_meters();
    assert!(near(meters[1].1, 0.05), "{meters:?}");
    // Ratios: the Bar's frames follow a thickness or edge change unchanged.
    app.send(Event::SetEdge(Edge::Top));
    assert_eq!(app.bar_meters(), meters);
}

#[test]
fn a_meter_pops_out_into_its_own_window_and_docks_back() {
    let mut app = App::mac();
    app.send(Event::PopOut { meter: SPECTRUM });
    let pop_out = app.window(WindowKey::PopOut(SPECTRUM)).unwrap();
    let frame = pop_out.frame.unwrap();
    assert_eq!(pop_out.meters.len(), 1);
    assert_eq!(pop_out.meters[0].meter, SPECTRUM);
    assert!(!pop_out.on_top);
    // Above the Bar, where the Spectrum was, inside the usable area.
    assert!(frame.bottom() <= 912.0 - 180.0);
    assert!(frame.y >= 33.0);
    // The other three share the Bar.
    let meters = app.bar_meters();
    assert_eq!(meters.iter().map(|m| m.0).collect::<Vec<_>>(), [0, 2, 3]);
    assert!(meters.iter().all(|m| near(m.1, 1.0 / 3.0)));

    // Moving and resizing it is kept.
    let moved = Rect {
        x: 300.0,
        y: 200.0,
        width: 600.0,
        height: 400.0,
    };
    app.send(Event::PopOutMoved {
        meter: SPECTRUM,
        frame: moved,
    });
    assert_eq!(
        app.window(WindowKey::PopOut(SPECTRUM)).unwrap().frame,
        Some(moved)
    );
    app.send(Event::SetPopOutOnTop {
        meter: SPECTRUM,
        on_top: true,
    });
    assert!(app.window(WindowKey::PopOut(SPECTRUM)).unwrap().on_top);

    app.send(Event::DockBack { meter: SPECTRUM });
    assert_eq!(app.window(WindowKey::PopOut(SPECTRUM)), None);
    let meters = app.bar_meters();
    assert_eq!(
        meters.iter().map(|m| m.0).collect::<Vec<_>>(),
        [0, 1, 2, 3],
        "back in its place"
    );
    assert!(near(meters.iter().map(|m| m.1).sum(), 1.0));
}

#[test]
fn the_last_meter_in_the_bar_stays() {
    let mut app = App::mac();
    for meter in 0..3 {
        app.send(Event::PopOut { meter });
    }
    app.send(Event::PopOut { meter: LOUDNESS });
    assert_eq!(app.bar_meters().len(), 1);
    assert_eq!(app.scene().windows.len(), 4);
}

#[test]
fn a_pop_out_takes_clicks_and_menus_for_its_meter() {
    let mut app = App::mac();
    app.send(Event::PopOut { meter: LOUDNESS });
    app.scene();
    app.send(Event::OpenMenu {
        window: WindowKey::PopOut(LOUDNESS),
        at: [0.5, 0.5],
    });
    let menu = app.scene().menu.unwrap();
    assert_eq!(
        (menu.meter, menu.window),
        (LOUDNESS, WindowKey::PopOut(LOUDNESS))
    );
    // Docking it back closes its menu.
    app.send(Event::DockBack { meter: LOUDNESS });
    assert_eq!(app.scene().menu, None);
}

#[test]
fn the_screen_button_on_macos_skips_reserve_space() {
    let mut app = App::mac();
    let bar = app.bar();
    assert_eq!(bar.screen, Some(ScreenMode::FloatOnTop));
    assert!(bar.on_top && !bar.reserve_space);
    app.send(Event::CycleScreenMode);
    let bar = app.bar();
    assert_eq!(bar.screen, Some(ScreenMode::NormalWindow));
    assert!(!bar.on_top);
    app.send(Event::CycleScreenMode);
    assert_eq!(app.bar().screen, Some(ScreenMode::FloatOnTop));
}

#[test]
fn the_screen_button_on_windows_cycles_all_three() {
    let mut app = App::on(Platform::Windows);
    let mut seen = Vec::new();
    for _ in 0..4 {
        let bar = app.bar();
        seen.push((bar.screen.unwrap(), bar.on_top, bar.reserve_space));
        app.send(Event::CycleScreenMode);
    }
    assert_eq!(
        seen,
        [
            (ScreenMode::ReserveSpace, true, true),
            (ScreenMode::FloatOnTop, true, false),
            (ScreenMode::NormalWindow, false, false),
            (ScreenMode::ReserveSpace, true, true),
        ]
    );
}

#[test]
fn the_bar_leaves_fullscreen_apps_alone_unless_asked() {
    let mut app = App::mac();
    assert!(!app.bar().over_fullscreen);
    app.send(Event::ShowOverFullscreen(true));
    assert!(app.bar().over_fullscreen);
    // Pop-outs never join fullscreen apps.
    app.send(Event::PopOut { meter: SPECTRUM });
    assert!(
        !app.window(WindowKey::PopOut(SPECTRUM))
            .unwrap()
            .over_fullscreen
    );
}

#[test]
fn nothing_is_drawn_while_hidden() {
    let mut app = App::mac();
    app.scene();
    app.send(Event::Visible(false));
    app.send(Event::SetEdge(Edge::Top));
    app.now += Duration::from_millis(40);
    assert_eq!(app.core.decide(app.now), Decision::Sleep { until: None });
    app.send(Event::Visible(true));
    app.now += Duration::from_millis(40);
    assert_eq!(app.core.decide(app.now), Decision::Draw);
}
