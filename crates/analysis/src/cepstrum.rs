//! The Cepstrum's analyser: the real cepstrum of the mono sum, and the pitch
//! it points to.
//!
//! The cepstrum is the inverse FFT of the log magnitude spectrum. A harmonic
//! sound's evenly spaced partials make a peak at the quefrency (a time) of
//! its period, so the loudest peak between the periods of the highest and the
//! lowest pitch asked for gives the fundamental: `f0 = sample rate / quefrency`.
//! Echoes show as peaks at their delay too.
//!
//! Each [`update`](CepstrumAnalyser::update) takes the last `fft_size` samples
//! with a Hann window, smooths the cepstrum over time, and resamples it for
//! drawing, from the shortest period (highest pitch) to the longest.

use std::sync::Arc;
use std::time::Duration;

use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

/// FFT sizes the Cepstrum offers.
pub const FFT_SIZES: [usize; 3] = [2_048, 4_096, 8_192];
/// The widest pitch range, in Hz.
pub const PITCH_LIMITS: (f32, f32) = (30.0, 2_000.0);
/// Points in the draw buffer.
pub const POINTS: usize = 256;
/// The quietest mono RMS (linear) with a pitch: about −80 dBFS.
const SILENCE_RMS: f32 = 1e-4;
/// How far the cepstral peak must stand above the rest of the range, in
/// standard deviations, to count as a pitch.
const VOICED_Z: f32 = 5.0;
/// Below this, the draw buffer isn't scaled up: noise stays low.
const MIN_SCALE: f32 = 0.02;

/// The Cepstrum's settings that affect analysis.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct CepstrumSettings {
    /// 2048, 4096 or 8192 samples. Default 4096 (85 ms at 48 kHz): long
    /// enough for several periods of the lowest pitch.
    pub fft_size: usize,
    /// The pitches looked for, in Hz. Default 50 to 1000.
    pub pitch_range: (f32, f32),
    /// Time constant of the smoothing over time. Default 100 ms.
    #[serde(with = "crate::seconds")]
    pub smoothing: Duration,
}

impl Default for CepstrumSettings {
    fn default() -> Self {
        CepstrumSettings {
            fft_size: 4_096,
            pitch_range: (50.0, 1_000.0),
            smoothing: Duration::from_millis(100),
        }
    }
}

/// A detected pitch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pitch {
    /// The fundamental in Hz.
    pub frequency: f32,
    /// How clearly the peak stands out, in standard deviations of the range.
    pub strength: f32,
}

/// What the renderer draws.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Cepstrum {
    /// The cepstrum from the shortest period to the longest, scaled so the
    /// highest peak is 1 (negative values are left at 0).
    pub values: Vec<f32>,
    /// The periods at the left and right edges, in seconds.
    pub quefrency_range: (f32, f32),
    pub pitch: Option<Pitch>,
}

/// Stereo frames in, [`Cepstrum`] out.
pub struct CepstrumAnalyser {
    sample_rate: u32,
    settings: CepstrumSettings,
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    window: Vec<f32>,
    /// The last `fft_size` mono samples, as a ring.
    history: Vec<f32>,
    write: usize,
    /// Samples since the last update, for the smoothing.
    pending: u64,
    input: Vec<f32>,
    spectrum: Vec<Complex32>,
    output: Vec<f32>,
    /// The smoothed cepstrum, per quefrency sample.
    smoothed: Vec<f32>,
    cepstrum: Cepstrum,
}

impl CepstrumAnalyser {
    /// # Panics
    /// If `fft_size` isn't one of [`FFT_SIZES`].
    pub fn new(sample_rate: u32, settings: CepstrumSettings) -> CepstrumAnalyser {
        let size = settings.fft_size;
        assert!(FFT_SIZES.contains(&size), "FFT size {size}");
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(size);
        let inverse = planner.plan_fft_inverse(size);
        let window = (0..size)
            .map(|i| {
                let phase = std::f32::consts::TAU * i as f32 / size as f32;
                0.5 - 0.5 * phase.cos()
            })
            .collect();
        let mut analyser = CepstrumAnalyser {
            sample_rate,
            settings,
            spectrum: forward.make_output_vec(),
            input: forward.make_input_vec(),
            output: inverse.make_output_vec(),
            forward,
            inverse,
            window,
            history: vec![0.0; size],
            write: 0,
            pending: 0,
            smoothed: vec![0.0; size / 2],
            cepstrum: Cepstrum::default(),
        };
        analyser.cepstrum.quefrency_range = analyser.quefrency_range();
        analyser.cepstrum.values = vec![0.0; POINTS];
        analyser
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn settings(&self) -> &CepstrumSettings {
        &self.settings
    }

    /// Restarts for a new sample rate.
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        *self = CepstrumAnalyser::new(sample_rate, self.settings);
    }

    /// Applies new settings; a new FFT size starts over.
    pub fn set_settings(&mut self, settings: CepstrumSettings) {
        if settings.fft_size != self.settings.fft_size {
            *self = CepstrumAnalyser::new(self.sample_rate, settings);
        } else {
            self.settings = settings;
            self.cepstrum.quefrency_range = self.quefrency_range();
        }
    }

