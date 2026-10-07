//! Presets: saved setups as TOML files in the presets folder, the built-ins,
//! `settings.toml`, and what the app does with them (switch, auto-save,
//! Revert, manage).
//!
//! The core never touches the disk: the shell hands it the folder's files and
//! carries out the [`PresetOp`]s it returns, so tests run on files in memory.

use serde::{Deserialize, Serialize};

use crate::layout::{BarLayout, LayoutMode, Platform};
use crate::meters::MeterSettings;
use crate::panes::{Direction, Node, WindowLayout};
use crate::settings::DEFAULT_FRAME_RATE_CAP;
use crate::sources::{ListenTo, Pick};
use crate::theme::{Colour, LIGHT, NOCTURNE, Role};

/// The Preset file layout's version.
pub const PRESET_VERSION: u32 = 1;
/// The `settings.toml` layout's version.
pub const SETTINGS_VERSION: u32 = 1;
/// The settings file's name, next to the presets folder.
pub const SETTINGS_FILE: &str = "settings.toml";

/// The built-in Presets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuiltIn {
    /// Bar mode at the bottom edge with four Meters.
    Bar,
    /// Window mode: Spectrum and Phase Scope on top; Waveform, Loudness
    /// Meter and Stereometer below.
    Mixing,
    /// Window mode: a large Loudness Meter, Spectrum and Stereometer beside it.
    Mastering,
    /// Window mode: a large Loudness Meter with its graph, beside Spectrum,
    /// Spectrogram, Waveform and Stereometer.
    #[serde(rename = "mastering-advanced")]
    MasteringAdvanced,
    /// Window mode: Spectrum on top; Phase Scope, Cepstrum and a small
    /// Loudness Meter below.
    Producing,
    /// A thin Bar: Loudness numbers, Spectrum and Stereometer.
    Compact,
}

impl BuiltIn {
    pub const ALL: [BuiltIn; 6] = [
        BuiltIn::Bar,
        BuiltIn::Mixing,
        BuiltIn::Mastering,
        BuiltIn::MasteringAdvanced,
        BuiltIn::Producing,
        BuiltIn::Compact,
    ];
    /// The built-ins added after the first three: an older install gets
    /// them once.
    const LATER: [BuiltIn; 3] = [
        BuiltIn::MasteringAdvanced,
        BuiltIn::Producing,
        BuiltIn::Compact,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BuiltIn::Bar => "Bar",
            BuiltIn::Mixing => "Mixing",
            BuiltIn::Mastering => "Mastering",
            BuiltIn::MasteringAdvanced => "Mastering advanced",
            BuiltIn::Producing => "Producing",
            BuiltIn::Compact => "Compact",
        }
    }

    fn file_name(self) -> String {
        format!("{}.toml", self.name().to_lowercase().replace(' ', "-"))
    }
}

/// The set of built-ins copied in: 1 the first three, 2 adds Mastering
/// advanced, Producing and Compact, and the Mixing with a Phase Scope.
const BUILT_INS_VERSION: u32 = 2;
/// The Bar Compact docks: thin.
const COMPACT_THICKNESS: f32 = 90.0;

/// A Meter as a Preset saves it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeterPreset {
    pub settings: MeterSettings,
    /// The Send Plugin it picked: found again by ID, then by name, else waited for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_plugin: Option<Pick>,
    /// A Phase Scope's Overlay Source, found again the same way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlay: Option<Pick>,
    #[serde(default)]
    pub show_source_label: bool,
    /// Its own colours for single roles, over the Theme's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overrides: Vec<RoleColour>,
}

/// One colour role override.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleColour {
    pub role: Role,
    pub colour: Colour,
}

/// The Theme a Preset uses: one name twice, or a light/dark pair.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeRef {
    pub light: String,
    pub dark: String,
}

impl Default for ThemeRef {
    fn default() -> Self {
        ThemeRef {
            light: LIGHT.to_owned(),
            dark: NOCTURNE.to_owned(),
        }
    }
}

