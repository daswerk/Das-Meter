//! Themes: files, built-ins, Duplicate, overrides, the light/dark pair and a
//! missing Theme. Files live in memory: the core reads `ThemeFile`s and hands
//! back `FileWrite`s.

use std::time::Duration;

use dasmeter_core::{
    AppCore, Appearance, Colour, Event, FileWrite, LineWeight, Look, Palette, Role, Scene, Styling,
    Theme, ThemeFile,
};

const DARK: usize = 0;
const LIGHT: usize = 1;
const HIGH_CONTRAST: usize = 2;
const WAVEFORM: usize = 0;

struct App {
    core: AppCore,
    now: Duration,
    /// The themes folder.
    folder: Vec<ThemeFile>,
}

impl App {
    fn new() -> App {
        App {
            core: AppCore::new(),
            now: Duration::ZERO,
            folder: Vec::new(),
        }
    }

    fn send(&mut self, event: Event) {
        self.core.handle(event, self.now);
    }

    fn scene(&mut self) -> Scene {
        self.now += Duration::from_millis(40);
        self.core.decide(self.now);
        self.core.scene().unwrap().clone()
    }

    /// Writes what the core asked for into the folder, as the shell does.
    fn flush(&mut self) -> Vec<FileWrite> {
        let writes = self.core.take_writes();
        for write in &writes {
            self.folder.retain(|f| f.file_name != write.file_name);
            self.folder.push(ThemeFile {
                file_name: write.file_name.clone(),
                text: write.contents.clone(),
            });
        }
        writes
    }

    /// The shell rescans the folder.
    fn rescan(&mut self) {
        let files = self.folder.clone();
        self.send(Event::ThemeFiles(&files));
    }

    fn index_of(&mut self, name: &str) -> usize {
        self.scene()
            .theme
            .themes
            .iter()
            .position(|t| t.name == name)
            .unwrap_or_else(|| panic!("no Theme {name:?}"))
    }
}

const RED: Colour = Colour::rgb(0xff, 0x00, 0x00);

#[test]
fn a_theme_survives_a_round_trip_through_its_file() {
    let theme = Theme {
        name: "Studio \"B\"".to_owned(),
        palette: Palette::light().with(&[(Role::Accent, Colour { a: 0x80, ..RED })]),
        styling: Styling {
            background_opacity: 0.6,
            line: LineWeight::Thick,
            gap: 10.0,
            corner_radius: 8.0,
            text_scale: 1.25,
            shape_cues: true,
            ..Styling::default()
        },
    };
    let text = theme.to_toml();
    assert!(text.contains("accent = \"#ff000080\""), "{text}");
    assert_eq!(Theme::from_toml(&text).unwrap(), theme);
}

#[test]
fn a_file_missing_values_takes_them_from_dark_and_keeps_ranges() {
    let theme = Theme::from_toml(
        "name = \"Sparse\"\n[colours]\ntext = \"#123456\"\n[styling]\ntext_scale = 9.0\n",
    )
    .unwrap();
    assert_eq!(theme.palette[Role::Text], Colour::rgb(0x12, 0x34, 0x56));
    assert_eq!(theme.palette[Role::Panel], Palette::dark()[Role::Panel]);
    assert_eq!(theme.styling.text_scale, 1.5);
    assert!(Theme::from_toml("version = 2\nname = \"Future\"").is_err());
    assert!(Theme::from_toml("not toml [").is_err());
}

#[test]
fn the_spectrums_second_trace_has_its_own_colour() {
    // A file from before the role existed keeps its accent there, as it drew.
    let older = Theme::from_toml("name = \"Older\"\n[colours]\naccent = \"#00ff00\"\n").unwrap();
    assert_eq!(
        older.palette[Role::SpectrumSecond],
        Colour::rgb(0x00, 0xff, 0x00)
    );
    let own = Theme::from_toml(
        "name = \"Own\"\n[colours]\naccent = \"#00ff00\"\nspectrum_second = \"#123456\"\n",
    )
    .unwrap();
    assert_eq!(
        own.palette[Role::SpectrumSecond],
        Colour::rgb(0x12, 0x34, 0x56)
    );
    assert_eq!(own.palette[Role::Accent], Colour::rgb(0x00, 0xff, 0x00));
}

