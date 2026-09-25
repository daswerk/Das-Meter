//! First launch: the welcome card, Start listening, the silence hint, the
//! one-time Send Plugin note and desktop tip, and the Dock setting, with
//! `settings.toml` in memory and a fake clock.

use std::time::Duration;

use dasmeter_core::{
    AppCore, Card, Event, LayoutMode, ListenTo, MeterState, PresetFile, PresetOp, Scene,
    SendPlugin, SendPluginState,
};

const RATE: u32 = 48_000;

struct App {
    core: AppCore,
    now: Duration,
    /// `settings.toml` as last written, and the Preset files.
    settings: Option<String>,
    files: Vec<PresetFile>,
}

impl App {
    fn launch(settings: Option<String>, files: Vec<PresetFile>) -> App {
        let mut app = App {
            core: AppCore::new(),
            now: Duration::ZERO,
            settings,
            files,
        };
        let settings = app.settings.clone();
        let files = app.files.clone();
        app.core.handle(
            Event::PresetFiles {
                files: &files,
                settings: settings.as_deref(),
            },
            app.now,
        );
        app.flush();
        app
    }

    fn first_launch() -> App {
        App::launch(None, Vec::new())
    }

    fn relaunch(mut self) -> App {
        self.flush();
        App::launch(self.settings, self.files)
    }

    fn flush(&mut self) {
        for op in self.core.take_preset_ops() {
            match op {
                PresetOp::Settings { contents } => self.settings = Some(contents),
                PresetOp::Write {
                    file_name,
                    contents,
                }
                | PresetOp::WriteIfMissing {
                    file_name,
                    contents,
                } => {
                    self.files.retain(|f| f.file_name != file_name);
                    self.files.push(PresetFile {
                        file_name,
                        text: contents,
                    });
                }
                PresetOp::Trash { file_name } => self.files.retain(|f| f.file_name != file_name),
            }
        }
    }

    fn send(&mut self, event: Event) {
        self.core.handle(event, self.now);
        self.flush();
    }

    fn scene(&mut self) -> Scene {
        self.now += Duration::from_millis(40);
        self.core.decide(self.now);
        self.flush();
        self.core.scene().unwrap().clone()
    }

    fn card(&mut self) -> Option<Card> {
        self.scene().card
    }

    /// Plays `seconds` of `level` (0 for silence) through System Capture in
    /// 10 ms blocks, as the shell does once capture runs.
    fn play(&mut self, seconds: f64, level: f32) {
        let block = vec![level; 2 * 480];
        for _ in 0..(seconds * 100.0).round() as usize {
            self.now += Duration::from_millis(10);
            self.core.handle(Event::Audio(&block), self.now);
            self.core.decide(self.now);
        }
        self.flush();
    }

    fn capture_starts(&mut self) {
        assert!(
            self.core.may_capture(),
            "the shell only captures when allowed"
        );
        self.send(Event::CaptureStarted { sample_rate: RATE });
    }
}

fn sending(name: &str, state: SendPluginState) -> SendPlugin {
    SendPlugin {
        id: 7,
        name: name.to_owned(),
        colour: 0x33_66_99,
        mono: false,
        sample_rate: RATE,
        state,
        outdated: false,
    }
}

#[test]
fn first_run_shows_the_welcome_card_and_captures_nothing() {
    let mut app = App::first_launch();
    let scene = app.scene();
    assert_eq!(scene.card, Some(Card::Welcome));
    assert_eq!(scene.mode, LayoutMode::Bar, "opens with the Bar Preset");
    assert_eq!(scene.listen_to, ListenTo::SystemCapture);
    assert!(
        !app.core.may_capture(),
        "no capture, so no macOS prompt yet"
    );
    assert!(
        scene.windows[0]
            .meters
            .iter()
            .all(|m| m.state == MeterState::NotListening)
    );
}

#[test]
fn start_listening_starts_capture() {
    let mut app = App::first_launch();
    app.send(Event::StartListening);
    assert!(app.core.may_capture());
    assert_eq!(app.card(), None);
    app.capture_starts();
    assert!(
        app.scene().windows[0]
            .meters
            .iter()
            .all(|m| matches!(m.state, MeterState::Live(_)))
    );
    // And it stays allowed after a relaunch, with no card.
    let mut app = app.relaunch();
    assert!(app.core.may_capture());
    assert_eq!(app.card(), None);
}

#[test]
fn closing_the_card_leaves_a_start_listening_button_on_the_meters() {
    let mut app = App::first_launch();
    app.send(Event::CloseWelcome);
    let scene = app.scene();
    assert_eq!(scene.card, None);
    assert!(!app.core.may_capture());
    assert!(
        scene.windows[0]
            .meters
            .iter()
            .all(|m| m.state == MeterState::NotListening)
    );
    // Until pressed, across launches too; then capture may run.
    let mut app = app.relaunch();
    assert_eq!(app.card(), None, "the card shows only once");
    assert!(!app.core.may_capture());
    app.send(Event::StartListening);
    assert!(app.core.may_capture());
}