    /// Feeds interleaved stereo frames (their mono sum is analysed).
    pub fn process(&mut self, interleaved: &[f32]) {
        let size = self.history.len();
        for frame in interleaved.chunks_exact(2) {
            self.history[self.write] = 0.5 * (frame[0] + frame[1]);
            self.write = (self.write + 1) % size;
        }
        self.pending += (interleaved.len() / 2) as u64;
    }

    /// The quefrency samples between the shortest and the longest period asked for.
    fn samples(&self) -> (usize, usize) {
        let (low, high) = self.pitch_range();
        let rate = self.sample_rate as f32;
        let last = self.smoothed.len() - 2;
        let first = ((rate / high).floor() as usize).clamp(2, last - 1);
        let end = ((rate / low).ceil() as usize).clamp(first + 1, last);
        (first, end)
    }

    /// The pitch range, kept within [`PITCH_LIMITS`] and in order.
    fn pitch_range(&self) -> (f32, f32) {
        let (a, b) = self.settings.pitch_range;
        let low = a.min(b).clamp(PITCH_LIMITS.0, PITCH_LIMITS.1);
        let high = a.max(b).clamp(PITCH_LIMITS.0, PITCH_LIMITS.1);
        (low, high.max(low * 2.0))
    }

    fn quefrency_range(&self) -> (f32, f32) {
        let (first, end) = self.samples();
        let rate = self.sample_rate as f32;
        (first as f32 / rate, end as f32 / rate)
    }

    /// Analyses the last `fft_size` samples and returns what to draw.
    pub fn update(&mut self) -> &Cepstrum {
        let size = self.history.len();
        let elapsed = self.pending as f32 / self.sample_rate as f32;
        self.pending = 0;

        let mut power = 0.0f32;
        for (i, (sample, w)) in self.input.iter_mut().zip(&self.window).enumerate() {
            let x = self.history[(self.write + i) % size];
            power += x * x;
            *sample = x * w;
        }
        let silent = (power / size as f32).sqrt() < SILENCE_RMS;

        // Real cepstrum: inverse FFT of the log magnitude.
        self.forward
            .process(&mut self.input, &mut self.spectrum)
            .expect("buffers sized by the planner");
        for bin in &mut self.spectrum {
            *bin = Complex32::new((bin.norm() + 1e-9).ln(), 0.0);
        }
        self.inverse
            .process(&mut self.spectrum, &mut self.output)
            .expect("buffers sized by the planner");

        // Smooth over time; silence fades the curve out.
        let tau = self.settings.smoothing.as_secs_f32();
        let alpha = if tau > 0.0 {
            1.0 - (-elapsed / tau).exp()
        } else {
            1.0
        };
        let scale = 1.0 / size as f32;
        for (smoothed, &value) in self.smoothed.iter_mut().zip(&self.output) {
            let target = if silent { 0.0 } else { value * scale };
            *smoothed += alpha * (target - *smoothed);
        }

        let (first, end) = self.samples();
        self.cepstrum.quefrency_range = self.quefrency_range();
        self.cepstrum.pitch = if silent { None } else { self.pitch(first, end) };
        self.draw(first, end);
        &self.cepstrum
    }

    /// The highest peak in the range, refined with a parabola, if it stands out.
    fn pitch(&self, first: usize, end: usize) -> Option<Pitch> {
        let range = &self.smoothed[first..=end];
        let n = range.len() as f32;
        let mean = range.iter().sum::<f32>() / n;
        let deviation = (range.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / n).sqrt();
        let k = first
            + range
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map(|(i, _)| i)?;
        let strength = (self.smoothed[k] - mean) / deviation.max(f32::EPSILON);
        if strength < VOICED_Z {
            return None;
        }
        let (left, centre, right) = (self.smoothed[k - 1], self.smoothed[k], self.smoothed[k + 1]);
        let curve = left - 2.0 * centre + right;
        let offset = if curve < 0.0 {
            (0.5 * (left - right) / curve).clamp(-0.5, 0.5)
        } else {
            0.0
        };
        Some(Pitch {
            frequency: self.sample_rate as f32 / (k as f32 + offset),
            strength,
        })
    }

    /// Resamples the range into [`POINTS`], keeping each point's highest value.
    fn draw(&mut self, first: usize, end: usize) {
        let range = &self.smoothed[first..=end];
        let peak = range.iter().copied().fold(0.0f32, f32::max).max(MIN_SCALE);
        let span = range.len() as f32 / POINTS as f32;
        for (i, value) in self.cepstrum.values.iter_mut().enumerate() {
            let from = (i as f32 * span) as usize;
            let to = (((i + 1) as f32 * span).ceil() as usize).clamp(from + 1, range.len());
            let highest = range[from..to].iter().copied().fold(0.0f32, f32::max);
            *value = (highest / peak).clamp(0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_range_covers_the_periods_asked_for() {
        let analyser = CepstrumAnalyser::new(48_000, CepstrumSettings::default());
        let (short, long) = analyser.quefrency_range();
        assert!((short - 1.0 / 1_000.0).abs() < 0.0001, "{short}");
        assert!((long - 1.0 / 50.0).abs() < 0.0001, "{long}");
    }
}
