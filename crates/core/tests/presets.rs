//! Presets with files in memory and a fake clock: first launch, auto-save,
//! Revert, versions, built-ins, managing, shortcuts and Send Plugin matching.

use std::collections::BTreeMap;
use std::time::Duration;

use dasmeter_core::{
    AUTOSAVE_DELAY, AppCore, Edge, Event, LayoutMode, ListenTo, MeterState, PresetData, PresetFile,
    PresetOp, Scene, SendPlugin, SendPluginState, WindowKey,
};

/// The presets folder, `settings.toml`, the trash and the Preset files' `.bak`s.
#[derive(Default, Clone)]
struct Disk {
    files: BTreeMap<String, String>,
    settings: Option<String>,
    trash: Vec<String>,
}

impl Disk {
    fn apply(&mut self, ops: Vec<PresetOp>) {
        for op in ops {
            match op {
                PresetOp::Write {
                    file_name,
                    contents,
                } => {
                    self.files.insert(file_name, contents);
                }
                PresetOp::WriteIfMissing {
                    file_name,
                    contents,
                } => {
                    self.files.entry(file_name).or_insert(contents);
                }
                PresetOp::Trash { file_name } => {
                    self.files.remove(&file_name);
                    self.trash.push(file_name);
                }
                PresetOp::Settings { contents } => self.settings = Some(contents),
            }
        }
    }

    fn preset_files(&self) -> Vec<PresetFile> {
        self.files
            .iter()
            .filter(|(name, _)| name.ends_with(".toml"))
            .map(|(name, text)| PresetFile {
                file_name: name.clone(),
                text: text.clone(),
            })
            .collect()
    }
}

struct App {
    core: AppCore,
    now: Duration,
    disk: Disk,
}

impl App {
    /// Launches the app on `disk`.
    fn launch(disk: Disk) -> App {
        let mut app = App {
            core: AppCore::new(),
            now: Duration::ZERO,
            disk,
        };
        let files = app.disk.preset_files();
        let settings = app.disk.settings.clone();
        app.send(Event::PresetFiles {
            files: &files,
            settings: settings.as_deref(),
        });
        app.flush();
        app
    }

    fn first_launch() -> App {
        App::launch(Disk::default())
    }

    /// Quits (after the pending save) and launches again on the same disk.
    fn relaunch(mut self) -> App {
        self.wait(AUTOSAVE_DELAY * 2);
        App::launch(self.disk)
    }

    fn send(&mut self, event: Event) {
        self.core.handle(event, self.now);
    }

    fn flush(&mut self) {
        let ops = self.core.take_preset_ops();
        self.disk.apply(ops);
    }

    /// Lets time pass, drawing as the shell would, and writes what the core asks.
    fn wait(&mut self, time: Duration) {
        let end = self.now + time;
        while self.now < end {
            self.now += Duration::from_millis(100);
            self.core.decide(self.now);
            self.flush();
        }
    }

    fn scene(&mut self) -> Scene {
        self.now += Duration::from_millis(40);
        self.core.decide(self.now);
        self.flush();
        self.core.scene().unwrap().clone()
    }

    fn names(&mut self) -> Vec<String> {
        self.scene()
            .presets
            .list
            .iter()
            .map(|p| p.name.clone())
            .collect()
    }

    fn current(&mut self) -> String {
        let scene = self.scene();
        let i = scene.presets.current.expect("a Preset is open");
        scene.presets.list[i].name.clone()
    }

    fn edge(&mut self) -> Option<Edge> {
        self.scene().windows[0].edge
    }
}

