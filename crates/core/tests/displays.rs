//! Multi-display layouts: windows remember their display by its EDID
//! fingerprint, fall back while it's missing, and return when it's back.

use std::time::Duration;

use dasmeter_core::{
    AppCore, BuiltIn, Display, DisplayRef, Edge, Event, Fingerprint, LayoutMode, PresetData,
    PresetFile, Rect, Scene, Screen, WindowKey, WindowScene,
};

const DELL: (u32, u32) = (0x10ac, 0xa0c4);

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// The laptop's built-in display: the main one, with a menu bar and Dock.
fn laptop() -> Screen {
    Screen {
        display: Display {
            frame: rect(0.0, 0.0, 1512.0, 982.0),
            usable: rect(0.0, 33.0, 1512.0, 879.0),
        },
        fingerprint: Some(Fingerprint {
            vendor: 0x0610,
            model: 0xa050,
            serial: 0,
        }),
        name: "Built-in Retina Display".into(),
        main: true,
    }
}

/// A 2560 × 1440 Dell at `x`, with `serial`.
fn dell(x: f32, serial: u32) -> Screen {
    Screen {
        display: Display {
            frame: rect(x, 0.0, 2560.0, 1440.0),
            usable: rect(x, 0.0, 2560.0, 1440.0),
        },
        fingerprint: Some(Fingerprint {
            vendor: DELL.0,
            model: DELL.1,
            serial,
        }),
        name: "DELL U2720Q".into(),
        main: false,
    }
}

struct App {
    core: AppCore,
    now: Duration,
}

impl App {
    fn with(screens: &[Screen]) -> App {
        let mut app = App {
            core: AppCore::new(),
            now: Duration::ZERO,
        };
        app.send(Event::Screens(screens));
        app
    }

    fn send(&mut self, event: Event) {
        self.core.handle(event, self.now);
    }

    fn scene(&mut self) -> Scene {
        self.now += Duration::from_millis(40);
        self.core.decide(self.now);
        self.core.scene().unwrap().clone()
    }

    fn window(&mut self, key: WindowKey) -> WindowScene {
        self.scene()
            .windows
            .into_iter()
            .find(|w| w.key == key)
            .unwrap()
    }

    fn frame(&mut self, key: WindowKey) -> Rect {
        self.window(key).frame.unwrap()
    }

    /// Pops the Spectrum out and puts it at `frame` (as the user drags it).
    fn pop_out_at(&mut self, frame: Rect) {
        self.send(Event::PopOut { meter: 1 });
        self.send(Event::PopOutMoved { meter: 1, frame });
    }
}

const SPECTRUM: WindowKey = WindowKey::PopOut(1);

#[test]
fn a_window_on_the_external_display_stays_with_that_monitor() {
    let mut app = App::with(&[laptop(), dell(1512.0, 7)]);
    app.pop_out_at(rect(1800.0, 200.0, 600.0, 400.0));
    // The Dell is now on the left and listed first: the Pop-out follows it.
    app.send(Event::Screens(&[dell(-2560.0, 7), laptop()]));
    assert_eq!(
        app.frame(SPECTRUM),
        rect(-2560.0 + 288.0, 200.0, 600.0, 400.0)
    );
    assert!(app.scene().missing_displays.is_empty());
}

#[test]
fn another_monitor_of_the_same_model_stands_in_when_the_serial_is_gone() {
    let mut app = App::with(&[laptop(), dell(1512.0, 7)]);
    app.pop_out_at(rect(1800.0, 200.0, 600.0, 400.0));
    // A different Dell of the same model (another desk, a replaced monitor).
    app.send(Event::Screens(&[laptop(), dell(1512.0, 9)]));
    assert_eq!(app.frame(SPECTRUM), rect(1800.0, 200.0, 600.0, 400.0));
}

#[test]
fn two_of_a_model_are_told_apart_by_position() {
    // Neither reports a serial: position relative to the main display decides.
    let mut app = App::with(&[laptop(), dell(-2560.0, 0), dell(1512.0, 0)]);
    app.pop_out_at(rect(1800.0, 200.0, 600.0, 400.0));
    app.send(Event::Screens(&[
        dell(1512.0, 0),
        laptop(),
        dell(-2560.0, 0),
    ]));
    assert_eq!(app.frame(SPECTRUM).x, 1800.0, "still the one on the right");
}

