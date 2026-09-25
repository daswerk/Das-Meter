//! The Spectrum's analyser.
//!
//! Per ADR 0005 it is sine-calibrated: a 0 dBFS sine exactly on an FFT bin
//! reads 0 dB, whatever the window (coherent-gain corrected). The slope tilts
//! the display around 1 kHz. Output is a log-frequency draw buffer (a line or
//! fractional-octave bars) with a peak-hold curve, per Channel View trace.

use std::sync::Arc;
use std::time::Duration;

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

use crate::ChannelView;
use crate::loudness::PeakHold;

/// The level that stands for silence, in dB. Finite, so smoothing never meets infinities.
pub const SPECTRUM_FLOOR_DB: f32 = -200.0;

/// Frequency the slope pivots around.
const SLOPE_PIVOT_HZ: f32 = 1_000.0;

/// Allowed FFT sizes.
pub const MIN_FFT_SIZE: usize = 1_024;
pub const MAX_FFT_SIZE: usize = 16_384;

/// The window applied before the FFT.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum WindowFunction {
    /// The default: good all-round resolution and leakage.
    #[default]
    Hann,
    /// Lower leakage, wider peaks.
    BlackmanHarris,
    /// No window: sharpest peaks, most leakage.
    Rectangular,
}

impl WindowFunction {
    fn coefficients(self, size: usize) -> Vec<f32> {
        use std::f64::consts::TAU;
        let n = size as f64;
        (0..size)
            .map(|i| {
                let x = TAU * i as f64 / n; // periodic form: exact on-bin calibration
                let w = match self {
                    WindowFunction::Hann => 0.5 - 0.5 * x.cos(),
                    WindowFunction::BlackmanHarris => {
                        0.35875 - 0.48829 * x.cos() + 0.14128 * (2.0 * x).cos()
                            - 0.01168 * (3.0 * x).cos()
                    }
                    WindowFunction::Rectangular => 1.0,
                };
                w as f32
            })
            .collect()
    }
}

/// How the Spectrum is drawn, which decides the draw buffer's points.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SpectrumStyle {
    /// A line (with soft fill) through this many log-spaced points.
    Line { points: usize },
    /// Fractional-octave bars: `bands_per_octave` is 3, 6 or 12.
    Bars { bands_per_octave: u32 },
}

/// The Spectrum's settings that affect analysis.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct SpectrumSettings {
    pub channel_view: ChannelView,
    /// dB per octave, tilting around 1 kHz: 0, 3, 4.5 (default) or 6.
    pub slope: f32,
    /// A power of two from 1024 to 16384. Default 8192.
    pub fft_size: usize,
    pub window: WindowFunction,
    /// Shown frequency range in Hz. Default 20 Hz to 20 kHz; the top is capped at Nyquist.
    pub frequency_range: (f32, f32),
    /// Shown level range in dB, for the renderer. Default −90 to 0.
    pub db_range: (f32, f32),
    /// How fast a rising level is followed. Zero follows instantly.
    #[serde(with = "crate::seconds")]
    pub attack: Duration,
    /// How fast a falling level is followed.
    #[serde(with = "crate::seconds")]
    pub release: Duration,
    /// Frequency smoothing width in octaves (0 for none, up to 1/3).
    pub smoothing_octaves: f32,
    pub peak_hold: PeakHold,
    pub style: SpectrumStyle,
}

impl Default for SpectrumSettings {
    fn default() -> Self {
        SpectrumSettings {
            channel_view: ChannelView::Mono,
            slope: 4.5,
            fft_size: 8_192,
            window: WindowFunction::Hann,
            frequency_range: (20.0, 20_000.0),
            db_range: (-90.0, 0.0),
            attack: Duration::ZERO,
            release: Duration::from_millis(300),
            smoothing_octaves: 0.0,
            peak_hold: PeakHold::For(Duration::from_secs(2)),
            style: SpectrumStyle::Line { points: 512 },
        }
    }
}

/// One trace of the draw buffer: a level per point, in dB.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SpectrumTrace {
    pub levels: Vec<f32>,
    pub peak_hold: Vec<f32>,
}

/// What the renderer draws.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Spectrum {
    /// Frequency of each point in Hz: log-spaced for a line, band centres for bars.
    pub frequencies: Vec<f32>,
    /// One trace per Channel View trace (one for Mono, two for L+R and M+S).
    pub traces: Vec<SpectrumTrace>,
    /// The settings' dB range, passed through for the renderer.
    pub db_range: (f32, f32),
}