#[test]
fn first_launch_copies_the_built_ins_and_opens_bar() {
    let mut app = App::first_launch();
    assert_eq!(app.names(), ["Bar", "Mixing", "Mastering"]);
    assert_eq!(app.current(), "Bar");
    assert!(app.scene().presets.list.iter().all(|p| p.built_in));
    let files: Vec<&String> = app.disk.files.keys().collect();
    assert_eq!(files, ["bar.toml", "mastering.toml", "mixing.toml"]);
    assert!(app.disk.settings.is_some());

    // The built-ins are what the spec lays out.
    app.send(Event::SwitchPreset { index: 1 });
    let scene = app.scene();
    assert_eq!(scene.mode, LayoutMode::Window);
    assert_eq!(scene.windows[0].meters.len(), 4);
    app.send(Event::SwitchPreset { index: 2 });
    let scene = app.scene();
    let loudness = scene.windows[0]
        .meters
        .iter()
        .find(|m| m.meter == 3)
        .unwrap();
    assert!(
        loudness.frame.width > 0.5 && loudness.frame.height == 1.0,
        "a large Loudness Meter"
    );
}

#[test]
fn a_deleted_built_in_isnt_copied_again() {
    let mut app = App::first_launch();
    app.send(Event::DeletePreset { index: 2 });
    app.flush();
    let mut app = app.relaunch();
    assert_eq!(app.names(), ["Bar", "Mixing"]);
}

#[test]
fn changes_are_saved_after_a_pause_and_come_back_at_launch() {
    let mut app = App::first_launch();
    let before = app.disk.files["bar.toml"].clone();
    app.send(Event::SetEdge(Edge::Top));
    app.wait(AUTOSAVE_DELAY / 2);
    assert_eq!(app.disk.files["bar.toml"], before, "not before the pause");
    // Another change restarts the pause.
    app.send(Event::SetBarThickness(240.0));
    app.wait(AUTOSAVE_DELAY / 2 + Duration::from_millis(200));
    assert_eq!(app.disk.files["bar.toml"], before);
    app.wait(AUTOSAVE_DELAY);
    assert_ne!(app.disk.files["bar.toml"], before, "saved after the pause");

    // Everything comes back as it was: the last Preset, its layout and Meters.
    app.send(Event::SwitchPreset { index: 1 });
    app.send(Event::ClosePane { meter: 2 });
    let mut app = app.relaunch();
    assert_eq!(app.current(), "Mixing");
    assert_eq!(app.scene().windows[0].meters.len(), 3);
    app.send(Event::SwitchPreset { index: 0 });
    assert_eq!(app.edge(), Some(Edge::Top));
    assert_eq!(app.core.layout().thickness, 240.0);
}

#[test]
fn app_settings_live_in_settings_toml() {
    let mut app = App::first_launch();
    let mut settings = app.core.app_settings();
    settings.check_for_updates = false;
    app.send(Event::SetApp(settings));
    app.flush();
    assert!(
        app.disk
            .settings
            .as_ref()
            .unwrap()
            .contains("check_for_updates = false")
    );
    let app = app.relaunch();
    assert!(!app.core.app_settings().check_for_updates);
}

#[test]
fn revert_goes_back_to_the_preset_as_opened() {
    let mut app = App::first_launch();
    assert!(!app.scene().presets.can_revert);
    app.send(Event::SetEdge(Edge::Left));
    app.wait(AUTOSAVE_DELAY * 2);
    assert!(app.scene().presets.can_revert);
    app.send(Event::RevertPreset);
    assert_eq!(app.edge(), Some(Edge::Bottom));
    app.wait(AUTOSAVE_DELAY * 2);
    assert!(!app.scene().presets.can_revert);
    let saved: PresetData = toml::from_str(&app.disk.files["bar.toml"]).unwrap();
    assert_eq!(saved.bar.edge, Edge::Bottom, "the reverted state is saved");

    // Revert only goes back to the last switch.
    app.send(Event::SetEdge(Edge::Top));
    app.send(Event::SwitchPreset { index: 1 });
    app.send(Event::SwitchPreset { index: 0 });
    app.send(Event::RevertPreset);
    assert_eq!(app.edge(), Some(Edge::Top));
}