#[test]
fn the_built_ins_are_listed_and_read_only() {
    let mut app = App::new();
    let scene = app.scene();
    let names: Vec<&str> = scene.theme.themes.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Dark",
            "Light",
            "High contrast",
            "Midnight Purple",
            "Deep Turquoise",
            "Graphite",
            "Nocturne"
        ]
    );
    assert!(scene.theme.themes.iter().all(|t| t.built_in));
    // Default: follow the system with Light and Nocturne; the system starts dark.
    assert_eq!(scene.theme.name, "Nocturne");
    assert!(scene.theme.follows_system());
    assert_eq!(scene.palette, Palette::nocturne());
}

#[test]
fn editing_a_built_in_makes_a_copy_and_uses_it() {
    let mut app = App::new();
    app.send(Event::SetThemeColour {
        theme: DARK,
        role: Role::Accent,
        colour: RED,
    });
    let scene = app.scene();
    assert_eq!(scene.theme.name, "Dark copy");
    assert_eq!(scene.palette[Role::Accent], RED);
    let writes = app.flush();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].file_name, "dark-copy.toml");
    let dark = &scene.theme.themes[DARK];
    assert!(dark.built_in && dark.name == "Dark");
    assert_eq!(Theme::built_ins()[DARK].palette, Palette::dark());

    // Styling too, and the next edit goes to the same copy.
    let copy = app.index_of("Dark copy");
    app.send(Event::SetThemeStyling {
        theme: copy,
        styling: Styling {
            glow: 0.2,
            ..Styling::default()
        },
    });
    assert_eq!(app.scene().theme.name, "Dark copy");
    assert_eq!(app.scene().theme.styling.glow, 0.2);
    assert!(
        !app.scene()
            .theme
            .themes
            .iter()
            .any(|t| t.name == "Dark copy 2")
    );
}

#[test]
fn every_built_in_is_drawn_smooth_and_classic_is_one_setting_away() {
    for theme in Theme::built_ins() {
        assert_eq!(theme.styling.look, Look::Smooth, "{}", theme.name);
    }
    let mut classic = Theme::dark();
    classic.name = "Old".to_owned();
    classic.styling = Styling {
        look: Look::Classic,
        glow: 0.3,
        grid_fade: 0.25,
        gradient: 0.1,
        vignette: 0.0,
        dim_when_silent: false,
        ..Styling::default()
    };
    let back = Theme::from_toml(&classic.to_toml()).unwrap();
    assert_eq!(back, classic);
}

#[test]
fn a_theme_file_from_before_looks_loads_smooth_with_defaults() {
    let file = "version = 1\nname = \"Mine\"\n[styling]\ngap = 8\n";
    let theme = Theme::from_toml(file).unwrap();
    assert_eq!(theme.styling.gap, 8.0);
    let defaults = Styling::default();
    assert_eq!(theme.styling.look, Look::Smooth);
    assert_eq!(theme.styling.glow, defaults.glow);
    assert_eq!(theme.styling.grid_fade, defaults.grid_fade);
    assert!(theme.styling.dim_when_silent);
    // Out of range comes back in range.
    let wild = "version = 1\nname = \"Wild\"\n[styling]\nglow = 7\nvignette = -1\n";
    let theme = Theme::from_toml(wild).unwrap();
    assert_eq!((theme.styling.glow, theme.styling.vignette), (1.0, 0.0));
}

