//! Reference curves: saved long-term spectra with names, one file each in
//! the curves folder, which a Spectrum draws behind its live trace.

use dasmeter_analysis::{REFERENCE_POINTS_PER_OCTAVE, REFERENCE_RANGE, reference_grid};
use serde::{Deserialize, Serialize};

use crate::themes::{FileWrite, ThemeFile};

/// The file layout's version.
pub const CURVE_VERSION: u32 = 1;

/// A saved long-term spectrum: its level at each frequency of the Reference
/// grid (1/24 octave from 20 Hz to 20 kHz), in dB, without the Spectrum's slope.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReferenceCurve {
    pub version: u32,
    pub name: String,
    /// The grid it's on, so a file says what its numbers mean.
    pub lowest_hz: f32,
    pub points_per_octave: f32,
    /// Seconds of audio (silence left out) it was averaged over.
    pub seconds: f64,
    pub levels: Vec<f32>,
}

impl ReferenceCurve {
    pub fn new(name: &str, seconds: f64, levels: Vec<f32>) -> ReferenceCurve {
        ReferenceCurve {
            version: CURVE_VERSION,
            name: name.to_owned(),
            lowest_hz: REFERENCE_RANGE.0,
            points_per_octave: REFERENCE_POINTS_PER_OCTAVE,
            seconds,
            levels: levels.iter().map(|l| (l * 10.0).round() / 10.0).collect(),
        }
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("a Reference curve always serialises")
    }

    /// Reads a file; one from a newer version, on another grid or with the
    /// wrong number of levels is refused.
    pub fn from_toml(text: &str) -> Result<ReferenceCurve, String> {
        let curve: ReferenceCurve = toml::from_str(text).map_err(|e| e.to_string())?;
        if curve.version > CURVE_VERSION {
            return Err("made by a newer Das-Meter".into());
        }
        if curve.name.trim().is_empty() {
            return Err("the curve has no name".into());
        }
        if curve.lowest_hz != REFERENCE_RANGE.0
            || curve.points_per_octave != REFERENCE_POINTS_PER_OCTAVE
            || curve.levels.len() != reference_grid().len()
            || curve.levels.iter().any(|l| !l.is_finite())
        {
            return Err("not on the Reference curve grid".into());
        }
        Ok(curve)
    }
}

struct Entry {
    curve: ReferenceCurve,
    file: String,
}

/// The curves folder as the app knows it, and the files edits write.
#[derive(Default)]
pub struct Curves {
    entries: Vec<Entry>,
    writes: Vec<FileWrite>,
}

impl Curves {
    /// Replaces what's known with the folder's files. Unreadable files, and
    /// ones whose name another curve has, are skipped.
    pub fn load(&mut self, files: &[ThemeFile]) {
        self.entries.clear();
        for file in files {
            let Ok(curve) = ReferenceCurve::from_toml(&file.text) else {
                continue;
            };
            if self.get(&curve.name).is_some() {
                continue;
            }
            self.entries.push(Entry {
                curve,
                file: file.file_name.clone(),
            });
        }
        self.entries.sort_by(|a, b| a.curve.name.cmp(&b.curve.name));
    }

    pub fn get(&self, name: &str) -> Option<&ReferenceCurve> {
        self.entries
            .iter()
            .find(|e| e.curve.name == name)
            .map(|e| &e.curve)
    }