#[test]
fn an_older_file_is_migrated_with_a_one_time_backup() {
    let mut disk = App::first_launch().disk;
    // Version 0: no `version`, and the mode was called `layout`.
    let old = "name = \"Old\"\nlayout = \"window\"\n";
    disk.files.insert("old.toml".into(), old.into());
    let mut app = App::launch(disk);
    assert!(app.names().contains(&"Old".to_owned()));
    assert_eq!(app.disk.files["old.toml.bak"], old);
    let migrated: PresetData = toml::from_str(&app.disk.files["old.toml"]).unwrap();
    assert_eq!(migrated.version, 1);
    assert_eq!(migrated.mode, LayoutMode::Window);

    // The backup is written once: a later migration never replaces it.
    app.disk
        .files
        .insert("old.toml".into(), "name = \"Old\"\n".into());
    let app = App::launch(app.disk);
    assert_eq!(app.disk.files["old.toml.bak"], old);
}

#[test]
fn a_newer_file_is_read_only_and_never_saved_over() {
    let mut disk = App::first_launch().disk;
    let newer = "version = 9\nname = \"Future\"\nmode = \"Window\"\nhologram = true\n";
    disk.files.insert("future.toml".into(), newer.into());
    let mut app = App::launch(disk);
    let index = app
        .names()
        .iter()
        .position(|n| n == "Future (newer, read-only)")
        .unwrap();
    app.send(Event::SwitchPreset { index });
    assert_eq!(
        app.scene().mode,
        LayoutMode::Window,
        "what's understood loads"
    );
    app.send(Event::ClosePane { meter: 0 });
    app.wait(AUTOSAVE_DELAY * 2);
    assert_eq!(app.disk.files["future.toml"], newer);
    app.send(Event::RenamePreset {
        index,
        name: "Mine",
    });
    app.flush();
    assert_eq!(app.disk.files["future.toml"], newer);
}

#[test]
fn a_broken_file_is_listed_but_never_opened_or_deleted() {
    let mut disk = App::first_launch().disk;
    disk.files.insert("zz.toml".into(), "name = [".into());
    let mut app = App::launch(disk);
    let names = app.names();
    let index = names
        .iter()
        .position(|n| n == "zz.toml (can't be read)")
        .unwrap();
    assert!(app.scene().presets.list[index].broken);
    app.send(Event::SwitchPreset { index });
    assert_eq!(app.current(), "Bar");
    app.send(Event::DeletePreset { index });
    app.flush();
    assert!(app.disk.files.contains_key("zz.toml"));
    assert!(app.disk.trash.is_empty());
}

#[test]
fn reset_brings_a_built_in_back() {
    let mut app = App::first_launch();
    app.send(Event::SwitchPreset { index: 1 });
    app.send(Event::ClosePane { meter: 1 });
    app.send(Event::RenamePreset {
        index: 1,
        name: "My mix",
    });
    app.wait(AUTOSAVE_DELAY * 2);
    assert_eq!(app.scene().windows[0].meters.len(), 3);
    app.send(Event::ResetPreset { index: 1 });
    app.flush();
    assert_eq!(app.current(), "Mixing");
    assert_eq!(app.scene().windows[0].meters.len(), 4);
    let saved: PresetData = toml::from_str(&app.disk.files["mixing.toml"]).unwrap();
    assert_eq!(saved, PresetData::built_in(dasmeter_core::BuiltIn::Mixing));
}

#[test]
fn deleting_the_current_preset_switches_to_the_one_above() {
    let mut app = App::first_launch();
    app.send(Event::SwitchPreset { index: 2 });
    app.send(Event::DeletePreset { index: 2 });
    app.flush();
    assert_eq!(app.current(), "Mixing");
    assert_eq!(app.disk.trash, ["mastering.toml"]);
    // The first one: the new first takes over.
    app.send(Event::SwitchPreset { index: 0 });
    app.send(Event::DeletePreset { index: 0 });
    assert_eq!(app.current(), "Mixing");
    // The last one stays.
    app.send(Event::DeletePreset { index: 0 });
    app.flush();
    assert_eq!(app.names(), ["Mixing"]);
}

