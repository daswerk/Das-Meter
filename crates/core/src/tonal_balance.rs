//! The Tonal Balance Meter in the app core: its settings, its analyser
//! (frames, the live average, teaching a song by playing, the frozen ghost)
//! and its Target as the scene shows it.

use dasmeter_analysis::LongTerm;
use dasmeter_analysis::tonal_balance::{
    Band, Frame, FrameAnalyser, KEY_CONFIDENCE, Key, LiveSummary, Parts, Summary,
    WHOLE_LIMIT_FRAMES, summarise,
};

use crate::targets::Target;

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
    /// Draw the Target's low end in the mix's key. Default off: the
    /// Meter suggests it when the keys differ.
    pub shift_to_key: bool,
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
            shift_to_key: false,
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
    /// While a song is being taught by playing: every frame since Learn.
    learning: Option<Vec<Frame>>,
    /// The mix's curve when frozen, drawn as a ghost.
    frozen: Option<Vec<f32>>,
}

impl TonalBalanceAnalyser {
    pub fn new(sample_rate: u32, settings: &TonalBalanceMeterSettings) -> Self {
        TonalBalanceAnalyser {
            frames: FrameAnalyser::new(sample_rate, settings.mono_cutoff),
            live: LiveSummary::new(settings.averaging),
            averaging: settings.averaging,
            mono_cutoff: settings.mono_cutoff,
            summary: None,
            learning: None,
            frozen: None,
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
        }
        if settings.mono_cutoff != self.mono_cutoff {
            self.mono_cutoff = settings.mono_cutoff;
            self.frames = FrameAnalyser::new(self.sample_rate(), settings.mono_cutoff);
        }
    }

    /// Takes audio; returns whether a frame was added (the view changed).
    pub fn process(&mut self, interleaved: &[f32]) -> bool {
        self.frames.process(interleaved);
        let frames = self.frames.take_frames();
        if frames.is_empty() {
            return false;
        }
        for frame in frames {
            if let Some(learning) = &mut self.learning
                && learning.len() < WHOLE_LIMIT_FRAMES
            {
                learning.push(frame.clone());
            }
            self.live.push(frame);
        }
        self.summary = self.live.summary();
        true
    }

    pub fn summary(&self) -> Option<&Summary> {
        self.summary.as_ref()
    }

    pub fn reset(&mut self) {
        self.live.reset();
        self.summary = None;
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
                missing: true,
                songs: Vec::new(),
                curve: Vec::new(),
                regions: None,
                low_end_crest: [None; 2],
                low_end_mono: None,
                key: None,
                home_key: None,
                suggest_shift: None,
            };
        };
        let home_key = target.home_key();
        let mix_key = settings.mix_key(mix);
        let mut combined = target.combined();
        let mut key = home_key;
        let mut suggest_shift = None;
        if let (Some(home), Some(mix_key)) = (home_key, mix_key)
            && home.tonic != mix_key.tonic
        {
            if settings.shift_to_key {
                combined = combined.map(|c| c.in_key(mix_key));
                key = Some(mix_key);
            } else {
                suggest_shift = Some(mix_key);
            }
        }
        TargetScene {
            name,
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
        }
    }
}