/// Everything a Preset holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PresetData {
    pub version: u32,
    pub name: String,
    /// Which built-in this was copied from, for Reset to built-in. (Its own
    /// default: a file without it is no built-in, whatever the struct default.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_in: Option<BuiltIn>,
    pub mode: LayoutMode,
    pub listen_to: ListenTo,
    pub theme: ThemeRef,
    pub bar: BarLayout,
    pub window: WindowLayout,
    pub meters: Vec<MeterPreset>,
}

impl Default for PresetData {
    fn default() -> Self {
        PresetData::built_in(BuiltIn::Bar)
    }
}

fn default_meters() -> Vec<MeterPreset> {
    use crate::meters::MeterKind::*;
    meters_of(&[Waveform, Spectrum, Stereometer, Loudness])
}

/// Waveform 0, Loudness Meter 3 and Stereometer 2 side by side.
fn mixing_row() -> Node {
    use Direction::SideBySide;
    Node::split(
        SideBySide,
        1.0 / 3.0,
        Node::Pane(0),
        Node::split(SideBySide, 0.5, Node::Pane(3), Node::Pane(2)),
    )
}

/// Each kind's default Meter, in order.
fn meters_of(kinds: &[crate::meters::MeterKind]) -> Vec<MeterPreset> {
    kinds
        .iter()
        .map(|&kind| MeterSettings::default_of(kind))
        .map(|settings| MeterPreset {
            settings,
            send_plugin: None,
            overlay: None,
            show_source_label: false,
            overrides: Vec::new(),
        })
        .collect()
}

impl PresetData {
    /// A built-in, as this version of Das-Meter defines it.
    pub fn built_in(kind: BuiltIn) -> PresetData {
        use crate::meters::MeterKind::*;
        use Direction::{SideBySide, Stacked};
        let (meters, tree) = match kind {
            // Waveform 0, Spectrum 1, Stereometer 2, Loudness Meter 3.
            BuiltIn::Bar => (default_meters(), WindowLayout::mixing().tree),
            BuiltIn::Mastering => (
                default_meters(),
                Node::split(
                    SideBySide,
                    0.55,
                    Node::Pane(3),
                    Node::split(Stacked, 0.5, Node::Pane(1), Node::Pane(2)),
                ),
            ),
            BuiltIn::Mixing => {
                let mut meters = default_meters();
                meters.extend(meters_of(&[PhaseScope]));
                let top = Node::split(SideBySide, 0.6, Node::Pane(1), Node::Pane(4));
                (meters, Node::split(Stacked, 0.5, top, mixing_row()))
            }
            BuiltIn::MasteringAdvanced => {
                let meters = meters_of(&[Loudness, Spectrum, Spectrogram, Stereometer, Waveform]);
                let bottom = Node::split(SideBySide, 0.6, Node::Pane(4), Node::Pane(3));
                let right = Node::split(
                    Stacked,
                    1.0 / 3.0,
                    Node::Pane(1),
                    Node::split(Stacked, 0.5, Node::Pane(2), bottom),
                );
                (meters, Node::split(SideBySide, 0.4, Node::Pane(0), right))
            }
            BuiltIn::Producing => {
                let meters = meters_of(&[Spectrum, PhaseScope, Cepstrum, Loudness]);
                let bottom = Node::split(
                    SideBySide,
                    0.45,
                    Node::Pane(1),
                    Node::split(SideBySide, 0.65, Node::Pane(2), Node::Pane(3)),
                );
                (meters, Node::split(Stacked, 0.5, Node::Pane(0), bottom))
            }
            BuiltIn::Compact => {
                let mut meters = meters_of(&[Loudness, Spectrum, Stereometer]);
                // The numbers only: no room for the graph in a thin Bar.
                if let MeterSettings::Loudness(loudness) = &mut meters[0].settings {
                    loudness.show_history = false;
                }
                let tree = Node::split(
                    SideBySide,
                    1.0 / 3.0,
                    Node::Pane(0),
                    Node::split(SideBySide, 0.5, Node::Pane(1), Node::Pane(2)),
                );
                (meters, tree)
            }
        };
        let mut bar = BarLayout::new(meters.len(), Platform::current());
        if kind == BuiltIn::Compact {
            bar.thickness = COMPACT_THICKNESS;
        }
        PresetData {
            version: PRESET_VERSION,
            name: kind.name().to_owned(),
            built_in: Some(kind),
            mode: match kind {
                BuiltIn::Bar | BuiltIn::Compact => LayoutMode::Bar,
                _ => LayoutMode::Window,
            },
            listen_to: ListenTo::SystemCapture,
            theme: ThemeRef::default(),
            bar,
            window: WindowLayout {
                frame: None,
                display: None,
                on_top: false,
                tree,
            },
            meters,
        }
    }

