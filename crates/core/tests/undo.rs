//! Undo and Redo of Theme and Preset changes: a step at a time, a slider
//! drag as one step, and the Meters kept running through it.

use std::time::Duration;

use dasmeter_core::{AppCore, Event, MeterSettings, ThemeScene};

struct App {
    core: AppCore,
    now: Duration,
}

impl App {
    fn new() -> App {
        let mut core = AppCore::new();
        core.handle(
            Event::PresetFiles {
                files: &[],
                settings: None,
            },
            Duration::ZERO,
        );
        App {
            core,
            now: Duration::ZERO,
        }
    }

    /// Something the user does, `after` the last.
    fn act(&mut self, after: Duration, event: Event) {
        self.now += after;
        self.core.handle(event, self.now);
    }

    fn undo(&mut self) {
        self.act(Duration::from_secs(2), Event::Undo);
    }

    fn redo(&mut self) {
        self.act(Duration::from_secs(2), Event::Redo);
    }

    fn theme(&mut self) -> ThemeScene {
        // Drawn now, or unchanged since the last frame.
        self.now += Duration::from_secs(1);
        self.core.decide(self.now);
        self.core.scene().expect("drawn").theme.clone()
    }

    fn index(&mut self, name: &str) -> usize {
        self.theme()
            .themes
            .iter()
            .position(|t| t.name == name)
            .expect("the Theme is listed")
    }

    fn chosen(&self) -> (String, String) {
        let data = self.core.current_data();
        (data.theme.light, data.theme.dark)
    }
}

#[test]
fn undo_brings_back_the_theme_before_and_redo_the_one_after() {
    let mut app = App::new();
    let before = app.chosen();
    let graphite = app.index("Graphite");
    app.act(
        Duration::from_secs(2),
        Event::ChooseTheme {
            light: graphite,
            dark: graphite,
        },
    );
    let after = app.chosen();
    assert_ne!(before, after);

    app.undo();
    assert_eq!(app.chosen(), before);
    app.redo();
    assert_eq!(app.chosen(), after);
}

#[test]
fn a_new_change_after_undo_drops_what_could_be_redone() {
    let mut app = App::new();
    let graphite = app.index("Graphite");
    let light = app.index("Light");
    app.act(
        Duration::from_secs(2),
        Event::ChooseTheme {
            light: graphite,
            dark: graphite,
        },
    );
    app.undo();
    app.act(
        Duration::from_secs(2),
        Event::ChooseTheme { light, dark: light },
    );
    let now = app.chosen();
    app.redo();
    assert_eq!(app.chosen(), now, "nothing to redo");
}

#[test]
fn a_slider_drag_is_undone_in_one_step() {
    let mut app = App::new();
    let graphite = app.index("Graphite");
    app.act(
        Duration::from_secs(2),
        Event::DuplicateTheme { theme: graphite },
    );
    let copy = app.theme().current.expect("the copy is in use");
    let start = app.theme().styling;
    // Dragged in small steps, a frame apart.
    for step in 1..=30 {
        let styling = dasmeter_core::Styling {
            glow: step as f32 / 30.0,
            ..start
        };
        app.act(
            Duration::from_millis(16),
            Event::SetThemeStyling {
                theme: copy,
                styling,
            },
        );
    }
    assert_eq!(app.theme().styling.glow, 1.0);

    app.undo();
    assert_eq!(app.theme().styling, start, "back to before the drag");
}

#[test]
fn undoing_a_meter_setting_puts_it_back() {
    let mut app = App::new();
    let data = app.core.current_data();
    let (meter, settings) = data
        .meters
        .iter()
        .enumerate()
        .find_map(|(i, m)| match &m.settings {
            MeterSettings::Waveform(w) => Some((i, *w)),
            _ => None,
        })
        .expect("a built-in with a Waveform");
    let changed = dasmeter_core::WaveformMeterSettings {
        auto_gain: !settings.auto_gain,
        ..settings
    };
    app.act(
        Duration::from_secs(2),
        Event::SetMeter {
            meter,
            settings: MeterSettings::Waveform(changed),
        },
    );
    app.undo();
    assert_eq!(
        app.core.current_data().meters[meter].settings,
        MeterSettings::Waveform(settings)
    );
}

#[test]
fn moving_the_window_isnt_a_step_to_undo() {
    let mut app = App::new();
    let before = app.chosen();
    let graphite = app.index("Graphite");
    app.act(
        Duration::from_secs(2),
        Event::ChooseTheme {
            light: graphite,
            dark: graphite,
        },
    );
    let frame = dasmeter_core::Rect {
        x: 10.0,
        y: 20.0,
        width: 800.0,
        height: 600.0,
    };
    app.act(Duration::from_secs(2), Event::WindowMoved(frame));
    // One Undo goes past the move to the Theme.
    app.undo();
    assert_eq!(app.chosen(), before);
}
