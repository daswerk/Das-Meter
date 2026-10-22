//! The Tonal Balance Meter in the app core: its settings, its analyser
//! (frames, the live average, teaching a song by playing, the frozen ghost)
//! and its Target as the scene shows it.

use dasmeter_analysis::LongTerm;
use dasmeter_analysis::tonal_balance::{
    Band, Frame, FrameAnalyser, KEY_CONFIDENCE, Key, LiveSummary, Parts, Summary,
    WHOLE_LIMIT_FRAMES, summarise,
};

use crate::targets::Target;

/// How quickly the drawn curve follows the live average, in seconds (time
/// constant). Frames arrive every half second; easing toward each one lets
/// the curve glide instead of jumping.
pub const EASE_SECONDS: f64 = 0.15;
/// Closer than this (dB) everywhere, the drawn curve takes the average's
/// values and stops moving.
const EASE_SNAP_DB: f32 = 0.05;

/// How the Tonal Balance Meter shows its comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum TonalView {
    /// Fine in a window, Broad in the Bar. The default.
    #[default]
    Auto,
    /// The mix's curve over the Target's range, 20 Hz–20 kHz.
    Fine,
    /// Five regions, each a bar inside its range.
    Broad,
}

/// Which key the mix is taken to be in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum KeyChoice {
    /// The key detected in the mix, when the detection is sure. The default.
    #[default]
    Auto,
    Fixed(Key),
}

/// What a Tonal Balance Meter compares with in its Target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TargetPick {
    /// One of its songs, by its place in the list. The default (the first).
    Song(usize),
    /// All its songs averaged, with their spread as the range.
    Average,
}

impl Default for TargetPick {
    fn default() -> Self {
        TargetPick::Song(0)
    }
}

/// The Tonal Balance Meter: a Source's long-term spectrum, shape only,
/// against a Target (named in the Meter's slot, like a Spectrum's Reference curve).
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct TonalBalanceMeterSettings {
    pub view: TonalView,
    /// The Fine view shows only 20–300 Hz, with notes on its axis.
    pub low_end_zoom: bool,
    /// How far back the mix's average reaches. Default 10 s; never Off.
    pub averaging: LongTerm,
    /// Which parts of a song count when teaching a Target. Default Loud.
    pub parts: Parts,
    pub key: KeyChoice,
    /// The key the Target's low end is drawn in, once the user shifted it
    /// there; it stays until undone, whatever the detection says next.
    /// Default none: the Meter suggests a shift when the keys differ.
    pub shift_to: Option<Key>,
    /// One song of the Target, or all of them averaged. Default its first song.
    pub pick: TargetPick,
    /// Below this the low end's mono-ness is measured, in Hz. Default 120.
    pub mono_cutoff: f32,
}

/// The mix's average when none is chosen.
pub const DEFAULT_AVERAGING: LongTerm = LongTerm::Seconds(10.0);

impl Default for TonalBalanceMeterSettings {
    fn default() -> Self {
        TonalBalanceMeterSettings {
            view: TonalView::Auto,
            low_end_zoom: false,
            averaging: DEFAULT_AVERAGING,
            parts: Parts::Loud,
            key: KeyChoice::Auto,
            shift_to: None,
            pick: TargetPick::default(),
            mono_cutoff: 120.0,
        }
    }
}

/// The mono cutoff's range in Hz.
pub const MONO_CUTOFF_RANGE: (f32, f32) = (40.0, 300.0);
/// The sliding average's range in seconds.
pub const AVERAGING_RANGE: (f32, f32) = (1.0, 600.0);

impl TonalBalanceMeterSettings {
    pub(crate) fn clamped(mut self) -> Self {
        self.averaging = match self.averaging {
            LongTerm::Off => DEFAULT_AVERAGING,
            LongTerm::Seconds(s) if s.is_finite() => {
                LongTerm::Seconds(s.clamp(AVERAGING_RANGE.0, AVERAGING_RANGE.1))
            }
            LongTerm::Seconds(_) => DEFAULT_AVERAGING,
            LongTerm::Whole => LongTerm::Whole,
        };
        self.mono_cutoff = if self.mono_cutoff.is_finite() {
            self.mono_cutoff
                .clamp(MONO_CUTOFF_RANGE.0, MONO_CUTOFF_RANGE.1)
        } else {
            120.0
        };
        if let KeyChoice::Fixed(key) = &mut self.key {
            key.tonic %= 12;
        }
        if let Some(key) = &mut self.shift_to {
            key.tonic %= 12;
        }
        self
    }