    /// Whether this is the Mixing built-in as the first version defined it,
    /// untouched but for where its window sits: it gets the new one.
    fn is_first_mixing(&self) -> bool {
        let settings: Vec<&MeterSettings> = self.meters.iter().map(|m| &m.settings).collect();
        let first: Vec<MeterSettings> = default_meters().into_iter().map(|m| m.settings).collect();
        self.built_in == Some(BuiltIn::Mixing)
            && self.mode == LayoutMode::Window
            && self.window.tree == WindowLayout::mixing().tree
            && settings.iter().copied().eq(first.iter())
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("a Preset always serialises")
    }

    /// Keeps the layouts pointing at Meters that exist.
    pub fn sanitised(mut self) -> PresetData {
        if self.meters.is_empty() {
            self.meters = default_meters();
        }
        let count = self.meters.len();
        self.bar.meters.retain(|(m, _)| *m < count);
        self.bar.pop_outs.retain(|p| p.meter < count);
        if self.bar.meters.is_empty() {
            let share = 1.0 / count as f32;
            self.bar.meters = (0..count).map(|m| (m, share)).collect();
        }
        let total: f32 = self.bar.meters.iter().map(|(_, s)| s.max(0.0)).sum();
        for (_, share) in &mut self.bar.meters {
            *share = if total > 0.0 {
                share.max(0.0) / total
            } else {
                1.0 / count as f32
            };
        }
        if self.window.tree.meters().iter().any(|&m| m >= count) {
            self.window.tree = Node::Pane(0);
        }
        self
    }
}

/// How a file was read.
enum Read {
    Current(PresetData),
    /// From an older version: the migrated data, and the original to keep as `.bak`.
    Migrated(PresetData),
    /// From a newer version: what this version understands. Read-only.
    Newer(PresetData),
    Broken,
}

/// Reads a Preset file, migrating older versions step by step.
fn read(text: &str) -> Read {
    let Ok(mut table) = text.parse::<toml::Table>() else {
        return Read::Broken;
    };
    let version = table
        .get("version")
        .and_then(toml::Value::as_integer)
        .unwrap_or(0);
    let migrated = version < i64::from(PRESET_VERSION);
    if version == 0 {
        migrate_0_to_1(&mut table);
    }
    let Ok(data) = toml::Value::Table(table).try_into::<PresetData>() else {
        return Read::Broken;
    };
    if data.name.trim().is_empty() {
        return Read::Broken;
    }
    let data = data.sanitised();
    if version > i64::from(PRESET_VERSION) {
        Read::Newer(data)
    } else if migrated {
        Read::Migrated(PresetData {
            version: PRESET_VERSION,
            ..data
        })
    } else {
        Read::Current(data)
    }
}

/// Version 0 (pre-release files, no `version` key) called the layout mode
/// `layout = "bar" | "window"`.
fn migrate_0_to_1(table: &mut toml::Table) {
    if let Some(layout) = table.remove("layout") {
        let mode = match layout.as_str() {
            Some("window") => "Window",
            _ => "Bar",
        };
        table.insert("mode".into(), mode.into());
    }
    table.insert("version".into(), 1.into());
}