#[test]
fn the_darker_built_ins_have_a_near_black_body() {
    let luma =
        |c: Colour| 0.2126 * f32::from(c.r) + 0.7152 * f32::from(c.g) + 0.0722 * f32::from(c.b);
    for theme in &Theme::built_ins()[3..] {
        if theme.name == "Nocturne" {
            continue;
        }
        let palette = &theme.palette;
        assert!(luma(palette[Role::Background]) < 20.0, "{}", theme.name);
        assert!(
            luma(palette[Role::Panel]) < 40.0,
            "{}: dark grey panels",
            theme.name
        );
        assert!(luma(palette[Role::Text]) > 180.0, "{}", theme.name);
    }
}

#[test]
fn duplicate_makes_an_editable_copy_in_the_folder() {
    let mut app = App::new();
    app.send(Event::DuplicateTheme { theme: DARK });
    let writes = app.flush();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].file_name, "dark-copy.toml");
    let scene = app.scene();
    assert_eq!(scene.theme.name, "Dark copy", "the copy is in use");
    assert!(!scene.theme.follows_system());

    let copy = app.index_of("Dark copy");
    app.send(Event::SetThemeColour {
        theme: copy,
        role: Role::Accent,
        colour: RED,
    });
    app.send(Event::SetThemeStyling {
        theme: copy,
        styling: Styling {
            background_opacity: 0.5,
            ..Styling::default()
        },
    });
    let scene = app.scene();
    assert_eq!(scene.palette[Role::Accent], RED);
    assert_eq!(scene.theme.styling.background_opacity, 0.5);
    // Both edits end up in the one file, newest contents only.
    let writes = app.flush();
    assert_eq!(writes.len(), 1);
    let saved = Theme::from_toml(&writes[0].contents).unwrap();
    assert_eq!(saved.palette[Role::Accent], RED);

    // A second copy of Dark gets its own name and file.
    app.send(Event::DuplicateTheme { theme: DARK });
    assert_eq!(app.flush()[0].file_name, "dark-copy-2.toml");
    assert_eq!(app.scene().theme.name, "Dark copy 2");
}

#[test]
fn themes_are_read_from_the_folder() {
    let mut app = App::new();
    let mut ocean = Theme::dark();
    ocean.name = "Ocean".to_owned();
    ocean.palette[Role::Background] = Colour::rgb(0, 0x20, 0x40);
    app.folder = vec![
        ThemeFile {
            file_name: "ocean.toml".to_owned(),
            text: ocean.to_toml(),
        },
        ThemeFile {
            file_name: "broken.toml".to_owned(),
            text: "name = ".to_owned(),
        },
    ];
    app.rescan();
    let scene = app.scene();
    let listed: Vec<&str> = scene.theme.themes.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(listed[7..], ["Ocean"], "after the built-ins");
    let ocean_index = app.index_of("Ocean");
    app.send(Event::ChooseTheme {
        light: ocean_index,
        dark: ocean_index,
    });
    assert_eq!(app.scene().palette, ocean.palette);
}

#[test]
fn a_users_theme_named_like_a_new_built_in_is_kept_as_a_copy() {
    let mut app = App::new();
    let mut mine = Theme::light();
    mine.name = "Graphite".to_owned();
    app.folder = vec![ThemeFile {
        file_name: "graphite.toml".to_owned(),
        text: mine.to_toml(),
    }];
    app.rescan();
    let scene = app.scene();
    let listed: Vec<&str> = scene.theme.themes.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(listed[7..], ["Graphite copy"]);
}

#[test]
fn the_pair_follows_the_system_appearance() {
    let mut app = App::new();
    app.send(Event::ChooseTheme {
        light: LIGHT,
        dark: HIGH_CONTRAST,
    });
    assert_eq!(app.scene().theme.name, "High contrast");
    app.send(Event::SystemAppearance(Appearance::Light));
    let scene = app.scene();
    assert_eq!(scene.theme.name, "Light");
    assert_eq!(scene.palette, Palette::light());
    app.send(Event::SystemAppearance(Appearance::Dark));
    let scene = app.scene();
    assert_eq!(scene.palette, Palette::high_contrast());
    assert!(
        scene.theme.styling.shape_cues,
        "High contrast adds shape cues"
    );
}