#[test]
fn the_card_shows_once_and_show_welcome_brings_it_back() {
    let mut app = App::first_launch();
    app.send(Event::StartListening);
    let mut app = app.relaunch();
    assert_eq!(app.card(), None);
    app.send(Event::ShowWelcome);
    assert_eq!(app.card(), Some(Card::Welcome));
    app.send(Event::CloseWelcome);
    assert_eq!(app.card(), None);
    assert!(app.core.may_capture(), "✕ after listening doesn't stop it");
}

#[test]
fn ten_seconds_of_silence_bring_the_hint() {
    let mut app = App::first_launch();
    app.send(Event::StartListening);
    app.capture_starts();
    app.play(9.0, 0.0);
    assert_eq!(app.card(), None, "not yet");
    app.play(1.5, 0.0);
    assert_eq!(app.card(), Some(Card::SilenceHint));
    // Sound makes it go away.
    app.play(0.1, 0.1);
    assert_eq!(app.card(), None);
}

#[test]
fn the_hint_switches_to_send_plugins_in_one_click_and_closes_for_the_run() {
    let mut app = App::first_launch();
    app.send(Event::StartListening);
    app.capture_starts();
    app.play(11.0, 0.0);
    assert_eq!(app.card(), Some(Card::SilenceHint));
    // The one-click switch is Listen to ▸ Send Plugins.
    app.send(Event::SetListenTo(ListenTo::SendPlugins));
    assert_eq!(app.card(), None);
    assert_eq!(app.scene().listen_to, ListenTo::SendPlugins);

    let mut app = App::first_launch();
    app.send(Event::StartListening);
    app.capture_starts();
    app.play(11.0, 0.0);
    app.send(Event::CloseCard);
    app.play(20.0, 0.0);
    assert_eq!(app.card(), None, "✕: not again this run");
}

#[test]
fn no_hint_once_something_played() {
    let mut app = App::first_launch();
    app.send(Event::StartListening);
    app.capture_starts();
    app.play(1.0, 0.1);
    app.play(30.0, 0.0);
    assert_eq!(app.card(), None, "a pause in the music isn't a problem");
}

#[test]
fn the_first_sending_send_plugin_brings_a_note_once() {
    let mut app = App::first_launch();
    app.send(Event::StartListening);
    app.capture_starts();
    assert!(app.core.wants_send_plugins_listed());
    app.send(Event::SendPlugins(&[sending(
        "Vocals",
        SendPluginState::Idle,
    )]));
    assert_eq!(app.card(), None, "idle isn't sending");
    app.send(Event::SendPlugins(&[sending(
        "Vocals",
        SendPluginState::Live,
    )]));
    assert_eq!(
        app.card(),
        Some(Card::SendPluginFound {
            name: "Vocals".into()
        })
    );
    assert!(!app.core.wants_send_plugins_listed());
    // Switch listens to Send Plugins and closes the note.
    app.send(Event::SetListenTo(ListenTo::SendPlugins));
    assert_eq!(app.card(), None);

    let mut app = app.relaunch();
    app.send(Event::SetListenTo(ListenTo::SystemCapture));
    app.send(Event::SendPlugins(&[sending(
        "Drums",
        SendPluginState::Live,
    )]));
    assert_eq!(app.card(), None, "once per install");
}

#[test]
fn the_welcome_card_comes_before_a_send_plugin_note() {
    let mut app = App::first_launch();
    app.send(Event::SendPlugins(&[sending(
        "Vocals",
        SendPluginState::Live,
    )]));
    assert_eq!(app.card(), Some(Card::Welcome));
    app.send(Event::StartListening);
    assert_eq!(
        app.card(),
        Some(Card::SendPluginFound {
            name: "Vocals".into()
        })
    );
    app.send(Event::CloseCard);
    assert_eq!(app.card(), None);
}

#[test]
fn the_desktop_tip_shows_once_per_install() {
    let mut app = App::first_launch();
    app.send(Event::DesktopWithoutBar);
    assert!(app.core.take_desktop_tip());
    assert!(!app.core.take_desktop_tip(), "taken");
    app.send(Event::DesktopWithoutBar);
    assert!(!app.core.take_desktop_tip());
    let mut app = app.relaunch();
    app.send(Event::DesktopWithoutBar);
    assert!(!app.core.take_desktop_tip());
}

#[test]
fn show_in_dock_is_on_by_default_and_kept() {
    let mut app = App::first_launch();
    assert!(app.core.show_in_dock());
    assert!(app.scene().show_in_dock);
    app.send(Event::SetShowInDock(false));
    let mut app = app.relaunch();
    assert!(!app.core.show_in_dock());
    assert!(!app.scene().show_in_dock);
}

#[test]
fn launch_at_login_is_off_by_default_and_kept() {
    let mut app = App::first_launch();
    assert!(!app.core.launch_at_login());
    app.send(Event::SetLaunchAtLogin(true));
    let app = app.relaunch();
    assert!(app.core.launch_at_login());
}