/// A file in the presets folder, as the shell read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetFile {
    pub file_name: String,
    pub text: String,
}

/// What the shell should do on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresetOp {
    /// Write a file in the presets folder (to a temporary file, then renamed).
    Write { file_name: String, contents: String },
    /// Write a file only if there's none by that name (the one-time `.bak`).
    WriteIfMissing { file_name: String, contents: String },
    /// Move a Preset file to the OS trash.
    Trash { file_name: String },
    /// Write `settings.toml`.
    Settings { contents: String },
}

/// What first launch shows once per install (see `onboarding`).
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FirstLaunch {
    pub built_ins_copied: bool,
    /// Which set of built-ins was copied in (see `BUILT_INS_VERSION`); 0 in
    /// a file from before the count was kept, which means the first three.
    pub built_ins_version: u32,
    /// The welcome card was answered (Start listening or ✕).
    pub welcome_shown: bool,
    /// Start listening was pressed: System Capture may run.
    pub listening_started: bool,
    pub send_plugin_note_shown: bool,
    pub virtual_desktop_tip_shown: bool,
}

/// An opt-in global hotkey for a Preset. None are assigned by default.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hotkey {
    /// The Preset's file.
    pub preset: String,
    /// The key combination, such as "ctrl+alt+1".
    pub keys: String,
}

/// `settings.toml`: app settings that aren't in a Preset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StoredSettings {
    pub version: u32,
    /// The Preset that opens at launch: the last one used.
    pub last_preset: Option<String>,
    /// The Presets' order in the list (their files); others follow by name.
    pub order: Vec<String>,
    /// Off by default, and never prompted for.
    pub launch_at_login: bool,
    pub check_for_updates: bool,
    /// When the update check last ran, in seconds since the Unix epoch.
    pub last_update_check: u64,
    pub frame_rate_cap: u32,
    pub show_in_dock: bool,
    pub hotkeys: Vec<Hotkey>,
    pub first_launch: FirstLaunch,
}

impl Default for StoredSettings {
    fn default() -> Self {
        StoredSettings {
            version: SETTINGS_VERSION,
            last_preset: None,
            order: Vec::new(),
            launch_at_login: false,
            check_for_updates: true,
            last_update_check: 0,
            frame_rate_cap: DEFAULT_FRAME_RATE_CAP,
            show_in_dock: true,
            hotkeys: Vec::new(),
            first_launch: FirstLaunch::default(),
        }
    }
}

/// A Preset as the menus list it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetInfo {
    /// Its name, or "<file> (can't be read)".
    pub name: String,
    pub file_name: String,
    /// Copied from a built-in: it can be reset.
    pub built_in: bool,
    /// From a newer Das-Meter: shown but never saved over.
    pub read_only: bool,
    /// Couldn't be read: listed, never opened or deleted.
    pub broken: bool,
}

/// The Presets for the scene.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PresetScene {
    pub list: Vec<PresetInfo>,
    /// The Preset in use, by index into `list`.
    pub current: Option<usize>,
    /// The Preset changed since it was opened: Revert is offered.
    pub can_revert: bool,
}

enum State {
    Ok {
        data: Box<PresetData>,
        read_only: bool,
    },
    Broken,
}

struct Entry {
    file: String,
    state: State,
}

impl Entry {
    fn data(&self) -> Option<&PresetData> {
        match &self.state {
            State::Ok { data, .. } => Some(data),
            State::Broken => None,
        }
    }
}

/// The Preset library; see the module docs.
#[derive(Default)]
pub struct Presets {
    entries: Vec<Entry>,
    current: Option<usize>,
    /// The current Preset as it was opened, for Revert.
    opened: Option<PresetData>,
    pub settings: StoredSettings,
    ops: Vec<PresetOp>,
}

