//! A folder of named TOML files, one item each, as the app knows it: the
//! Reference curves and the Targets. Edits become files for the shell to write.

use crate::themes::{FileWrite, ThemeFile};

/// An item kept as one file in a library folder.
pub trait LibraryItem: Clone {
    /// The name a blank one gets, and its files' name when the name has no
    /// letters or digits.
    const UNNAMED: &'static str;
    fn name(&self) -> &str;
    fn set_name(&mut self, name: String);
    fn to_toml(&self) -> String;
    fn from_toml(text: &str) -> Result<Self, String>;
    /// The same but for the name: installing it again adds nothing.
    fn same_contents(&self, other: &Self) -> bool;
}

struct Entry<T> {
    item: T,
    file: String,
}

/// The folder's items, sorted by name, and the files edits write.
pub struct Library<T> {
    entries: Vec<Entry<T>>,
    writes: Vec<FileWrite>,
}

impl<T> Default for Library<T> {
    fn default() -> Self {
        Library {
            entries: Vec::new(),
            writes: Vec::new(),
        }
    }
}

impl<T: LibraryItem> Library<T> {
    /// Replaces what's known with the folder's files. Unreadable files, and
    /// ones whose name another item has, are skipped.
    pub fn load(&mut self, files: &[ThemeFile]) {
        self.entries.clear();
        for file in files {
            let Ok(item) = T::from_toml(&file.text) else {
                continue;
            };
            if self.get(item.name()).is_some() {
                continue;
            }
            self.entries.push(Entry {
                item,
                file: file.file_name.clone(),
            });
        }
        self.sort();
    }

    pub fn get(&self, name: &str) -> Option<&T> {
        self.entries
            .iter()
            .find(|e| e.item.name() == name)
            .map(|e| &e.item)
    }

    /// The names, sorted, for menus.
    pub fn names(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|e| e.item.name().to_owned())
            .collect()
    }

    /// Saves an item: over the one with its name, or as a new file. Returns
    /// the name it got (trimmed; [`LibraryItem::UNNAMED`] when blank).
    pub fn save(&mut self, mut item: T) -> String {
        let name = match item.name().trim() {
            "" => T::UNNAMED.to_owned(),
            name => name.to_owned(),
        };
        item.set_name(name.clone());
        let file = match self.entries.iter().find(|e| e.item.name() == name) {
            Some(entry) => entry.file.clone(),
            None => self.file_name_for(&name),
        };
        // Only the newest contents of a file need writing.
        self.writes.retain(|w| w.file_name != file);
        self.writes.push(FileWrite {
            file_name: file.clone(),
            contents: item.to_toml(),
        });
        self.entries.retain(|e| e.item.name() != name);
        self.entries.push(Entry { item, file });
        self.sort();
        name
    }

    /// Changes the item called `name` and saves it; returns whether there was one.
    pub fn edit(&mut self, name: &str, change: impl FnOnce(&mut T)) -> bool {
        let Some(mut item) = self.get(name).cloned() else {
            return false;
        };
        change(&mut item);
        item.set_name(name.to_owned());
        self.save(item);
        true
    }

    /// Adds an item from elsewhere (an imported Preset) unless an identical
    /// one is there, under " (2)" if its name is taken. Returns its name.
    pub fn install(&mut self, mut item: T) -> String {
        if let Some(existing) = self.get(item.name())
            && existing.same_contents(&item)
        {
            return item.name().to_owned();
        }
        if self.get(item.name()).is_some() {
            let base = item.name().to_owned();
            let name = (2..)
                .map(|n| format!("{base} ({n})"))
                .find(|name| self.get(name).is_none())
                .expect("some number is free");
            item.set_name(name);
        }
        self.save(item)
    }

    /// The files to write since the last call.
    pub fn take_writes(&mut self) -> Vec<FileWrite> {
        std::mem::take(&mut self.writes)
    }

    fn sort(&mut self) {
        self.entries
            .sort_by(|a, b| a.item.name().cmp(b.item.name()));
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
        let fallback = T::UNNAMED.to_ascii_lowercase().replace(' ', "-");
        let slug = if slug.is_empty() { &fallback } else { slug };
        let taken = |file: &str| self.entries.iter().any(|e| e.file == file);
        let first = format!("{slug}.toml");
        if !taken(&first) {
            return first;
        }
        (2..)
            .map(|n| format!("{slug}-{n}.toml"))
            .find(|file| !taken(file))
            .expect("some number is free")
    }
}