    /// The key the mix is taken to be in: the fixed one, or the detected
    /// one when the detection is sure.
    pub fn mix_key(&self, mix: Option<&Summary>) -> Option<Key> {
        match self.key {
            KeyChoice::Fixed(key) => Some(key),
            KeyChoice::Auto => mix
                .and_then(|m| m.key)
                .filter(|k| k.confidence >= KEY_CONFIDENCE)
                .map(|k| k.key),
        }
    }
}

/// The analyser behind a Tonal Balance Meter.
pub(crate) struct TonalBalanceAnalyser {
    frames: FrameAnalyser,
    live: LiveSummary,
    averaging: LongTerm,
    mono_cutoff: f32,
    /// The live average, summed again whenever a frame arrives.
    summary: Option<Summary>,
    /// What's drawn of it: its curve and regions eased toward the average's.
    shown: Option<Summary>,
    /// While a song is being taught by playing: every frame since Learn.
    learning: Option<Vec<Frame>>,
    /// The mix's curve when frozen, drawn as a ghost.
    frozen: Option<Vec<f32>>,
    /// A file compared in place of the live Source: its name and summary.
    compare: Option<(String, Summary)>,
    /// A/B: the reference plays (B), so what comes in isn't the mix alone:
    /// nothing is added to the average, nor to a song being learned.
    paused: bool,
}

