//! Sharing Presets as `.dasmeter-preset` files: export, import, and the rules
//! for Theme and name clashes.

use std::time::Duration;

use dasmeter_core::sharing::{SharedPreset, without_serial};
use dasmeter_core::{
    AppCore, BuiltIn, Colour, Edge, Event, Note, PresetData, PresetOp, Role, Scene, Theme,
    ThemeFile,
};

struct App {
    core: AppCore,
    now: Duration,
    preset_ops: Vec<PresetOp>,
    theme_files: Vec<ThemeFile>,
}

impl App {
    /// A first launch with these Theme files in the folder.
    fn launch(theme_files: Vec<ThemeFile>) -> App {
        let mut app = App {
            core: AppCore::new(),
            now: Duration::ZERO,
            preset_ops: Vec::new(),
            theme_files,
        };
        let files = app.theme_files.clone();
        app.send(Event::ThemeFiles(&files));
        app.send(Event::PresetFiles {
            files: &[],
            settings: None,
        });
        app.flush();
        app
    }

    fn send(&mut self, event: Event) {
        self.core.handle(event, self.now);
    }

    fn flush(&mut self) {
        self.preset_ops.extend(self.core.take_preset_ops());
        for write in self.core.take_writes() {
            self.theme_files.retain(|f| f.file_name != write.file_name);
            self.theme_files.push(ThemeFile {
                file_name: write.file_name,
                text: write.contents,
            });
        }
    }

    fn scene(&mut self) -> Scene {
        self.now += Duration::from_millis(40);
        self.core.decide(self.now);
        self.flush();
        self.core.scene().unwrap().clone()
    }

    fn current(&mut self) -> String {
        let scene = self.scene();
        scene.presets.list[scene.presets.current.unwrap()]
            .name
            .clone()
    }

    fn import(&mut self, text: &str) {
        self.send(Event::ImportPreset(text));
        self.flush();
    }
}

fn theme_file(theme: &Theme) -> ThemeFile {
    ThemeFile {
        file_name: format!("{}.toml", theme.name.to_lowercase()),
        text: theme.to_toml(),
    }
}

/// A custom Theme: Dark with a red accent.
fn ember() -> Theme {
    let mut theme = Theme::dark();
    theme.name = "Ember".to_owned();
    theme.palette[Role::Accent] = Colour::rgb(0xff, 0x30, 0x10);
    theme
}

/// An app using Ember, with the Bar at the top, exporting its current Preset.
fn exported() -> (String, String) {
    let mut app = App::launch(vec![theme_file(&ember())]);
    let ember = app
        .scene()
        .theme
        .themes
        .iter()
        .position(|t| t.name == "Ember")
        .unwrap();
    app.send(Event::ChooseTheme {
        light: 1,
        dark: ember,
    });
    app.send(Event::SetEdge(Edge::Top));
    app.core.export().unwrap()
}

#[test]
fn export_holds_the_preset_and_its_custom_themes_only() {
    let (name, text) = exported();
    assert_eq!(name, "Bar");
    let (data, themes) = SharedPreset::from_toml(&text).unwrap();
    assert_eq!(data.bar.edge, Edge::Top);
    assert_eq!(
        (data.theme.light.as_str(), data.theme.dark.as_str()),
        ("Light", "Ember")
    );
    // Ember is embedded; Light, a built-in, goes by name only.
    assert_eq!(themes, [ember()]);
    assert_eq!(data.built_in, None, "a shared Preset isn't a built-in");
}

#[test]
fn display_serials_are_stripped() {
    assert_eq!(without_serial("DEL:U2720Q:8XK1234"), "DEL:U2720Q");
    assert_eq!(without_serial("APP:A050"), "APP:A050");
    let data = PresetData {
        bar_display: Some("DEL:U2720Q:8XK1234".into()),
        window_display: Some("APP:A050:SERIAL".into()),
        ..PresetData::built_in(BuiltIn::Bar)
    };
    let text = SharedPreset::new(data, Vec::new()).to_toml();
    assert!(
        !text.contains("8XK1234") && !text.contains("SERIAL"),
        "{text}"
    );
    let (read, _) = SharedPreset::from_toml(&text).unwrap();
    assert_eq!(read.bar_display.as_deref(), Some("DEL:U2720Q"));
}