/// Which FFT bins feed one point of the draw buffer.
#[derive(Clone, Copy, Debug)]
struct PointBins {
    /// Fractional bin at the point's frequency, for interpolation when no bin falls inside.
    centre: f32,
    first: usize,
    /// Exclusive. `first == end` means no bin falls inside.
    end: usize,
    /// Average power over the bins instead of taking the peak (frequency smoothing).
    average: bool,
}

struct Ballistics {
    peak_age: Vec<f64>,
}

/// Stereo frames in, [`Spectrum`] out.
pub struct SpectrumAnalyser {
    sample_rate: u32,
    settings: SpectrumSettings,
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    /// Converts |X| to sine amplitude: 2 / (N · coherent gain).
    scale: f32,
    /// The last `fft_size` samples of each trace, as rings.
    history: [Vec<f32>; 2],
    write: usize,
    /// Samples processed since the last update, for time-based smoothing.
    pending: u64,
    input: Vec<f32>,
    output: Vec<Complex32>,
    scratch: Vec<Complex32>,
    /// Calibrated, sloped level per bin, per trace.
    bins: [Vec<f32>; 2],
    /// Slope in dB per bin.
    tilt: Vec<f32>,
    points: Vec<PointBins>,
    spectrum: Spectrum,
    ballistics: [Ballistics; 2],
    fresh: bool,
}

