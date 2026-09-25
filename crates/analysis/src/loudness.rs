//! The Loudness Meter's analyser.
//!
//! LUFS (momentary, short-term, integrated), LRA and true peak come from the
//! `ebur128` crate (ITU-R BS.1770-5, EBU Tech 3341/3342). Sample peak and RMS
//! are computed here, per ADR 0005.

use std::cell::Cell;
use std::collections::VecDeque;
use std::time::Duration;

use ebur128::{EbuR128, Mode};

/// Decibels below which a level reads as silence.
pub const FLOOR_DB: f64 = f64::NEG_INFINITY;

/// AES17 reference: a full-scale sine reads 0 dB, i.e. plain RMS + 20·log10(√2).
const AES17_OFFSET_DB: f64 = 3.010_299_956_639_812;

/// The loudness history's length in 100 ms steps: two minutes.
pub const HISTORY_STEPS: usize = 1_200;
/// How far back the peak in PSR looks, in 100 ms steps: the short-term window (3 s).
const PSR_STEPS: usize = 30;

/// Sample rate at and above which `ebur128` doesn't oversample true peak.
const NO_OVERSAMPLING_RATE: u32 = 192_000;

/// How RMS is scaled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum RmsMode {
    /// A full-scale sine reads 0 dB (+3.01 dB over plain RMS). The default.
    #[default]
    Aes17,
    /// Plain `20·log10(√mean(x²))`: a full-scale sine reads −3.01 dB.
    Plain,
}

/// How long the peak-hold value stays before it follows the signal down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PeakHold {
    For(#[serde(with = "crate::seconds")] Duration),
    Infinite,
}

/// The Loudness Meter's advanced settings that affect analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LoudnessSettings {
    /// Length of the sliding rectangular RMS (and sample peak) window. Default 300 ms.
    #[serde(with = "crate::seconds")]
    pub rms_window: Duration,
    pub rms_mode: RmsMode,
    /// Default 2 s.
    pub peak_hold: PeakHold,
}

impl Default for LoudnessSettings {
    fn default() -> Self {
        LoudnessSettings {
            rms_window: Duration::from_millis(300),
            rms_mode: RmsMode::Aes17,
            peak_hold: PeakHold::For(Duration::from_secs(2)),
        }
    }
}

/// One channel's level readings, in dBFS. Silence reads [`f64::NEG_INFINITY`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelLevels {
    /// RMS over the sliding window, scaled per [`RmsMode`].
    pub rms: f64,
    /// Sample peak `20·log10(max|x|)` over the sliding window.
    pub peak: f64,
    /// The highest recent peak, held per [`PeakHold`].
    pub peak_hold: f64,
    /// The highest sample peak since the last reset.
    pub peak_max: f64,
}

/// Everything the Loudness Meter shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoudnessReadings {
    /// LUFS over the last 400 ms. [`f64::NEG_INFINITY`] until there is audio.
    pub momentary: f64,
    /// LUFS over the last 3 s.
    pub short_term: f64,
    /// Gated LUFS since the last reset.
    pub integrated: f64,
    /// Loudness range in LU since the last reset.
    pub range: f64,
    /// The highest true peak (dBTP) of either channel since the last reset.
    pub true_peak_max: f64,
    /// Peak to loudness ratio: the true-peak maximum minus integrated
    /// loudness, in LU. [`f64::NEG_INFINITY`] until both exist.
    pub plr: f64,
    /// Peak to short-term loudness ratio: the true peak of the last 3 s minus
    /// short-term loudness, in LU. [`f64::NEG_INFINITY`] in silence.
    pub psr: f64,
    /// False at 192 kHz and above, where true peak isn't oversampled and is only
    /// a sample peak. The UI then labels it "Peak" instead of "TP".
    pub true_peak_oversampled: bool,
    pub left: ChannelLevels,
    pub right: ChannelLevels,
}

/// Sliding-window sum of squares and peak for one channel.
struct Window {
    /// Squared samples of the last `len` frames, as a ring.
    squares: Vec<f64>,
    next: usize,
    sum: f64,
    /// Peak of each chunk of the window, as a ring; the window's peak is their max.
    chunk_peaks: Vec<f32>,
    chunk_len: usize,
    chunk_fill: usize,
    chunk_next: usize,
    peak_hold: f32,
    hold_age: u64,
    peak_max: f32,
}

/// Chunks per window for the sliding peak: the peak window is exact to 1/64 of its length.
const PEAK_CHUNKS: usize = 64;

impl Window {
    fn new(len: usize) -> Window {
        let len = len.max(1);
        let chunk_len = len.div_ceil(PEAK_CHUNKS);
        Window {
            squares: vec![0.0; len],
            next: 0,
            sum: 0.0,
            chunk_peaks: vec![0.0; len.div_ceil(chunk_len)],
            chunk_len,
            chunk_fill: 0,
            chunk_next: 0,
            peak_hold: 0.0,
            hold_age: 0,
            peak_max: 0.0,
        }
    }

