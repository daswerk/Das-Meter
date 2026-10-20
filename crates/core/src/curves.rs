//! Reference curves: saved long-term spectra with names, one file each in
//! the curves folder, which a Spectrum draws behind its live trace.

use dasmeter_analysis::{REFERENCE_POINTS_PER_OCTAVE, REFERENCE_RANGE, reference_grid};
use serde::{Deserialize, Serialize};

use crate::library::{Library, LibraryItem};

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

impl LibraryItem for ReferenceCurve {
    const UNNAMED: &'static str = "Reference curve";

    fn name(&self) -> &str {
        &self.name
    }

    fn set_name(&mut self, name: String) {
        self.name = name;
    }

    fn to_toml(&self) -> String {
        ReferenceCurve::to_toml(self)
    }

    fn from_toml(text: &str) -> Result<Self, String> {
        ReferenceCurve::from_toml(text)
    }

    fn same_contents(&self, other: &Self) -> bool {
        self.levels == other.levels
    }
}

/// The curves folder as the app knows it, and the files edits write.
pub type Curves = Library<ReferenceCurve>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::ThemeFile;

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
