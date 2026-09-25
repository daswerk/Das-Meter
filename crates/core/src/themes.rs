//! The Theme library: the built-ins and the themes folder's files, which Theme
//! (or light/dark pair) is chosen, and the files edits write.

use crate::theme::{Colour, DARK, LIGHT, Role, Styling, Theme};

/// A file in the themes folder, as the shell read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeFile {
    pub file_name: String,
    pub text: String,
}

/// A file the shell should write into the themes folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileWrite {
    pub file_name: String,
    pub contents: String,
}

/// The OS's light or dark appearance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Appearance {
    Light,
    #[default]
    Dark,
}

/// A Theme as the settings list it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeInfo {
    pub name: String,
    /// Built-ins are read-only: Duplicate them to edit.
    pub built_in: bool,
}

/// Which Theme is in use, for the scene.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeScene {
    /// The Theme shown, or "<name> (missing)" while its file is gone.
    pub name: String,
    pub styling: Styling,
    pub themes: Vec<ThemeInfo>,
    /// The chosen light and dark Themes, as indexes into `themes`; `None`
    /// while missing. The same index twice means one Theme, not following
    /// the system.
    pub light: Option<usize>,
    pub dark: Option<usize>,
    /// The Theme in use now, if it isn't missing.
    pub current: Option<usize>,
}

impl ThemeScene {
    pub fn follows_system(&self) -> bool {
        self.light != self.dark || self.light.is_none()
    }
}

struct Entry {
    theme: Theme,
    built_in: bool,
    file: Option<String>,
}

/// The library and the choice; see the module docs.
pub struct Themes {
    entries: Vec<Entry>,
    /// Names, not indexes: a missing Theme is kept and used again when it returns.
    light: String,
    dark: String,
    appearance: Appearance,
    writes: Vec<FileWrite>,
}

impl Default for Themes {
    fn default() -> Self {
        Themes::new()
    }
}

impl Themes {
    /// The built-ins, following the system with Light and Dark.
    pub fn new() -> Themes {
        Themes {
            entries: Theme::built_ins()
                .into_iter()
                .map(|theme| Entry {
                    theme,
                    built_in: true,
                    file: None,
                })
                .collect(),
            light: LIGHT.to_owned(),
            dark: DARK.to_owned(),
            appearance: Appearance::Dark,
            writes: Vec::new(),
        }
    }

    /// Replaces the folder's Themes with `files`. Files that can't be read, or
    /// whose name another Theme has, are skipped.
    pub fn load(&mut self, files: &[ThemeFile]) {
        self.entries.retain(|e| e.built_in);
        for file in files {
            let Ok(theme) = Theme::from_toml(&file.text) else {
                continue;
            };
            if self.find(&theme.name).is_some() {
                continue;
            }
            self.entries.push(Entry {
                theme,
                built_in: false,
                file: Some(file.file_name.clone()),
            });
        }
    }

    pub fn set_appearance(&mut self, appearance: Appearance) -> bool {
        let changed = self.appearance != appearance;
        self.appearance = appearance;
        changed
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.theme.name == name)
    }

    /// The name the OS appearance picks.
    fn wanted(&self) -> &str {
        match self.appearance {
            Appearance::Light => &self.light,
            Appearance::Dark => &self.dark,
        }
    }

    /// The Theme in use: the chosen one, or Dark while it's missing.
    pub fn current(&self) -> &Theme {
        match self.find(self.wanted()) {
            Some(i) => &self.entries[i].theme,
            None => &self.entries[0].theme,
        }
    }

    /// Chooses Themes by their index in the list: one for both, or a pair
    /// that follows the system.
    pub fn choose(&mut self, light: usize, dark: usize) -> bool {
        let (Some(l), Some(d)) = (self.entries.get(light), self.entries.get(dark)) else {
            return false;
        };
        let (light, dark) = (l.theme.name.clone(), d.theme.name.clone());
        if (light == self.light) && (dark == self.dark) {
            return false;
        }
        self.light = light;
        self.dark = dark;
        true
    }

    fn unique_name(&self, base: &str) -> String {
        let first = format!("{base} copy");
        if self.find(&first).is_none() {
            return first;
        }
        (2..)
            .map(|n| format!("{base} copy {n}"))
            .find(|name| self.find(name).is_none())
            .expect("some number is free")
    }

    fn file_name_for(&self, name: &str) -> String {
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
        let slug = slug.trim_matches('-');
        let slug = if slug.is_empty() { "theme" } else { slug };
        let taken = |f: &str| self.entries.iter().any(|e| e.file.as_deref() == Some(f));
        let first = format!("{slug}.toml");
        if !taken(&first) {
            return first;
        }
        (2..)
            .map(|n| format!("{slug}-{n}.toml"))
            .find(|f| !taken(f))
            .expect("some number is free")
    }

    /// The chosen light and dark Themes' names (the same twice for one Theme).
    pub fn choice(&self) -> (String, String) {
        (self.light.clone(), self.dark.clone())
    }

    /// Chooses Themes by name; a name no Theme has yet is kept (missing).
    pub fn choose_names(&mut self, light: &str, dark: &str) {
        self.light = light.to_owned();
        self.dark = dark.to_owned();
    }

    /// Copies a Theme into an editable one in the themes folder and uses it.
    /// Returns the copy's index.
    pub fn duplicate(&mut self, index: usize) -> Option<usize> {
        let source = self.entries.get(index)?;
        let name = self.unique_name(&source.theme.name);
        let file = self.file_name_for(&name);
        let theme = Theme {
            name: name.clone(),
            ..source.theme.clone()
        };
        self.writes.push(FileWrite {
            file_name: file.clone(),
            contents: theme.to_toml(),
        });
        self.entries.push(Entry {
            theme,
            built_in: false,
            file: Some(file),
        });
        self.light.clone_from(&name);
        self.dark = name;
        Some(self.entries.len() - 1)
    }

    /// Edits one of the folder's Themes; built-ins can't be changed.
    fn edit(&mut self, index: usize, change: impl FnOnce(&mut Theme)) -> bool {
        let Some(entry) = self.entries.get_mut(index).filter(|e| !e.built_in) else {
            return false;
        };
        let before = entry.theme.clone();
        change(&mut entry.theme);
        if entry.theme == before {
            return false;
        }
        let file_name = entry.file.clone().expect("folder Themes have a file");
        let contents = entry.theme.to_toml();
        // Only the newest contents of a file need writing.
        self.writes.retain(|w| w.file_name != file_name);
        self.writes.push(FileWrite {
            file_name,
            contents,
        });
        true
    }

    pub fn set_colour(&mut self, index: usize, role: Role, colour: Colour) -> bool {
        self.edit(index, |theme| theme.palette[role] = colour)
    }

    pub fn set_styling(&mut self, index: usize, styling: Styling) -> bool {
        self.edit(index, |theme| theme.styling = styling.clamped())
    }

    /// The files to write since the last call.
    pub fn take_writes(&mut self) -> Vec<FileWrite> {
        std::mem::take(&mut self.writes)
    }

    pub fn scene(&self) -> ThemeScene {
        let current = self.find(self.wanted());
        let name = match current {
            Some(i) => self.entries[i].theme.name.clone(),
            None => format!("{} (missing)", self.wanted()),
        };
        ThemeScene {
            name,
            styling: self.current().styling,
            themes: self
                .entries
                .iter()
                .map(|e| ThemeInfo {
                    name: e.theme.name.clone(),
                    built_in: e.built_in,
                })
                .collect(),
            light: self.find(&self.light),
            dark: self.find(&self.dark),
            current,
        }
    }
}