    fn push(&mut self, x: f32, hold_frames: Option<u64>) {
        let square = f64::from(x) * f64::from(x);
        self.sum += square - self.squares[self.next];
        self.squares[self.next] = square;
        self.next += 1;
        if self.next == self.squares.len() {
            self.next = 0;
            // Recompute once per window to stop rounding drift.
            self.sum = self.squares.iter().sum();
        }

        let magnitude = x.abs();
        if self.chunk_fill == 0 {
            self.chunk_peaks[self.chunk_next] = 0.0;
        }
        let chunk = &mut self.chunk_peaks[self.chunk_next];
        *chunk = chunk.max(magnitude);
        self.chunk_fill += 1;
        if self.chunk_fill == self.chunk_len {
            self.chunk_fill = 0;
            self.chunk_next = (self.chunk_next + 1) % self.chunk_peaks.len();
        }

        self.peak_max = self.peak_max.max(magnitude);
        if magnitude >= self.peak_hold {
            self.peak_hold = magnitude;
            self.hold_age = 0;
        } else {
            self.hold_age += 1;
            if hold_frames.is_some_and(|limit| self.hold_age > limit) {
                // Held long enough: drop to the current window peak.
                self.peak_hold = self.peak();
                self.hold_age = 0;
            }
        }
    }

    fn peak(&self) -> f32 {
        self.chunk_peaks.iter().fold(0.0, |m, &p| m.max(p))
    }

    fn levels(&self, mode: RmsMode) -> ChannelLevels {
        let mean = (self.sum / self.squares.len() as f64).max(0.0);
        let rms = 10.0 * mean.log10()
            + match mode {
                RmsMode::Aes17 => AES17_OFFSET_DB,
                RmsMode::Plain => 0.0,
            };
        ChannelLevels {
            rms: if mean > 0.0 { rms } else { FLOOR_DB },
            peak: db(self.peak()),
            peak_hold: db(self.peak_hold),
            peak_max: db(self.peak_max),
        }
    }

    fn reset_maxima(&mut self) {
        self.peak_max = 0.0;
        self.peak_hold = self.peak();
        self.hold_age = 0;
    }
}

fn db(linear: f32) -> f64 {
    if linear > 0.0 {
        20.0 * f64::from(linear).log10()
    } else {
        FLOOR_DB
    }
}

/// The LUFS figures, as last asked of `ebur128`.
#[derive(Clone, Copy)]
struct Lufs {
    momentary: f64,
    short_term: f64,
    integrated: f64,
    range: f64,
}

/// Stereo frames and a sample rate in, [`LoudnessReadings`] out.
pub struct LoudnessAnalyser {
    sample_rate: u32,
    settings: LoudnessSettings,
    ebu: EbuR128,
    channels: [Window; 2],
    hold_frames: Option<u64>,
    /// The LUFS figures and how many frames came since. `ebur128` sums its
    /// whole window (3 s for short-term) on every query and moves on in
    /// 100 ms steps, so they're asked again only after that much new audio.
    lufs: Cell<Option<(Lufs, usize)>>,
    /// Momentary and short-term LUFS every 100 ms of audio, oldest first.
    history: VecDeque<(f64, f64)>,
    /// Frames since the last history step, and the highest true peak (linear) in them.
    step: (usize, f64),
    /// The highest true peak (linear) of each of the last 3 s of steps.
    step_peaks: VecDeque<f64>,
}

/// New audio after which the LUFS figures are asked for again: 100 ms, the
/// step `ebur128`'s windows move in and the EBU display rate (10 Hz).
fn lufs_refresh(sample_rate: u32) -> usize {
    sample_rate as usize / 10
}