impl Presets {
    /// Reads the folder and `settings.toml`. On first launch (no settings, or
    /// built-ins not yet copied) the built-ins are copied in. Returns the
    /// Preset to open: the last used, else the first.
    pub fn load(&mut self, files: &[PresetFile], settings: Option<&str>) -> Option<PresetData> {
        self.settings = settings
            .and_then(|text| toml::from_str(text).ok())
            .unwrap_or_default();
        self.entries.clear();
        for file in files {
            if !file.file_name.ends_with(".toml") {
                continue;
            }
            let state = match read(&file.text) {
                Read::Current(data) => State::Ok {
                    data: Box::new(data),
                    read_only: false,
                },
                Read::Migrated(data) => {
                    self.ops.push(PresetOp::WriteIfMissing {
                        file_name: format!("{}.bak", file.file_name),
                        contents: file.text.clone(),
                    });
                    self.ops.push(PresetOp::Write {
                        file_name: file.file_name.clone(),
                        contents: data.to_toml(),
                    });
                    State::Ok {
                        data: Box::new(data),
                        read_only: false,
                    }
                }
                Read::Newer(data) => State::Ok {
                    data: Box::new(data),
                    read_only: true,
                },
                Read::Broken => State::Broken,
            };
            self.entries.push(Entry {
                file: file.file_name.clone(),
                state,
            });
        }
        if !self.settings.first_launch.built_ins_copied {
            for kind in BuiltIn::ALL {
                let file = kind.file_name();
                if self.entries.iter().any(|e| e.file == file) {
                    continue;
                }
                let data = PresetData::built_in(kind);
                self.ops.push(PresetOp::Write {
                    file_name: file.clone(),
                    contents: data.to_toml(),
                });
                self.entries.push(Entry {
                    file,
                    state: State::Ok {
                        data: Box::new(data),
                        read_only: false,
                    },
                });
            }
            self.settings.first_launch.built_ins_copied = true;
            self.settings.first_launch.built_ins_version = BUILT_INS_VERSION;
            if self.settings.order.is_empty() {
                self.settings.order = BuiltIn::ALL.map(BuiltIn::file_name).to_vec();
            }
        } else if self.settings.first_launch.built_ins_version < BUILT_INS_VERSION {
            self.add_later_built_ins();
        }
        self.sort();
        let last = self.settings.last_preset.clone();
        let index = last
            .and_then(|file| self.position(&file))
            .filter(|&i| self.entries[i].data().is_some())
            .or_else(|| self.entries.iter().position(|e| e.data().is_some()));
        self.open(index)
    }

    /// The list in the saved order; files not in it follow, by name.
    fn sort(&mut self) {
        let order = &self.settings.order;
        let rank = |file: &str| order.iter().position(|f| f == file).unwrap_or(usize::MAX);
        self.entries.sort_by(|a, b| {
            rank(&a.file)
                .cmp(&rank(&b.file))
                .then_with(|| a.file.cmp(&b.file))
        });
        self.settings.order = self.entries.iter().map(|e| e.file.clone()).collect();
    }