    /// The names, sorted, for menus.
    pub fn names(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.curve.name.clone()).collect()
    }

    /// Saves a curve: over the one with its name, or as a new file. Returns
    /// the name it got (trimmed; "Reference curve" when blank).
    pub fn save(&mut self, curve: ReferenceCurve) -> String {
        let name = match curve.name.trim() {
            "" => "Reference curve".to_owned(),
            name => name.to_owned(),
        };
        let curve = ReferenceCurve {
            name: name.clone(),
            ..curve
        };
        let file = match self.entries.iter().find(|e| e.curve.name == name) {
            Some(entry) => entry.file.clone(),
            None => self.file_name_for(&name),
        };
        self.writes.push(FileWrite {
            file_name: file.clone(),
            contents: curve.to_toml(),
        });
        self.entries.retain(|e| e.curve.name != name);
        self.entries.push(Entry { curve, file });
        self.entries.sort_by(|a, b| a.curve.name.cmp(&b.curve.name));
        name
    }

    /// Adds a curve from elsewhere (an imported Preset) unless an identical
    /// one is there, under " (2)" if its name is taken. Returns its name.
    pub fn install(&mut self, curve: ReferenceCurve) -> String {
        if let Some(existing) = self.get(&curve.name)
            && existing.levels == curve.levels
        {
            return curve.name;
        }
        let name = if self.get(&curve.name).is_none() {
            curve.name.clone()
        } else {
            (2..)
                .map(|n| format!("{} ({n})", curve.name))
                .find(|name| self.get(name).is_none())
                .expect("some number is free")
        };
        self.save(ReferenceCurve { name, ..curve })
    }

    /// The files to write since the last call.
    pub fn take_writes(&mut self) -> Vec<FileWrite> {
        std::mem::take(&mut self.writes)
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
        let slug = if slug.is_empty() { "curve" } else { slug };
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

#[cfg(test)]
mod tests {
    use super::*;

    fn curve(name: &str, level: f32) -> ReferenceCurve {
        ReferenceCurve::new(name, 30.0, vec![level; reference_grid().len()])
    }

    #[test]
    fn a_curve_reads_back_as_written() {
        let original = curve("Club mix", -31.25);
        let read = ReferenceCurve::from_toml(&original.to_toml()).unwrap();
        assert_eq!(read, original);
        assert_eq!(read.levels[0], -31.3);
    }

    #[test]
    fn a_curve_on_another_grid_is_refused() {
        let mut short = curve("Short", -30.0);
        short.levels.pop();
        assert!(ReferenceCurve::from_toml(&short.to_toml()).is_err());
        let mut newer = curve("Newer", -30.0);
        newer.version = CURVE_VERSION + 1;
        assert!(ReferenceCurve::from_toml(&newer.to_toml()).is_err());
    }

    #[test]
    fn saving_writes_a_file_and_lists_it() {
        let mut curves = Curves::default();
        assert_eq!(curves.save(curve("  Club mix ", -30.0)), "Club mix");
        let writes = curves.take_writes();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].file_name, "club-mix.toml");
        assert_eq!(curves.names(), ["Club mix"]);
        // Saving under the same name writes over the same file.
        curves.save(curve("Club mix", -20.0));
        assert_eq!(curves.take_writes()[0].file_name, "club-mix.toml");
        assert_eq!(curves.get("Club mix").unwrap().levels[0], -20.0);
    }

    #[test]
    fn loading_skips_bad_files_and_duplicate_names() {
        let mut curves = Curves::default();
        let file = |name: &str, text: String| ThemeFile {
            file_name: name.into(),
            text,
        };
        curves.load(&[
            file("a.toml", curve("Mix", -30.0).to_toml()),
            file("b.toml", "not a curve".into()),
            file("c.toml", curve("Mix", -10.0).to_toml()),
        ]);
        assert_eq!(curves.names(), ["Mix"]);
        assert_eq!(curves.get("Mix").unwrap().levels[0], -30.0);
    }

    #[test]
    fn installing_reuses_an_identical_curve_and_renames_a_different_one() {
        let mut curves = Curves::default();
        curves.save(curve("Mix", -30.0));
        assert_eq!(curves.install(curve("Mix", -30.0)), "Mix");
        assert_eq!(curves.install(curve("Mix", -12.0)), "Mix (2)");
        assert_eq!(curves.names(), ["Mix", "Mix (2)"]);
    }
}