impl LoudnessAnalyser {
    /// # Panics
    /// If `sample_rate` is outside what `ebur128` accepts (it must be at least 16 Hz and fit the filters).
    pub fn new(sample_rate: u32, settings: LoudnessSettings) -> LoudnessAnalyser {
        let window = frames_in(sample_rate, settings.rms_window);
        LoudnessAnalyser {
            sample_rate,
            settings,
            ebu: EbuR128::new(
                2,
                sample_rate,
                Mode::M | Mode::S | Mode::I | Mode::LRA | Mode::TRUE_PEAK,
            )
            .expect("supported sample rate"),
            channels: [Window::new(window), Window::new(window)],
            hold_frames: match settings.peak_hold {
                PeakHold::For(time) => Some(frames_in(sample_rate, time) as u64),
                PeakHold::Infinite => None,
            },
            lufs: Cell::new(None),
            history: VecDeque::with_capacity(HISTORY_STEPS),
            step: (0, 0.0),
            step_peaks: VecDeque::with_capacity(PSR_STEPS),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Feeds interleaved stereo frames. A trailing half frame is ignored.
    pub fn process(&mut self, interleaved: &[f32]) {
        let whole = interleaved.len() / 2 * 2;
        let interleaved = &interleaved[..whole];
        self.ebu
            .add_frames_f32(interleaved)
            .expect("two-channel frames");
        let [left, right] = &mut self.channels;
        for frame in interleaved.chunks_exact(2) {
            left.push(frame[0], self.hold_frames);
            right.push(frame[1], self.hold_frames);
        }
        if let Some((lufs, since)) = self.lufs.get() {
            self.lufs.set(Some((lufs, since + whole / 2)));
        }
        // Every 100 ms of audio: a history point, and the step's true peak for PSR.
        let block_peak = (0..2)
            .filter_map(|channel| self.ebu.prev_true_peak(channel).ok())
            .fold(0.0f64, f64::max);
        self.step.0 += whole / 2;
        self.step.1 = self.step.1.max(block_peak);
        let refresh = lufs_refresh(self.sample_rate);
        if self.step.0 >= refresh {
            let value = |value: Result<f64, ebur128::Error>| value.unwrap_or(FLOOR_DB);
            if self.history.len() == HISTORY_STEPS {
                self.history.pop_front();
            }
            self.history.push_back((
                value(self.ebu.loudness_momentary()),
                value(self.ebu.loudness_shortterm()),
            ));
            if self.step_peaks.len() == PSR_STEPS {
                self.step_peaks.pop_front();
            }
            self.step_peaks.push_back(self.step.1);
            self.step = (self.step.0 - refresh, 0.0);
        }
    }

    /// Momentary and short-term LUFS every 100 ms of audio, oldest first, for
    /// the last [`HISTORY_STEPS`] steps. Silence reads [`f64::NEG_INFINITY`].
    pub fn history(&self) -> impl ExactSizeIterator<Item = (f64, f64)> + '_ {
        self.history.iter().copied()
    }

    /// The LUFS figures: asked of `ebur128` again after 100 ms of new audio.
    fn lufs(&self) -> Lufs {
        if let Some((lufs, since)) = self.lufs.get()
            && since < lufs_refresh(self.sample_rate)
        {
            return lufs;
        }
        let value = |value: Result<f64, ebur128::Error>| value.unwrap_or(FLOOR_DB);
        let lufs = Lufs {
            momentary: value(self.ebu.loudness_momentary()),
            short_term: value(self.ebu.loudness_shortterm()),
            integrated: value(self.ebu.loudness_global()),
            range: self.ebu.loudness_range().unwrap_or(0.0),
        };
        self.lufs.set(Some((lufs, 0)));
        lufs
    }

    pub fn readings(&self) -> LoudnessReadings {
        let lufs = self.lufs();
        let true_peak = (0..2)
            .filter_map(|channel| self.ebu.true_peak(channel).ok())
            .fold(0.0f64, f64::max);
        let db = |linear: f64| {
            if linear > 0.0 {
                20.0 * linear.log10()
            } else {
                FLOOR_DB
            }
        };
        let true_peak_max = db(true_peak);
        let recent_peak = db(self.step_peaks.iter().copied().fold(0.0, f64::max));
        let ratio = |peak: f64, loudness: f64| {
            if peak.is_finite() && loudness.is_finite() {
                peak - loudness
            } else {
                FLOOR_DB
            }
        };
        LoudnessReadings {
            momentary: lufs.momentary,
            short_term: lufs.short_term,
            integrated: lufs.integrated,
            range: lufs.range,
            true_peak_max,
            plr: ratio(true_peak_max, lufs.integrated),
            psr: ratio(recent_peak, lufs.short_term),
            true_peak_oversampled: self.sample_rate < NO_OVERSAMPLING_RATE,
            left: self.channels[0].levels(self.settings.rms_mode),
            right: self.channels[1].levels(self.settings.rms_mode),
        }
    }

    /// Clears integrated LUFS, LRA and the maxima (true peak, sample peak and hold),
    /// as a click on the Loudness Meter does. RMS keeps its window; momentary and
    /// short-term LUFS start over too (`ebur128` resets them together) and refill within 3 s.
    pub fn reset(&mut self) {
        self.ebu.reset();
        self.lufs.set(None);
        self.history.clear();
        self.step = (0, 0.0);
        self.step_peaks.clear();
        for channel in &mut self.channels {
            channel.reset_maxima();
        }
    }

    /// Restarts the analyser for a new sample rate (for example, the output device changed).
    /// Everything starts over, including integrated LUFS and peak hold.
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        *self = LoudnessAnalyser::new(sample_rate, self.settings);
    }

    /// Changes the settings. A different RMS window or hold time starts the levels over.
    pub fn set_settings(&mut self, settings: LoudnessSettings) {
        if settings.rms_window != self.settings.rms_window
            || settings.peak_hold != self.settings.peak_hold
        {
            let window = frames_in(self.sample_rate, settings.rms_window);
            self.channels = [Window::new(window), Window::new(window)];
            self.hold_frames = match settings.peak_hold {
                PeakHold::For(time) => Some(frames_in(self.sample_rate, time) as u64),
                PeakHold::Infinite => None,
            };
        }
        self.settings = settings;
    }
}

fn frames_in(sample_rate: u32, time: Duration) -> usize {
    (f64::from(sample_rate) * time.as_secs_f64()).round() as usize
}