#[test]
fn a_missing_display_sends_windows_to_the_fallbacks_and_they_return() {
    let mut app = App::with(&[laptop(), dell(1512.0, 7)]);
    app.pop_out_at(rect(1800.0, 900.0, 600.0, 400.0));
    app.send(Event::SetMode(LayoutMode::Window));
    app.send(Event::WindowMoved(rect(2000.0, 100.0, 1600.0, 1000.0)));
    app.send(Event::SetMode(LayoutMode::Bar));

    app.send(Event::Screens(&[laptop()]));
    // The Pop-out: the Bar's display at the same offset, pulled inside.
    let frame = app.frame(SPECTRUM);
    assert_eq!((frame.x, frame.width), (288.0, 600.0));
    assert!(frame.bottom() <= 912.0, "inside the usable area: {frame:?}");
    let scene = app.scene();
    assert_eq!(scene.missing_displays, ["DELL U2720Q"]);
    // The Window: the main display, clamped to fit.
    app.send(Event::SetMode(LayoutMode::Window));
    let window = app.frame(WindowKey::Main);
    assert!(window.x >= 0.0 && window.right() <= 1512.0 && window.bottom() <= 912.0);

    // The Dell comes back: everything goes back where it was.
    app.send(Event::Screens(&[laptop(), dell(1512.0, 7)]));
    assert_eq!(
        app.frame(WindowKey::Main),
        rect(2000.0, 100.0, 1600.0, 1000.0)
    );
    app.send(Event::SetMode(LayoutMode::Bar));
    assert_eq!(app.frame(SPECTRUM), rect(1800.0, 900.0, 600.0, 400.0));
    assert!(app.scene().missing_displays.is_empty());
}

#[test]
fn a_move_while_the_display_is_missing_wins() {
    let mut app = App::with(&[laptop(), dell(1512.0, 7)]);
    app.pop_out_at(rect(1800.0, 200.0, 600.0, 400.0));
    app.send(Event::Screens(&[laptop()]));
    // The user puts it somewhere else on the laptop.
    app.send(Event::PopOutMoved {
        meter: 1,
        frame: rect(50.0, 60.0, 600.0, 400.0),
    });
    assert!(app.scene().missing_displays.is_empty());
    app.send(Event::Screens(&[laptop(), dell(1512.0, 7)]));
    assert_eq!(app.frame(SPECTRUM), rect(50.0, 60.0, 600.0, 400.0));
}

#[test]
fn keep_here_adopts_every_window_out_of_place() {
    let mut app = App::with(&[laptop(), dell(1512.0, 7)]);
    app.pop_out_at(rect(1800.0, 200.0, 600.0, 400.0));
    app.send(Event::Screens(&[laptop()]));
    let fallback = app.frame(SPECTRUM);
    app.send(Event::KeepHere);
    assert!(app.scene().missing_displays.is_empty());
    app.send(Event::Screens(&[laptop(), dell(1512.0, 7)]));
    assert_eq!(app.frame(SPECTRUM), fallback, "it stays on the laptop");
}

fn preset_on_unknown_monitors() -> PresetData {
    let elsewhere = DisplayRef {
        fingerprint: Fingerprint {
            vendor: 0x4c2d,
            model: 0x1234,
            serial: 0,
        },
        name: "Samsung Odyssey".into(),
        position: [0.0, -1440.0],
    };
    let mut data = PresetData::built_in(BuiltIn::Bar);
    data.name = "Studio".into();
    data.bar.display = Some(elsewhere.clone());
    data.bar.edge = Edge::Top;
    data.bar.thickness = 300.0;
    data
}

#[test]
fn a_preset_from_another_machine_lands_on_the_main_display() {
    let files = [PresetFile {
        file_name: "studio.toml".into(),
        text: preset_on_unknown_monitors().to_toml(),
    }];
    let mut app = App::with(&[laptop(), dell(1512.0, 7)]);
    app.send(Event::PresetFiles {
        files: &files,
        settings: Some("last_preset = \"studio.toml\"\n[first_launch]\nbuilt_ins_copied = true\n"),
    });
    // The Bar: same edge, on the main display, capped to a third of it.
    let bar = app.frame(WindowKey::Bar);
    assert_eq!((bar.x, bar.y, bar.width), (0.0, 33.0, 1512.0));
    assert_eq!(bar.height, 300.0);
    assert_eq!(app.scene().missing_displays, ["Samsung Odyssey"]);
}

#[test]
fn the_bar_is_capped_and_frames_are_kept_inside_the_display() {
    let mut app = App::with(&[laptop()]);
    app.send(Event::SetBarThickness(2_000.0));
    assert!((app.frame(WindowKey::Bar).height - 982.0 / 3.0).abs() < 0.01);
    // A Pop-out dragged half off the screen is shown inside it.
    app.pop_out_at(rect(1400.0, 800.0, 600.0, 400.0));
    let frame = app.frame(SPECTRUM);
    assert!(
        frame.right() <= 1512.0 && frame.bottom() <= 912.0,
        "{frame:?}"
    );
}
