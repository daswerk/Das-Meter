//! Targets: named references for the Tonal Balance Meter, one file each in
//! the targets folder. A Target keeps each of its songs' figures (never any
//! audio), so removing a song recomputes it exactly.

use dasmeter_analysis::tonal_balance::{
    Combined, KEY_CONFIDENCE, Key, KeyEstimate, Parts, SongShape, Summary, combine, tonal_grid,
};
use serde::{Deserialize, Serialize};

use crate::library::{Library, LibraryItem};

/// The file layout's version.
pub const TARGET_VERSION: u32 = 1;

/// One song a Target was taught with: its figures as the Tonal Balance
/// analysis gave them, rounded to 0.1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TargetSong {
    /// Its file's name, or the name typed when it was taught by playing.
    pub name: String,
    /// Seconds of (non-silent) audio counted.
    pub seconds: f64,
    /// The loudness its curve was normalised by, about LUFS.
    pub loudness: f32,
    /// Which parts of it counted.
    pub parts: Parts,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<Key>,
    /// How sure the key detection was, 0–1.
    #[serde(default)]
    pub key_confidence: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_crest: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bass_crest: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub low_end_mono: Option<f32>,
    /// Level per region (Sub, Bass, Low-mid, Mid, High) relative to the loudness.
    pub regions: [f32; 5],
    /// Level per point of the Tonal Balance grid, relative to the loudness.
    pub curve: Vec<f32>,
}

fn tenth(value: f32) -> f32 {
    (value * 10.0).round() / 10.0
}

impl TargetSong {
    pub fn new(name: &str, parts: Parts, summary: &Summary) -> TargetSong {
        TargetSong {
            name: name.trim().to_owned(),
            seconds: summary.seconds,
            loudness: tenth(summary.loudness),
            parts,
            key: summary.key.map(|k| k.key),
            key_confidence: summary
                .key
                .map_or(0.0, |k| (k.confidence * 100.0).round() / 100.0),
            sub_crest: summary.low_end_crest[0].map(tenth),
            bass_crest: summary.low_end_crest[1].map(tenth),
            low_end_mono: summary.low_end_mono.map(|m| (m * 100.0).round() / 100.0),
            regions: summary.regions.map(tenth),
            curve: summary.curve.iter().copied().map(tenth).collect(),
        }
    }

    /// Its key, when the detection was sure enough to align by.
    pub fn sure_key(&self) -> Option<Key> {
        self.key.filter(|_| self.key_confidence >= KEY_CONFIDENCE)
    }

    fn shape(&self) -> SongShape {
        SongShape {
            curve: self.curve.clone(),
            regions: self.regions,
            key: self.key.map(|key| KeyEstimate {
                key,
                confidence: self.key_confidence,
            }),
            low_end_crest: [self.sub_crest, self.bass_crest],
            low_end_mono: self.low_end_mono,
        }
    }
}

/// A Target as saved: its name and its songs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Target {
    pub version: u32,
    pub name: String,
    /// How many points the songs' curves have, so a file says what grid
    /// its numbers are on.
    pub points: usize,
    #[serde(default)]
    pub songs: Vec<TargetSong>,
}

impl Target {
    pub fn new(name: &str) -> Target {
        Target {
            version: TARGET_VERSION,
            name: name.to_owned(),
            points: tonal_grid().len(),
            songs: Vec::new(),
        }
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("a Target always serialises")
    }

    /// Reads a file; one from a newer version or on another grid is refused.
    pub fn from_toml(text: &str) -> Result<Target, String> {
        let target: Target = toml::from_str(text).map_err(|e| e.to_string())?;
        if target.version > TARGET_VERSION {
            return Err("made by a newer Das-Meter".into());
        }
        if target.name.trim().is_empty() {
            return Err("the Target has no name".into());
        }
        let points = tonal_grid().len();
        let finite = |s: &TargetSong| {
            s.curve.iter().chain(&s.regions).all(|v| v.is_finite()) && s.seconds.is_finite()
        };
        if target.points != points
            || target.songs.iter().any(|s| {
                s.curve.len() != points || !finite(s) || s.key.is_some_and(|k| k.tonic > 11)
            })
        {
            return Err("not on the Tonal Balance grid".into());
        }
        Ok(target)
    }

    /// Its songs combined, the low end in the key most of its songs are in
    /// ([`home_key`](Self::home_key)); `None` while it has no songs.
    pub fn combined(&self) -> Option<Combined> {
        let shapes: Vec<SongShape> = self.songs.iter().map(TargetSong::shape).collect();
        let combined = combine(&shapes)?;
        Some(match self.home_key() {
            Some(key) => combined.in_key(key),
            None => combined,
        })
    }

    /// The Target with only its song at `index` (the last if there's none there).
    pub fn only(&self, index: usize) -> Target {
        let song = self.songs.get(index).or(self.songs.last()).cloned();
        Target {
            songs: song.into_iter().collect(),
            ..self.clone()
        }
    }