    /// For an install from before the later built-ins: copies them in once,
    /// at the end of the list, and makes an untouched first Mixing the new
    /// one.
    fn add_later_built_ins(&mut self) {
        for kind in BuiltIn::LATER {
            let file = kind.file_name();
            if self.position(&file).is_none() {
                let data = PresetData::built_in(kind);
                self.ops.push(PresetOp::Write {
                    file_name: file.clone(),
                    contents: data.to_toml(),
                });
                self.entries.push(Entry {
                    file: file.clone(),
                    state: State::Ok {
                        data: Box::new(data),
                        read_only: false,
                    },
                });
            }
            // An empty order lists by name: the new ones fit in there too.
            if !self.settings.order.is_empty() && !self.settings.order.contains(&file) {
                self.settings.order.push(file);
            }
        }
        let mixing = BuiltIn::Mixing.file_name();
        if let Some(entry) = self.entries.iter_mut().find(|e| e.file == mixing)
            && let State::Ok {
                data,
                read_only: false,
            } = &mut entry.state
            && data.is_first_mixing()
        {
            let mut new = PresetData::built_in(BuiltIn::Mixing);
            // The first four Meters are the same ones: their picks, labels
            // and colours stay, and so does the Bar, with the Phase Scope added.
            for (now, before) in new.meters.iter_mut().zip(&data.meters) {
                *now = before.clone();
            }
            let added = new.meters.len() - 1;
            new.bar = data.bar.clone();
            if new.bar.meters.iter().all(|&(m, _)| m != added) {
                new.bar.meters.push((added, 1.0 / new.meters.len() as f32));
                let total: f32 = new.bar.meters.iter().map(|(_, s)| s).sum();
                for (_, share) in &mut new.bar.meters {
                    *share /= total;
                }
            }
            new.name = data.name.clone();
            new.listen_to = data.listen_to;
            new.theme = data.theme.clone();
            new.window.frame = data.window.frame;
            new.window.display = data.window.display.clone();
            new.window.on_top = data.window.on_top;
            self.ops.push(PresetOp::Write {
                file_name: mixing,
                contents: new.to_toml(),
            });
            **data = new;
        }
        self.settings.first_launch.built_ins_version = BUILT_INS_VERSION;
        self.save_settings();
    }