impl SpectrumAnalyser {
    /// # Panics
    /// If `fft_size` isn't a power of two from 1024 to 16384.
    pub fn new(sample_rate: u32, settings: SpectrumSettings) -> SpectrumAnalyser {
        let size = settings.fft_size;
        assert!(
            size.is_power_of_two() && (MIN_FFT_SIZE..=MAX_FFT_SIZE).contains(&size),
            "FFT size {size} must be a power of two from {MIN_FFT_SIZE} to {MAX_FFT_SIZE}"
        );
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(size);
        let window = settings.window.coefficients(size);
        let coherent_gain = window.iter().map(|&w| f64::from(w)).sum::<f64>() / size as f64;
        let bin_count = size / 2 + 1;
        let bin_hz = sample_rate as f32 / size as f32;
        let tilt = (0..bin_count)
            .map(|k| match k {
                0 => 0.0,
                _ => settings.slope * (k as f32 * bin_hz / SLOPE_PIVOT_HZ).log2(),
            })
            .collect();
        let (frequencies, points) = layout(sample_rate, &settings);
        let traces = settings.channel_view.traces();
        let spectrum = Spectrum {
            traces: vec![
                SpectrumTrace {
                    levels: vec![SPECTRUM_FLOOR_DB; frequencies.len()],
                    peak_hold: vec![SPECTRUM_FLOOR_DB; frequencies.len()],
                };
                traces
            ],
            frequencies,
            db_range: settings.db_range,
        };
        let ballistics = || Ballistics {
            peak_age: vec![0.0; points.len()],
        };
        SpectrumAnalyser {
            sample_rate,
            settings,
            input: fft.make_input_vec(),
            output: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            window,
            scale: (2.0 / (size as f64 * coherent_gain)) as f32,
            history: [vec![0.0; size], vec![0.0; size]],
            write: 0,
            pending: 0,
            bins: [
                vec![SPECTRUM_FLOOR_DB; bin_count],
                vec![SPECTRUM_FLOOR_DB; bin_count],
            ],
            tilt,
            ballistics: [ballistics(), ballistics()],
            points,
            spectrum,
            fresh: true,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn settings(&self) -> &SpectrumSettings {
        &self.settings
    }

    /// Restarts for a new sample rate.
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        *self = SpectrumAnalyser::new(sample_rate, self.settings);
    }

    /// Applies new settings. The analyser restarts, so history and peak hold start over.
    pub fn set_settings(&mut self, settings: SpectrumSettings) {
        *self = SpectrumAnalyser::new(self.sample_rate, settings);
    }

    /// Feeds interleaved stereo frames. A trailing half frame is ignored.
    pub fn process(&mut self, interleaved: &[f32]) {
        let view = self.settings.channel_view;
        let size = self.history[0].len();
        for frame in interleaved.chunks_exact(2) {
            let [a, b] = view.split(frame[0], frame[1]);
            self.history[0][self.write] = a;
            self.history[1][self.write] = b;
            self.write = (self.write + 1) % size;
        }
        self.pending += (interleaved.len() / 2) as u64;
    }

    /// Analyses the newest `fft_size` samples and returns the draw buffer.
    /// Call it once per drawn frame; smoothing follows the audio time since the last call.
    pub fn update(&mut self) -> &Spectrum {
        let elapsed = self.pending as f64 / f64::from(self.sample_rate);
        self.pending = 0;
        for trace in 0..self.spectrum.traces.len() {
            self.analyse_trace(trace);
            self.draw_trace(trace, elapsed);
        }
        self.fresh = false;
        &self.spectrum
    }

    /// The last draw buffer, without analysing again.
    pub fn spectrum(&self) -> &Spectrum {
        &self.spectrum
    }

    /// Calibrated, sloped level of every FFT bin of one trace, as of the last [`update`](Self::update).
    pub fn bin_levels(&self, trace: usize) -> &[f32] {
        &self.bins[trace]
    }

    /// Centre frequency of an FFT bin in Hz.
    pub fn bin_frequency(&self, bin: usize) -> f32 {
        bin as f32 * self.sample_rate as f32 / self.settings.fft_size as f32
    }

    /// Frequency of the loudest bin of one trace within the shown range.
    pub fn peak_frequency(&self, trace: usize) -> f32 {
        let (low, high) = self.shown_range();
        let bins = &self.bins[trace];
        let best = (1..bins.len())
            .filter(|&k| (low..=high).contains(&self.bin_frequency(k)))
            .max_by(|&a, &b| bins[a].total_cmp(&bins[b]))
            .unwrap_or(0);
        self.bin_frequency(best)
    }

    /// The loudest peak of any trace within the shown range, as of the last
    /// [`update`](Self::update): its frequency, refined between bins by a
    /// parabola through the loudest bin and its neighbours (in dB), and its
    /// level. `None` when nothing rises above `floor` dB.
    pub fn peak(&self, floor: f32) -> Option<(f32, f32)> {
        let (low, high) = self.shown_range();
        let bin_hz = self.bin_frequency(1);
        let mut best: Option<(f32, f32)> = None;
        for bins in &self.bins[..self.spectrum.traces.len()] {
            let Some(k) = (1..bins.len().saturating_sub(1))
                .filter(|&k| (low..=high).contains(&self.bin_frequency(k)))
                .max_by(|&a, &b| bins[a].total_cmp(&bins[b]))
            else {
                continue;
            };
            let (left, centre, right) = (bins[k - 1], bins[k], bins[k + 1]);
            if centre <= floor {
                continue;
            }
            let curve = left - 2.0 * centre + right;
            let offset = if curve < 0.0 {
                (0.5 * (left - right) / curve).clamp(-0.5, 0.5)
            } else {
                0.0
            };
            let level = centre - 0.25 * (left - right) * offset;
            let frequency = (k as f32 + offset) * bin_hz;
            if best.is_none_or(|(_, db)| level > db) {
                best = Some((frequency, level));
            }
        }
        best
    }

    fn shown_range(&self) -> (f32, f32) {
        shown_range(self.sample_rate, &self.settings)
    }

    fn analyse_trace(&mut self, trace: usize) {
        let size = self.history[trace].len();
        let history = &self.history[trace];
        for (i, (sample, w)) in self.input.iter_mut().zip(&self.window).enumerate() {
            *sample = history[(self.write + i) % size] * w;
        }
        self.fft
            .process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch)
            .expect("buffers sized by the planner");
        for ((level, x), tilt) in self.bins[trace]
            .iter_mut()
            .zip(&self.output)
            .zip(&self.tilt)
        {
            let amplitude = x.norm() * self.scale;
            *level = if amplitude > 0.0 {
                (20.0 * amplitude.log10() + tilt).max(SPECTRUM_FLOOR_DB)
            } else {
                SPECTRUM_FLOOR_DB
            };
        }
    }

    fn draw_trace(&mut self, trace: usize, elapsed: f64) {
        let bins = &self.bins[trace];
        let out = &mut self.spectrum.traces[trace];
        let attack = smoothing_factor(self.settings.attack, elapsed);
        let release = smoothing_factor(self.settings.release, elapsed);
        let hold = match self.settings.peak_hold {
            PeakHold::For(time) => Some(time.as_secs_f64()),
            PeakHold::Infinite => None,
        };
        let ages = &mut self.ballistics[trace].peak_age;
        for (i, point) in self.points.iter().enumerate() {
            let now = point_level(bins, point);
            let level = &mut out.levels[i];
            *level = if self.fresh {
                now
            } else {
                let factor = if now > *level { attack } else { release };
                *level + (now - *level) * factor
            };
            let peak = &mut out.peak_hold[i];
            if self.fresh || *level >= *peak {
                *peak = *level;
                ages[i] = 0.0;
            } else {
                ages[i] += elapsed;
                if hold.is_some_and(|limit| ages[i] > limit) {
                    *peak = *level;
                    ages[i] = 0.0;
                }
            }
        }
    }
}