impl TonalBalanceAnalyser {
    pub fn new(sample_rate: u32, settings: &TonalBalanceMeterSettings) -> Self {
        TonalBalanceAnalyser {
            frames: FrameAnalyser::new(sample_rate, settings.mono_cutoff),
            live: LiveSummary::new(settings.averaging),
            averaging: settings.averaging,
            mono_cutoff: settings.mono_cutoff,
            summary: None,
            shown: None,
            learning: None,
            frozen: None,
            compare: None,
            paused: false,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.frames.sample_rate()
    }

    /// Takes new settings: a new cutoff measures afresh, a new averaging
    /// averages afresh.
    pub fn set_settings(&mut self, settings: &TonalBalanceMeterSettings) {
        if settings.averaging != self.averaging {
            self.averaging = settings.averaging;
            self.live = LiveSummary::new(settings.averaging);
            self.summary = None;
            self.shown = None;
        }
        if settings.mono_cutoff != self.mono_cutoff {
            self.mono_cutoff = settings.mono_cutoff;
            self.frames = FrameAnalyser::new(self.sample_rate(), settings.mono_cutoff);
        }
    }

    /// Takes audio; returns whether the view changed: a frame was added, or
    /// the drawn curve moved toward the average.
    pub fn process(&mut self, interleaved: &[f32]) -> bool {
        self.frames.process(interleaved);
        let mut frames = self.frames.take_frames();
        if self.paused {
            frames.clear();
        }
        let added = !frames.is_empty();
        for frame in frames {
            if let Some(learning) = &mut self.learning
                && learning.len() < WHOLE_LIMIT_FRAMES
            {
                learning.push(frame.clone());
            }
            self.live.push(frame);
        }
        if added {
            self.summary = self.live.summary();
        }
        let seconds = (interleaved.len() / 2) as f64 / f64::from(self.sample_rate());
        self.ease(seconds) || added
    }

    /// Moves the drawn curve `seconds` toward the live average; returns
    /// whether it moved.
    fn ease(&mut self, seconds: f64) -> bool {
        let Some(summary) = &self.summary else {
            return self.shown.take().is_some();
        };
        let Some(shown) = self
            .shown
            .as_mut()
            .filter(|s| s.curve.len() == summary.curve.len())
        else {
            self.shown = Some(summary.clone());
            return true;
        };
        let step = (1.0 - (-seconds / EASE_SECONDS).exp()) as f32;
        let mut next = summary.clone();
        let mut far = false;
        let pairs = next
            .curve
            .iter_mut()
            .zip(&shown.curve)
            .chain(next.regions.iter_mut().zip(&shown.regions));
        for (goal, &from) in pairs {
            // Silent points (−∞) and the first values show at once.
            if goal.is_finite() && from.is_finite() {
                let eased = from + (*goal - from) * step;
                far |= (*goal - eased).abs() > EASE_SNAP_DB;
                *goal = eased;
            }
        }
        if !far {
            next = summary.clone();
        }
        let moved = *shown != next;
        *shown = next;
        moved
    }

    /// What's drawn: the file being compared, or the live average as it
    /// eases toward each new frame.
    pub fn shown(&self) -> Option<&Summary> {
        match &self.compare {
            Some((_, summary)) => Some(summary),
            None => self.shown.as_ref(),
        }
    }

    /// What's compared with the Target: the file being compared, or the
    /// live average once it has heard something.
    pub fn summary(&self) -> Option<&Summary> {
        match &self.compare {
            Some((_, summary)) => Some(summary),
            None => self.summary.as_ref(),
        }
    }

    /// Compares a file in place of the live Source, or (`None`) the Source
    /// again; returns whether anything changed.
    pub fn set_compare(&mut self, compare: Option<(String, Summary)>) -> bool {
        if compare.is_none() && self.compare.is_none() {
            return false;
        }
        self.compare = compare;
        true
    }

    pub fn comparing(&self) -> Option<&str> {
        self.compare.as_ref().map(|(name, _)| name.as_str())
    }

    pub fn reset(&mut self) {
        self.live.reset();
        self.summary = None;
        self.shown = None;
    }

    pub fn learn(&mut self) {
        self.learning = Some(Vec::new());
    }

    pub fn cancel_learning(&mut self) -> bool {
        self.learning.take().is_some()
    }

    /// Seconds heard since Learn, while learning.
    pub fn learning_seconds(&self) -> Option<f64> {
        self.learning
            .as_ref()
            .map(|f| f.len() as f64 * dasmeter_analysis::tonal_balance::FRAME_SECONDS)
    }

    /// Ends learning: what was heard since Learn, summed. `None` if it
    /// wasn't learning or heard only silence (learning goes on then).
    pub fn take_learned(&mut self, parts: Parts) -> Option<Summary> {
        let summary = summarise(self.learning.as_ref()?, parts)?;
        self.learning = None;
        Some(summary)
    }

    /// Freezes the mix's curve as a ghost, or lets it go; returns whether
    /// anything changed.
    pub fn toggle_freeze(&mut self) -> bool {
        if self.frozen.take().is_some() {
            return true;
        }
        self.frozen = self.summary.as_ref().map(|s| s.curve.clone());
        self.frozen.is_some()
    }

    pub fn frozen(&self) -> Option<&Vec<f32>> {
        self.frozen.as_ref()
    }

    /// A/B: B pauses the average; A carries on. Returns whether it changed.
    pub fn set_paused(&mut self, paused: bool) -> bool {
        std::mem::replace(&mut self.paused, paused) != paused
    }
}

/// A Tonal Balance Meter's state for its menu and drawing.
#[derive(Clone, Debug, PartialEq)]
pub struct TonalBalanceScene {
    /// The Target it compares with, if it has one.
    pub target: Option<TargetScene>,
    /// While a song is being taught by playing: the whole seconds heard.
    pub learning: Option<u32>,
    /// Whether a ghost of the curve is frozen.
    pub frozen: bool,
    /// The file compared in place of the live Source, by name.
    pub comparing: Option<String>,
    /// Files being analysed for it, and those that couldn't be.
    pub upload: Option<UploadScene>,
    /// A/B listening, when the picked song has an audio file to play.
    pub ab: Option<AbScene>,
}

/// A/B listening on a Tonal Balance Meter: your mix (A) or the picked song (B).
#[derive(Clone, Debug, PartialEq)]
pub struct AbScene {
    /// B: the song plays (and a Send Plugin listened to goes quiet).
    pub b: bool,
    /// The song's audio file.
    pub file: String,
    /// How much to turn the song up (or down) to match the mix's loudness,
    /// in dB, to 0.1; 0 until the mix has been heard.
    pub gain_db: f32,
    /// The part of the song B loops, in seconds; `None` for all of it.
    pub region: Option<(f32, f32)>,
    /// The song's waveform, once read from its file.
    pub overview: Option<std::sync::Arc<SongOverview>>,
    /// Where B is in the song, in seconds, while it plays.
    pub position: Option<f32>,
}

/// A song's waveform at a glance: its length and its peak level (0–1) in
/// each of [`OVERVIEW_POINTS`] equal stretches.
#[derive(Clone, Debug, PartialEq)]
pub struct SongOverview {
    pub seconds: f32,
    pub peaks: Vec<f32>,
}

/// How many stretches a [`SongOverview`] has.
pub const OVERVIEW_POINTS: usize = 600;
/// The shortest region B loops, in seconds.
pub const AB_MIN_REGION: f32 = 0.5;

/// The most the gain match turns a song up or down, in dB.
pub const AB_GAIN_RANGE: (f32, f32) = (-30.0, 12.0);

/// Audio files being analysed for a Tonal Balance Meter, in the background.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct UploadScene {
    pub done: usize,
    pub total: usize,
    /// Each file that couldn't be read, with why.
    pub failed: Vec<(String, String)>,
}

/// A song of a Target, as menus list it.
#[derive(Clone, Debug, PartialEq)]
pub struct TargetSongInfo {
    pub name: String,
    pub seconds: f64,
    pub key: Option<Key>,
}