    /// The key most of its songs are in, of those whose key is sure; the
    /// first song's on a tie. `None` when no song's key is sure.
    pub fn home_key(&self) -> Option<Key> {
        let keys: Vec<Key> = self.songs.iter().filter_map(TargetSong::sure_key).collect();
        let count = |key: &Key| keys.iter().filter(|k| *k == key).count();
        let most = keys.iter().map(count).max()?;
        keys.iter().copied().find(|k| count(k) == most)
    }
}

impl LibraryItem for Target {
    const UNNAMED: &'static str = "Target";

    fn name(&self) -> &str {
        &self.name
    }

    fn set_name(&mut self, name: String) {
        self.name = name;
    }

    fn to_toml(&self) -> String {
        Target::to_toml(self)
    }

    fn from_toml(text: &str) -> Result<Self, String> {
        Target::from_toml(text)
    }

    fn same_contents(&self, other: &Self) -> bool {
        self.songs == other.songs
    }
}

/// The targets folder as the app knows it, and the files edits write.
pub type Targets = Library<Target>;

impl Targets {
    /// Adds a song to the Target called `target`, made if there's none.
    /// Returns the name the Target has.
    pub fn add_song(&mut self, target: &str, song: TargetSong) -> String {
        let mut item = self
            .get(target.trim())
            .cloned()
            .unwrap_or_else(|| Target::new(target));
        item.songs.push(song);
        self.save(item)
    }

    /// Removes a Target's song by its place in the list; returns whether it did.
    pub fn remove_song(&mut self, target: &str, song: usize) -> bool {
        if self.get(target).is_none_or(|t| song >= t.songs.len()) {
            return false;
        }
        self.edit(target, |t| {
            t.songs.remove(song);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::themes::ThemeFile;

    fn song(name: &str, level: f32, key: Option<(u8, f32)>) -> TargetSong {
        let points = tonal_grid().len();
        TargetSong {
            name: name.into(),
            seconds: 180.0,
            loudness: -9.0,
            parts: Parts::Loud,
            key: key.map(|(tonic, _)| Key { tonic, minor: true }),
            key_confidence: key.map_or(0.0, |(_, c)| c),
            sub_crest: Some(9.5),
            bass_crest: None,
            low_end_mono: Some(0.95),
            regions: [level; 5],
            curve: vec![level; points],
        }
    }

    #[test]
    fn a_target_reads_back_as_written() {
        let mut target = Target::new("Techno");
        target.songs.push(song("One", -20.0, Some((9, 0.4))));
        target.songs.push(song("Two", -22.0, None));
        let read = Target::from_toml(&target.to_toml()).unwrap();
        assert_eq!(read, target);
    }

    #[test]
    fn a_target_on_another_grid_is_refused() {
        let mut target = Target::new("Short");
        let mut short = song("One", -20.0, None);
        short.curve.pop();
        target.songs.push(short);
        assert!(Target::from_toml(&target.to_toml()).is_err());
        let mut newer = Target::new("Newer");
        newer.version = TARGET_VERSION + 1;
        assert!(Target::from_toml(&newer.to_toml()).is_err());
    }

    #[test]
    fn the_home_key_is_the_most_common_sure_key() {
        let mut target = Target::new("Mixed");
        target.songs.push(song("Unsure", -20.0, Some((2, 0.05))));
        assert_eq!(target.home_key(), None);
        target.songs.push(song("A", -20.0, Some((9, 0.5))));
        target.songs.push(song("F", -20.0, Some((5, 0.5))));
        target.songs.push(song("F again", -20.0, Some((5, 0.3))));
        assert_eq!(
            target.home_key(),
            Some(Key {
                tonic: 5,
                minor: true
            })
        );
        let combined = target.combined().unwrap();
        assert_eq!(combined.key.tonic, 5);
        assert_eq!(combined.songs, 4);
        assert_eq!(combined.aligned, 3);
    }

    #[test]
    fn songs_are_added_and_removed_through_files() {
        let mut targets = Targets::default();
        assert_eq!(
            targets.add_song("  Techno ", song("One", -20.0, None)),
            "Techno"
        );
        targets.add_song("Techno", song("Two", -24.0, None));
        let writes = targets.take_writes();
        assert_eq!(writes.len(), 1, "one file, its newest contents");
        assert_eq!(writes[0].file_name, "techno.toml");
        let combined = targets.get("Techno").unwrap().combined().unwrap();
        assert_eq!(combined.regions[0].average, -22.0);

        assert!(targets.remove_song("Techno", 0));
        assert!(!targets.remove_song("Techno", 5));
        assert_eq!(targets.get("Techno").unwrap().songs[0].name, "Two");

        let mut read = Targets::default();
        read.load(&[ThemeFile {
            file_name: "techno.toml".into(),
            text: targets.take_writes()[0].contents.clone(),
        }]);
        assert_eq!(read.get("Techno").unwrap().songs.len(), 1);
    }
}