/// Share of the gap to close this update for a one-pole smoother with time constant `time`.
fn smoothing_factor(time: Duration, elapsed: f64) -> f32 {
    let tau = time.as_secs_f64();
    if tau <= 0.0 {
        1.0
    } else {
        (1.0 - (-elapsed / tau).exp()) as f32
    }
}

fn point_level(bins: &[f32], point: &PointBins) -> f32 {
    if point.first >= point.end {
        // Narrower than a bin: interpolate between the bins around the point.
        let below = (point.centre.floor() as usize).min(bins.len() - 1);
        let above = (below + 1).min(bins.len() - 1);
        let t = point.centre - below as f32;
        return bins[below] + (bins[above] - bins[below]) * t;
    }
    let span = &bins[point.first..point.end];
    if point.average {
        let power = span.iter().map(|&db| 10f32.powf(db / 10.0)).sum::<f32>() / span.len() as f32;
        (10.0 * power.log10()).max(SPECTRUM_FLOOR_DB)
    } else {
        span.iter().copied().fold(SPECTRUM_FLOOR_DB, f32::max)
    }
}

fn shown_range(sample_rate: u32, settings: &SpectrumSettings) -> (f32, f32) {
    let nyquist = sample_rate as f32 / 2.0;
    let (low, high) = settings.frequency_range;
    let low = low.max(1.0);
    (low.min(nyquist), high.min(nyquist).max(low))
}

/// The draw buffer's frequencies and which bins feed each point.
fn layout(sample_rate: u32, settings: &SpectrumSettings) -> (Vec<f32>, Vec<PointBins>) {
    let (low, high) = shown_range(sample_rate, settings);
    let bin_hz = sample_rate as f32 / settings.fft_size as f32;
    let bin_count = settings.fft_size / 2 + 1;

    // Each point covers [edge_low, edge_high) around its frequency.
    let spans: Vec<(f32, f32, f32)> = match settings.style {
        SpectrumStyle::Line { points } => {
            let points = points.max(2);
            let ratio = (high / low).powf(1.0 / (points - 1) as f32);
            let half = ratio.sqrt();
            (0..points)
                .map(|i| {
                    let f = low * ratio.powi(i as i32);
                    (f, f / half, f * half)
                })
                .collect()
        }
        SpectrumStyle::Bars { bands_per_octave } => {
            let n = bands_per_octave.max(1) as f32;
            let half = 2f32.powf(0.5 / n);
            let first = (n * (low / SLOPE_PIVOT_HZ).log2()).ceil() as i32;
            let last = (n * (high / SLOPE_PIVOT_HZ).log2()).floor() as i32;
            (first..=last)
                .map(|k| {
                    let f = SLOPE_PIVOT_HZ * 2f32.powf(k as f32 / n);
                    (f, f / half, f * half)
                })
                .collect()
        }
    };

    let smoothing = settings.smoothing_octaves.clamp(0.0, 1.0 / 3.0);
    let points = spans
        .iter()
        .map(|&(f, mut edge_low, mut edge_high)| {
            let average = smoothing > 0.0;
            if average {
                let half = 2f32.powf(smoothing / 2.0);
                edge_low = edge_low.min(f / half);
                edge_high = edge_high.max(f * half);
            }
            let first = ((edge_low / bin_hz).ceil() as usize).clamp(1, bin_count);
            let end = ((edge_high / bin_hz).ceil() as usize).clamp(first, bin_count);
            PointBins {
                centre: (f / bin_hz).min((bin_count - 1) as f32),
                first,
                end,
                average,
            }
        })
        .collect();
    (spans.iter().map(|&(f, _, _)| f).collect(), points)
}

/// A frequency as the nearest equal-tempered note (A4 = 440 Hz), for the cursor readout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    /// "C", "C#", … "B".
    pub name: &'static str,
    /// Scientific pitch notation: middle C is C4.
    pub octave: i32,
    /// How far the frequency is from the note, −50 to +50.
    pub cents: f32,
}

impl std::fmt::Display for Note {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", self.name, self.octave)
    }
}

/// The note nearest to `frequency` Hz, or `None` for a non-positive frequency.
pub fn note_name(frequency: f32) -> Option<Note> {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    if frequency.is_nan() || frequency <= 0.0 {
        return None;
    }
    // MIDI note number: A4 = 69.
    let midi = 69.0 + 12.0 * (f64::from(frequency) / 440.0).log2();
    let nearest = midi.round();
    let index = nearest as i64;
    Some(Note {
        name: NAMES[index.rem_euclid(12) as usize],
        octave: (index.div_euclid(12) - 1) as i32,
        cents: ((midi - nearest) * 100.0) as f32,
    })
}