/// The Target a Tonal Balance Meter compares with, for the scene.
#[derive(Clone, Debug, PartialEq)]
pub struct TargetScene {
    pub name: String,
    /// What's compared: one song (its place, clamped to the list) or the average.
    pub pick: TargetPick,
    /// Its file is gone; it's kept and used again when it returns.
    pub missing: bool,
    pub songs: Vec<TargetSongInfo>,
    /// Per point of the Tonal Balance grid: its average and range, the
    /// low end in [`key`](Self::key). Empty while it has no songs.
    pub curve: Vec<Band>,
    /// Per region; `None` while it has no songs.
    pub regions: Option<[Band; 5]>,
    pub low_end_crest: [Option<Band>; 2],
    pub low_end_mono: Option<Band>,
    /// The key its low end is drawn in, if its songs' keys are known.
    pub key: Option<Key>,
    /// The key most of its songs are in.
    pub home_key: Option<Key>,
    /// The mix is in another key: suggest shifting the Target to it.
    pub suggest_shift: Option<Key>,
    /// The key it was shifted to, which stays until undone.
    pub shifted_to: Option<Key>,
    /// The picked song's audio file and loudness (about LUFS), when one
    /// song is picked and it came from a file.
    pub song_file: Option<(String, f32)>,
}

impl TargetScene {
    /// What it compares with, as the picker names it: the song's name, or
    /// the Target's for its average.
    pub fn label(&self) -> String {
        match self.pick {
            TargetPick::Song(i) => self
                .songs
                .get(i)
                .map_or_else(|| self.name.clone(), |s| s.name.clone()),
            TargetPick::Average if self.songs.len() > 1 => {
                format!("{} (average of {})", self.name, self.songs.len())
            }
            TargetPick::Average => self.name.clone(),
        }
    }
}

/// To 0.1 dB: finer would only redraw for nothing.
fn round_band(b: Band) -> Band {
    Band {
        average: (b.average * 10.0).round() / 10.0,
        range: (b.range * 10.0).round() / 10.0,
    }
}

impl TargetScene {
    /// `target` as a Meter with these settings and this mix shows it.
    pub(crate) fn new(
        name: String,
        target: Option<&Target>,
        settings: &TonalBalanceMeterSettings,
        mix: Option<&Summary>,
    ) -> TargetScene {
        let Some(target) = target else {
            return TargetScene {
                name,
                pick: settings.pick,
                missing: true,
                songs: Vec::new(),
                curve: Vec::new(),
                regions: None,
                low_end_crest: [None; 2],
                low_end_mono: None,
                key: None,
                home_key: None,
                suggest_shift: None,
                shifted_to: settings.shift_to,
                song_file: None,
            };
        };
        // One song (clamped to the list), or all of them.
        let pick = match settings.pick {
            TargetPick::Song(i) => TargetPick::Song(i.min(target.songs.len().saturating_sub(1))),
            TargetPick::Average => TargetPick::Average,
        };
        let picked = match pick {
            TargetPick::Song(i) => target.only(i),
            TargetPick::Average => target.clone(),
        };
        let home_key = picked.home_key();
        let mix_key = settings.mix_key(mix);
        let mut combined = picked.combined();
        let mut key = home_key;
        let mut suggest_shift = None;
        match (settings.shift_to, home_key, mix_key) {
            // Shifted: it stays in that key until undone.
            (Some(to), Some(home), _) => {
                if home.tonic != to.tonic {
                    combined = combined.map(|c| c.in_key(to));
                }
                key = Some(to);
            }
            (None, Some(home), Some(mix_key)) if home.tonic != mix_key.tonic => {
                suggest_shift = Some(mix_key);
            }
            _ => {}
        }
        TargetScene {
            name,
            pick,
            missing: false,
            songs: target
                .songs
                .iter()
                .map(|s| TargetSongInfo {
                    name: s.name.clone(),
                    seconds: s.seconds,
                    key: s.sure_key(),
                })
                .collect(),
            curve: combined
                .as_ref()
                .map(|c| c.curve.iter().copied().map(round_band).collect())
                .unwrap_or_default(),
            regions: combined.as_ref().map(|c| c.regions.map(round_band)),
            low_end_crest: combined
                .as_ref()
                .map_or([None; 2], |c| c.low_end_crest.map(|b| b.map(round_band))),
            low_end_mono: combined.as_ref().and_then(|c| c.low_end_mono),
            key,
            home_key,
            suggest_shift,
            shifted_to: settings.shift_to.filter(|_| home_key.is_some()),
            song_file: match pick {
                TargetPick::Song(_) => picked
                    .songs
                    .first()
                    .and_then(|s| Some((s.file.clone()?, s.loudness))),
                TargetPick::Average => None,
            },
        }
    }
}