#[test]
fn importing_switches_to_the_preset_under_a_free_name() {
    let (_, text) = exported();
    let mut other = App::launch(Vec::new());
    other.import(&text);
    // Another Mac already has a "Bar": nothing is overwritten.
    assert_eq!(other.current(), "Bar (2)");
    let scene = other.scene();
    assert_eq!(scene.windows[0].edge, Some(Edge::Top));
    assert_eq!(
        scene.theme.name, "Ember",
        "a dark system uses the imported Theme"
    );
    let written: Vec<&str> = other
        .preset_ops
        .iter()
        .filter_map(|op| match op {
            PresetOp::Write { file_name, .. } => Some(file_name.as_str()),
            _ => None,
        })
        .collect();
    assert!(written.contains(&"bar-2.toml"), "{written:?}");
    assert!(
        written.contains(&"bar.toml"),
        "the built-in copy is still there"
    );
}

#[test]
fn a_new_theme_is_installed() {
    let (_, text) = exported();
    let mut other = App::launch(Vec::new());
    other.import(&text);
    let installed: Vec<&str> = other
        .theme_files
        .iter()
        .map(|f| f.file_name.as_str())
        .collect();
    assert_eq!(installed, ["ember.toml"]);
    assert_eq!(
        Theme::from_toml(&other.theme_files[0].text).unwrap(),
        ember()
    );
}

#[test]
fn an_identical_theme_is_reused() {
    let (_, text) = exported();
    let mut other = App::launch(vec![theme_file(&ember())]);
    other.import(&text);
    assert_eq!(other.theme_files.len(), 1, "nothing new written");
    let names: Vec<String> = other
        .scene()
        .theme
        .themes
        .iter()
        .map(|t| t.name.clone())
        .collect();
    assert_eq!(names.iter().filter(|n| n.starts_with("Ember")).count(), 1);
    assert_eq!(other.scene().theme.name, "Ember");
}

#[test]
fn a_different_theme_of_the_same_name_comes_in_as_2() {
    let (_, text) = exported();
    let mut theirs = ember();
    theirs.palette[Role::Accent] = Colour::rgb(0x00, 0x80, 0xff);
    let mut other = App::launch(vec![theme_file(&theirs)]);
    other.import(&text);
    let scene = other.scene();
    assert_eq!(
        scene.theme.name, "Ember (2)",
        "the Preset points at the copy"
    );
    assert_eq!(scene.palette[Role::Accent], Colour::rgb(0xff, 0x30, 0x10));
    // Their own Ember is untouched.
    let own = other
        .theme_files
        .iter()
        .find(|f| f.file_name == "ember.toml")
        .unwrap();
    assert_eq!(Theme::from_toml(&own.text).unwrap(), theirs);
}

#[test]
fn a_malformed_file_is_refused_without_side_effects() {
    let mut other = App::launch(Vec::new());
    let before = other.scene();
    other.preset_ops.clear();
    for bad in [
        "not toml at all [",
        "format = \"something-else\"\nversion = 1\n[preset]\nname = \"X\"\n",
        // A Theme in it that can't be read.
        &format!(
            "{}\n[[themes]]\ntoml = \"name = \"\n",
            exported().1.split("[[themes]]").next().unwrap()
        ),
    ] {
        other.import(bad);
        let scene = other.scene();
        assert_eq!(scene.presets.list, before.presets.list);
        assert_eq!(scene.presets.current, before.presets.current);
        assert!(other.theme_files.is_empty());
        assert!(
            !other
                .preset_ops
                .iter()
                .any(|op| matches!(op, PresetOp::Write { .. })),
            "{:?}",
            other.preset_ops
        );
        assert_eq!(scene.notes, [Note::ImportFailed]);
    }
}