#[test]
fn a_missing_theme_falls_back_to_dark_and_returns() {
    let mut app = App::new();
    app.send(Event::DuplicateTheme { theme: LIGHT });
    app.flush();
    app.rescan();
    assert_eq!(app.scene().theme.name, "Light copy");

    // The file goes away: Dark is drawn, the reference is kept.
    let saved = app.folder.clone();
    app.folder.clear();
    app.rescan();
    let scene = app.scene();
    assert_eq!(scene.theme.name, "Light copy (missing)");
    assert_eq!(scene.palette, Palette::dark());
    assert_eq!(scene.theme.current, None);

    // It comes back: it's used again.
    app.folder = saved;
    app.rescan();
    let scene = app.scene();
    assert_eq!(scene.theme.name, "Light copy");
    assert_eq!(scene.palette, Palette::light());
}

#[test]
fn a_meters_override_wins_over_the_theme() {
    let mut app = App::new();
    app.send(Event::SetOverride {
        meter: WAVEFORM,
        role: Role::WaveformLow,
        colour: Some(RED),
    });
    let scene = app.scene();
    let waveform = &scene.windows[0].meters[WAVEFORM];
    assert_eq!(waveform.overrides, [(Role::WaveformLow, RED)]);
    assert_eq!(
        scene.palette.with(&waveform.overrides)[Role::WaveformLow],
        RED
    );
    // Other Meters and the Theme are untouched; switching Theme keeps the override.
    assert!(scene.windows[0].meters[1].overrides.is_empty());
    app.send(Event::ChooseTheme {
        light: LIGHT,
        dark: LIGHT,
    });
    let scene = app.scene();
    assert_eq!(scene.windows[0].meters[WAVEFORM].overrides.len(), 1);
    // Back to the Theme's colour.
    app.send(Event::SetOverride {
        meter: WAVEFORM,
        role: Role::WaveformLow,
        colour: None,
    });
    assert!(app.scene().windows[0].meters[WAVEFORM].overrides.is_empty());
}

#[test]
fn a_folder_theme_can_be_renamed_and_the_choice_follows() {
    let mut app = App::new();
    app.send(Event::DuplicateTheme { theme: DARK });
    app.flush();
    let copy = app.index_of("Dark copy");

    app.send(Event::RenameTheme {
        theme: copy,
        name: "  Studio night ",
    });
    let scene = app.scene();
    assert_eq!(
        scene.theme.name, "Studio night",
        "trimmed, and still in use"
    );
    assert_eq!(scene.theme.themes[copy].name, "Studio night");
    assert_eq!(app.core.current_data().theme.light, "Studio night");
    assert_eq!(app.core.current_data().theme.dark, "Studio night");
    // The same file, now with the new name.
    let writes = app.flush();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].file_name, "dark-copy.toml");
    assert_eq!(
        Theme::from_toml(&writes[0].contents).unwrap().name,
        "Studio night"
    );
    // A rescan finds it under its new name and keeps it chosen.
    app.rescan();
    assert_eq!(app.scene().theme.name, "Studio night");

    // A name another Theme has gets " (2)"; built-ins and empty names are refused.
    app.send(Event::RenameTheme {
        theme: copy,
        name: "Light",
    });
    assert_eq!(app.scene().theme.name, "Light (2)");
    app.send(Event::RenameTheme {
        theme: copy,
        name: "   ",
    });
    app.send(Event::RenameTheme {
        theme: LIGHT,
        name: "Day",
    });
    let names: Vec<String> = app
        .scene()
        .theme
        .themes
        .iter()
        .map(|t| t.name.clone())
        .collect();
    assert_eq!(names[7..], ["Light (2)"], "after the built-ins");
}