    fn position(&self, file: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.file == file)
    }

    pub(crate) fn save_settings(&mut self) {
        self.ops
            .retain(|op| !matches!(op, PresetOp::Settings { .. }));
        self.ops.push(PresetOp::Settings {
            contents: toml::to_string(&self.settings).expect("settings always serialise"),
        });
    }

    /// Makes `index` the current Preset and returns its data to apply.
    fn open(&mut self, index: Option<usize>) -> Option<PresetData> {
        let data = index.and_then(|i| self.entries[i].data().cloned());
        self.current = index.filter(|_| data.is_some());
        self.opened.clone_from(&data);
        self.settings.last_preset = self.current.map(|i| self.entries[i].file.clone());
        self.save_settings();
        data
    }

    fn read_only(&self, index: usize) -> bool {
        matches!(
            self.entries[index].state,
            State::Ok {
                read_only: true,
                ..
            }
        )
    }

    /// Saves `data` over the current Preset if it changed (never over a
    /// read-only one). Returns whether it wrote.
    pub fn save_current(&mut self, data: PresetData) -> bool {
        let Some(i) = self.current else { return false };
        if self.read_only(i) {
            return false;
        }
        let entry = &mut self.entries[i];
        let State::Ok { data: saved, .. } = &mut entry.state else {
            return false;
        };
        // The name and built-in origin belong to the file, not the app state.
        let data = PresetData {
            name: saved.name.clone(),
            built_in: saved.built_in,
            ..data
        };
        if **saved == data {
            return false;
        }
        **saved = data;
        let contents = saved.to_toml();
        let file_name = entry.file.clone();
        self.ops
            .retain(|op| !matches!(op, PresetOp::Write { file_name: f, .. } if *f == file_name));
        self.ops.push(PresetOp::Write {
            file_name,
            contents,
        });
        true
    }

    /// Switches to the Preset at `index`. The caller saves the current one first.
    pub fn switch(&mut self, index: usize) -> Option<PresetData> {
        if index >= self.entries.len() || Some(index) == self.current {
            return None;
        }
        self.entries[index].data()?;
        self.open(Some(index))
    }

    /// Cmd/Ctrl+1–9: the n-th Preset in list order.
    pub fn shortcut(&mut self, number: usize) -> Option<PresetData> {
        self.switch(number.checked_sub(1)?)
    }

    /// The current Preset as it was opened.
    pub fn opened(&self) -> Option<&PresetData> {
        self.opened.as_ref()
    }

    fn unique_name(&self, base: &str) -> String {
        let taken = |n: &str| {
            self.entries
                .iter()
                .any(|e| e.data().is_some_and(|d| d.name == n))
        };
        if !taken(base) {
            return base.to_owned();
        }
        (2..)
            .map(|n| format!("{base} ({n})"))
            .find(|name| !taken(name))
            .expect("some number is free")
    }

    fn file_for(&self, name: &str) -> String {
        let slug: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect();
        // One dash between words, however many other characters stood there.
        let slug = slug
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let slug = if slug.is_empty() { "preset" } else { &slug };
        let first = format!("{slug}.toml");
        if self.position(&first).is_none() {
            return first;
        }
        (2..)
            .map(|n| format!("{slug}-{n}.toml"))
            .find(|f| self.position(f).is_none())
            .expect("some number is free")
    }

    /// Adds a new Preset after `after` holding `data` under a free name.
    fn add(&mut self, data: PresetData, after: Option<usize>) -> usize {
        let name = self.unique_name(&data.name);
        let file = self.file_for(&name);
        let data = PresetData {
            name,
            built_in: None,
            version: PRESET_VERSION,
            ..data
        };
        self.ops.push(PresetOp::Write {
            file_name: file.clone(),
            contents: data.to_toml(),
        });
        let at = after.map_or(self.entries.len(), |i| i + 1);
        self.entries.insert(
            at,
            Entry {
                file,
                state: State::Ok {
                    data: Box::new(data),
                    read_only: false,
                },
            },
        );
        if let Some(current) = self.current.filter(|&c| c >= at) {
            self.current = Some(current + 1);
        }
        self.settings.order = self.entries.iter().map(|e| e.file.clone()).collect();
        at
    }

    /// Save as new: the app's current state as a new Preset, which becomes current.
    pub fn save_as_new(&mut self, data: PresetData) -> Option<PresetData> {
        let base = self
            .current
            .and_then(|i| self.entries[i].data())
            .map_or_else(|| "Preset".to_owned(), |d| d.name.clone());
        let at = self.add(PresetData { name: base, ..data }, self.current);
        self.open(Some(at))
    }

    /// An imported Preset: added at the end under a free name, and opened.
    pub fn import(&mut self, data: PresetData) -> Option<PresetData> {
        let at = self.add(data, None);
        self.open(Some(at))
    }

    /// The current Preset's name.
    pub fn current_name(&self) -> Option<String> {
        let i = self.current?;
        self.entries[i].data().map(|d| d.name.clone())
    }

    /// A copy of the Preset at `index`, listed after it.
    pub fn duplicate(&mut self, index: usize) -> bool {
        let Some(data) = self.entries.get(index).and_then(Entry::data).cloned() else {
            return false;
        };
        self.add(data, Some(index));
        self.save_settings();
        true
    }

    /// Renames the Preset at `index`; a taken name gets " (2)".
    pub fn rename(&mut self, index: usize, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || index >= self.entries.len() || self.read_only(index) {
            return false;
        }
        if self.entries[index].data().is_none_or(|d| d.name == name) {
            return false;
        }
        let name = self.unique_name(name);
        let entry = &mut self.entries[index];
        let State::Ok { data, .. } = &mut entry.state else {
            return false;
        };
        data.name = name;
        let contents = data.to_toml();
        self.ops.push(PresetOp::Write {
            file_name: entry.file.clone(),
            contents,
        });
        if Some(index) == self.current
            && let Some(opened) = &mut self.opened
        {
            opened.name = data_name(&self.entries[index]);
        }
        true
    }

    /// A Theme was renamed: every Preset that uses it follows.
    pub fn rename_theme(&mut self, old: &str, new: &str) {
        let follow = |theme: &mut ThemeRef| {
            let mut changed = false;
            for name in [&mut theme.light, &mut theme.dark] {
                if name == old {
                    *name = new.to_owned();
                    changed = true;
                }
            }
            changed
        };
        for entry in &mut self.entries {
            let State::Ok {
                data,
                read_only: false,
            } = &mut entry.state
            else {
                continue;
            };
            if follow(&mut data.theme) {
                self.ops.push(PresetOp::Write {
                    file_name: entry.file.clone(),
                    contents: data.to_toml(),
                });
            }
        }
        if let Some(opened) = &mut self.opened {
            follow(&mut opened.theme);
        }
    }

    /// Moves the Preset at `from` to `to` in the list.
    pub fn reorder(&mut self, from: usize, to: usize) -> bool {
        if from >= self.entries.len() || to >= self.entries.len() || from == to {
            return false;
        }
        let entry = self.entries.remove(from);
        self.entries.insert(to, entry);
        if let Some(current) = self.current {
            self.current = Some(if current == from {
                to
            } else if from < current && current <= to {
                current - 1
            } else if to <= current && current < from {
                current + 1
            } else {
                current
            });
        }
        self.settings.order = self.entries.iter().map(|e| e.file.clone()).collect();
        self.save_settings();
        true
    }

    /// Deletes the Preset at `index` to the trash. Deleting the current one
    /// switches to the one above (returned to apply). The last readable Preset,
    /// and unreadable files, are never deleted.
    pub fn delete(&mut self, index: usize) -> Option<PresetData> {
        let readable = self.entries.iter().filter(|e| e.data().is_some()).count();
        match self.entries.get(index) {
            Some(entry) if entry.data().is_some() && readable > 1 => {}
            _ => return None,
        }
        let entry = self.entries.remove(index);
        self.ops.push(PresetOp::Trash {
            file_name: entry.file,
        });
        self.settings.order = self.entries.iter().map(|e| e.file.clone()).collect();
        match self.current {
            Some(current) if current == index => {
                // The one above, or the new first; skipping unreadable ones.
                let above = (0..index.min(self.entries.len()))
                    .rev()
                    .chain(index..self.entries.len())
                    .find(|&i| self.entries[i].data().is_some());
                self.open(above)
            }
            Some(current) if current > index => {
                self.current = Some(current - 1);
                self.save_settings();
                None
            }
            _ => {
                self.save_settings();
                None
            }
        }
    }

    /// Reset to built-in: the Preset at `index` takes this version's built-in
    /// again. Returns the data to apply if it's the current Preset.
    pub fn reset(&mut self, index: usize) -> Option<PresetData> {
        let kind = self
            .entries
            .get(index)
            .and_then(Entry::data)
            .and_then(|d| d.built_in)?;
        let data = PresetData::built_in(kind);
        let file_name = self.entries[index].file.clone();
        self.ops.push(PresetOp::Write {
            file_name,
            contents: data.to_toml(),
        });
        self.entries[index].state = State::Ok {
            data: Box::new(data.clone()),
            read_only: false,
        };
        if Some(index) == self.current {
            self.opened = Some(data.clone());
            Some(data)
        } else {
            None
        }
    }

    pub fn current_index(&self) -> Option<usize> {
        self.current
    }

    /// The disk work since the last call.
    pub fn take_ops(&mut self) -> Vec<PresetOp> {
        std::mem::take(&mut self.ops)
    }

    pub fn scene(&self, changed_since_opened: bool) -> PresetScene {
        PresetScene {
            list: self
                .entries
                .iter()
                .map(|e| match &e.state {
                    State::Ok { data, read_only } => PresetInfo {
                        name: if *read_only {
                            format!("{} (newer, read-only)", data.name)
                        } else {
                            data.name.clone()
                        },
                        file_name: e.file.clone(),
                        built_in: data.built_in.is_some(),
                        read_only: *read_only,
                        broken: false,
                    },
                    State::Broken => PresetInfo {
                        name: format!("{} (can't be read)", e.file),
                        file_name: e.file.clone(),
                        built_in: false,
                        read_only: false,
                        broken: true,
                    },
                })
                .collect(),
            current: self.current,
            can_revert: changed_since_opened,
        }
    }
}

fn data_name(entry: &Entry) -> String {
    entry.data().map(|d| d.name.clone()).unwrap_or_default()
}