#[test]
fn managing_names_copies_and_order() {
    let mut app = App::first_launch();
    app.send(Event::DuplicatePreset { index: 0 });
    app.flush();
    assert_eq!(app.names(), ["Bar", "Bar (2)", "Mixing", "Mastering"]);
    assert!(
        !app.scene().presets.list[1].built_in,
        "a copy isn't a built-in"
    );
    app.send(Event::RenamePreset {
        index: 1,
        name: "Mixing",
    });
    assert_eq!(app.names()[1], "Mixing (2)", "a taken name gets (2)");

    app.send(Event::SetEdge(Edge::Right));
    app.send(Event::SavePresetAsNew);
    app.flush();
    assert_eq!(app.current(), "Bar (2)");
    assert_eq!(app.edge(), Some(Edge::Right));

    app.send(Event::MovePreset { from: 4, to: 0 });
    let order = app.names();
    let app = app.relaunch();
    let mut app = app;
    assert_eq!(app.names(), order, "the order is kept");
}

#[test]
fn shortcuts_follow_the_list_order() {
    let mut app = App::first_launch();
    app.send(Event::PresetShortcut { number: 3 });
    assert_eq!(app.current(), "Mastering");
    app.send(Event::MovePreset { from: 2, to: 0 });
    app.send(Event::PresetShortcut { number: 2 });
    assert_eq!(app.current(), "Bar");
    // Numbers past the list, or 0, do nothing.
    app.send(Event::PresetShortcut { number: 9 });
    app.send(Event::PresetShortcut { number: 0 });
    assert_eq!(app.current(), "Bar");
}

fn plugin(id: u64, name: &str) -> SendPlugin {
    SendPlugin {
        id,
        name: name.to_owned(),
        colour: 0x33_66_99,
        mono: false,
        sample_rate: 48_000,
        state: SendPluginState::Live,
        outdated: false,
    }
}

#[test]
fn a_loaded_preset_finds_its_send_plugins_by_id_then_name() {
    let mut app = App::first_launch();
    app.send(Event::SetListenTo(ListenTo::SendPlugins));
    let plugins = [plugin(1, "Kick"), plugin(2, "Bass")];
    app.send(Event::SendPlugins(&plugins));
    app.send(Event::PickSendPlugin { meter: 0, id: 1 });
    app.send(Event::PickSendPlugin { meter: 1, id: 2 });
    app.send(Event::PickSendPlugin { meter: 2, id: 2 });
    app.send(Event::PickSendPlugin { meter: 3, id: 2 });

    // Next session: the DAW gave Kick a new ID; Bass kept its own; no Hats.
    let mut app = app.relaunch();
    let plugins = [plugin(7, "Kick"), plugin(2, "Bass")];
    app.send(Event::SendPlugins(&plugins));
    let scene = app.scene();
    let picked: Vec<Option<u64>> = scene.windows[0].meters.iter().map(|m| m.picked).collect();
    assert_eq!(picked, [Some(7), Some(2), Some(2), Some(2)]);

    // A Send Plugin that isn't there is waited for.
    let plugins = [plugin(2, "Bass")];
    app.send(Event::SendPlugins(&plugins));
    let scene = app.scene();
    let waiting = &scene.windows[0].meters[0].state;
    assert!(matches!(waiting, MeterState::WaitingFor(name) if name == "Kick"));
    let _ = WindowKey::Bar;
}

#[test]
#[ignore = "prints a Preset file to look at"]
fn print_a_preset_file() {
    println!(
        "{}",
        PresetData::built_in(dasmeter_core::BuiltIn::Mastering).to_toml()
    );
}
