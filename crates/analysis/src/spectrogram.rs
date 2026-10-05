//! The Spectrogram's analyser: the Spectrum of the mono sum over time.
//!
//! Every `span / COLUMNS` of audio it analyses the newest `fft_size` samples
//! with the Spectrum's analyser (same calibration, window and slope, no
//! smoothing over time) and keeps the levels at `ROWS` log-spaced frequencies
//! as one column. The newest [`COLUMNS`] columns make the picture.

use std::collections::VecDeque;
use std::time::Duration;

use crate::ChannelView;
use crate::loudness::PeakHold;
use crate::spectrum::{SpectrumAnalyser, SpectrumSettings, SpectrumStyle, WindowFunction};

/// Columns across the span.
pub const COLUMNS: usize = 256;
/// Log-spaced frequencies per column.
pub const ROWS: usize = 160;
/// The FFT sizes offered.
pub const FFT_SIZES: [usize; 4] = [1_024, 2_048, 4_096, 8_192];

/// The Spectrogram's settings that affect analysis.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SpectrogramSettings {
    /// One of [`FFT_SIZES`]. Default 4096.
    pub fft_size: usize,
    pub window: WindowFunction,
    /// dB per octave, tilting around 1 kHz, as the Spectrum. Default 4.5.
    pub slope: f32,
    /// Shown frequency range in Hz. Default 20 Hz to 20 kHz; the top is capped at Nyquist.
    pub frequency_range: (f32, f32),
    /// The levels the colours span, in dB. Default −90 to 0.
    pub db_range: (f32, f32),
    /// How much time the picture spans. Default 10 s.
    #[serde(with = "crate::seconds")]
    pub span: Duration,
}

impl Default for SpectrogramSettings {
    fn default() -> Self {
        SpectrogramSettings {
            fft_size: 4_096,
            window: WindowFunction::Hann,
            slope: 4.5,
            frequency_range: (20.0, 20_000.0),
            db_range: (-90.0, 0.0),
            span: Duration::from_secs(10),
        }
    }
}

/// See the module docs.
pub struct SpectrogramAnalyser {
    spectrum: SpectrumAnalyser,
    settings: SpectrogramSettings,
    /// Frames between columns.
    hop: usize,
    /// Frames until the next column.
    until_column: usize,
    /// Oldest first; each level is 0 (at or below the range's floor) to 255
    /// (at or above its top).
    columns: VecDeque<Vec<u8>>,
    completed: u64,
}

impl SpectrogramAnalyser {
    pub fn new(sample_rate: u32, settings: SpectrogramSettings) -> SpectrogramAnalyser {
        let hop = hop(sample_rate, &settings);
        SpectrogramAnalyser {
            spectrum: SpectrumAnalyser::new(sample_rate, spectrum_settings(sample_rate, &settings)),
            settings,
            hop,
            until_column: hop,
            columns: VecDeque::with_capacity(COLUMNS),
            completed: 0,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.spectrum.sample_rate()
    }

    pub fn settings(&self) -> &SpectrogramSettings {
        &self.settings
    }

    /// New settings start the picture over.
    pub fn set_settings(&mut self, settings: SpectrogramSettings) {
        if settings != self.settings {
            *self = SpectrogramAnalyser::new(self.sample_rate(), settings);
        }
    }

    /// Feeds interleaved stereo frames; a column is added every hop.
    pub fn process(&mut self, interleaved: &[f32]) {
        let mut frames = interleaved;
        while !frames.is_empty() {
            let take = self.until_column.min(frames.len() / 2);
            let (now, rest) = frames.split_at(take * 2);
            self.spectrum.process(now);
            frames = rest;
            self.until_column -= take;
            if self.until_column == 0 {
                self.until_column = self.hop;
                self.add_column();
            }
            if take == 0 {
                break;
            }
        }
    }

    fn add_column(&mut self) {
        let (floor, top) = self.settings.db_range;
        let scale = 255.0 / (top - floor).max(1.0);
        let column = self.spectrum.update().traces[0]
            .levels
            .iter()
            .map(|&db| ((db - floor) * scale).round().clamp(0.0, 255.0) as u8)
            .collect();
        if self.columns.len() == COLUMNS {
            self.columns.pop_front();
        }
        self.columns.push_back(column);
        self.completed += 1;
    }

    /// The columns so far, oldest first: at most [`COLUMNS`], each [`ROWS`]
    /// levels from the lowest frequency up.
    pub fn columns(&self) -> impl Iterator<Item = &Vec<u8>> {
        self.columns.iter()
    }

    /// Columns added since the start.
    pub fn completed(&self) -> u64 {
        self.completed
    }

    /// The frequencies of the rows, lowest first.
    pub fn frequencies(&self) -> &[f32] {
        &self.spectrum.spectrum().frequencies
    }
}

fn hop(sample_rate: u32, settings: &SpectrogramSettings) -> usize {
    let frames = settings.span.as_secs_f64() * f64::from(sample_rate) / COLUMNS as f64;
    (frames.round() as usize).max(1)
}

fn spectrum_settings(sample_rate: u32, settings: &SpectrogramSettings) -> SpectrumSettings {
    let nyquist = sample_rate as f32 / 2.0;
    let (low, high) = settings.frequency_range;
    SpectrumSettings {
        channel_view: ChannelView::Mono,
        slope: settings.slope,
        fft_size: settings.fft_size,
        window: settings.window,
        frequency_range: (low, high.min(nyquist)),
        db_range: settings.db_range,
        attack: Duration::ZERO,
        release: Duration::ZERO,
        smoothing_octaves: 0.0,
        peak_hold: PeakHold::For(Duration::ZERO),
        style: SpectrumStyle::Line { points: ROWS },
    }
}
