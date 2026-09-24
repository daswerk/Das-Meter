//! The scene description: everything the shell needs to draw, and nothing more.
//!
//! Values are rounded to what is shown, so a scene only changes when something
//! visible changes. That is what lets the app core decide to sleep.

use dasmeter_analysis::{ChannelLevels, LoudnessReadings};

/// Everything on screen: the windows and the notes shown over them.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub windows: Vec<WindowScene>,
    /// Brief notes, such as "Output changed: reset".
    pub notes: Vec<Note>,
}

/// One window and the Meters in it.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowScene {
    pub title: String,
    pub meters: Vec<MeterScene>,
}

/// One Meter's content.
#[derive(Clone, Debug, PartialEq)]
pub enum MeterScene {
    Loudness(LoudnessScene),
}

/// What the Loudness Meter shows.
#[derive(Clone, Debug, PartialEq)]
pub enum LoudnessScene {
    /// System Capture hasn't delivered a sample rate yet.
    Starting,
    /// System Capture couldn't start; the reason is shown in place of the Meter.
    Unavailable(String),
    Live(LoudnessDisplay),
}

/// A brief note shown over the Meters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Note {
    /// The output device or its rate changed, so the readings started over.
    OutputChanged,
}

impl Note {
    pub fn text(self) -> &'static str {
        match self {
            Note::OutputChanged => "Output changed: reset",
        }
    }
}

/// Loudness readings as shown: in 0.1 dB steps, with anything below the floor as silence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoudnessDisplay {
    pub sample_rate: u32,
    /// Momentary, short-term and integrated LUFS. [`Level::Silent`] below the −70 LUFS gate.
    pub momentary: Level,
    pub short_term: Level,
    pub integrated: Level,
    /// Loudness range in LU.
    pub range: Level,
    /// Highest true peak since the last reset, in dBTP.
    pub true_peak_max: Level,
    /// False when the true peak is only a sample peak (192 kHz and above).
    pub true_peak_oversampled: bool,
    pub left: ChannelDisplay,
    pub right: ChannelDisplay,
}

/// One channel's levels as shown, in dBFS.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelDisplay {
    pub rms: Level,
    pub peak: Level,
    pub peak_hold: Level,
}

/// A level in tenths of a dB, or silence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Silent,
    Tenths(i32),
}

impl Level {
    /// Rounds `db` to 0.1 dB; anything below `floor` (or not a number) is silence.
    pub fn from_db(db: f64, floor: f64) -> Level {
        if db.is_nan() || db < floor {
            Level::Silent
        } else {
            Level::Tenths((db * 10.0).round() as i32)
        }
    }

    /// The level in dB, or `None` for silence.
    pub fn db(self) -> Option<f64> {
        match self {
            Level::Silent => None,
            Level::Tenths(tenths) => Some(f64::from(tenths) / 10.0),
        }
    }
}

/// LUFS below the absolute gate (ITU-R BS.1770) show as silence.
pub const LUFS_FLOOR: f64 = -70.0;
/// Sample levels below this show as silence.
pub const LEVEL_FLOOR_DB: f64 = -90.0;

impl LoudnessDisplay {
    pub(crate) fn new(sample_rate: u32, readings: &LoudnessReadings) -> LoudnessDisplay {
        let lufs = |value| Level::from_db(value, LUFS_FLOOR);
        LoudnessDisplay {
            sample_rate,
            momentary: lufs(readings.momentary),
            short_term: lufs(readings.short_term),
            integrated: lufs(readings.integrated),
            range: Level::from_db(readings.range, 0.0),
            true_peak_max: Level::from_db(readings.true_peak_max, LEVEL_FLOOR_DB),
            true_peak_oversampled: readings.true_peak_oversampled,
            left: ChannelDisplay::new(&readings.left),
            right: ChannelDisplay::new(&readings.right),
        }
    }
}

impl ChannelDisplay {
    fn new(levels: &ChannelLevels) -> ChannelDisplay {
        let level = |value| Level::from_db(value, LEVEL_FLOOR_DB);
        ChannelDisplay {
            rms: level(levels.rms),
            peak: level(levels.peak),
            peak_hold: level(levels.peak_hold),
        }
    }
}
