//! The Tonal Balance Meter's analysis (#105).
//!
//! Audio is analysed in **frames** half a second apart, each from a long FFT
//! (about 1.4 s), fine enough to tell bass notes apart. A frame holds the
//! power in each band of the [`tonal_grid`] (1/24 octave up to 300 Hz, coarser
//! above), a chromagram for the key, and the low end's stereo and dynamics
//! figures. Frames are summed, all of them or only the loud parts, into a
//! [`Summary`]: a curve normalised to its own loudness (shape only), five
//! region levels, the Sub and Bass fundamentals, the low end's mono-ness and
//! crest, and the key. The live Meter and a Target's songs use the same
//! frames, so what's compared is measured the same way.

use std::collections::VecDeque;
use std::f64::consts::TAU;
use std::sync::Arc;

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};

use crate::spectrum::{LongTerm, Note, note_name};

/// Seconds between frames.
pub const FRAME_SECONDS: f64 = 0.5;
/// The FFT spans about this long, rounded up to a power of two of samples.
const FFT_SECONDS: f64 = 1.36;
/// Below this the grid is 1/24 octave: every semitone gets two points.
pub const FINE_BELOW_HZ: f32 = 300.0;
/// Between [`FINE_BELOW_HZ`] and this the grid is 1/12 octave, above it 1/6.
const MEDIUM_BELOW_HZ: f32 = 2_000.0;
/// The grid's ends.
pub const GRID_RANGE: (f32, f32) = (20.0, 20_000.0);
/// A frame this far below the loudest short-term loudness of a song (in LU)
/// isn't one of its loud parts.
pub const LOUD_PARTS_LU: f32 = 4.0;
/// Frames quieter than this (K-weighted, about LUFS) count as silence.
const SILENCE_LUFS: f32 = -70.0;
/// Short-term loudness spans this many frames (3 s).
const SHORT_TERM_FRAMES: usize = 6;
/// Chroma is gathered between these frequencies.
const CHROMA_RANGE: (f32, f32) = (55.0, 5_000.0);

/// The five regions of the Broad view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    Sub,
    Bass,
    LowMid,
    Mid,
    High,
}

impl Region {
    pub const ALL: [Region; 5] = [
        Region::Sub,
        Region::Bass,
        Region::LowMid,
        Region::Mid,
        Region::High,
    ];

    /// Its frequency range in Hz.
    pub fn range(self) -> (f32, f32) {
        match self {
            Region::Sub => (20.0, 60.0),
            Region::Bass => (60.0, 120.0),
            Region::LowMid => (120.0, 300.0),
            Region::Mid => (300.0, 2_000.0),
            Region::High => (2_000.0, 20_000.0),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Region::Sub => "Sub",
            Region::Bass => "Bass",
            Region::LowMid => "Low-mid",
            Region::Mid => "Mid",
            Region::High => "High",
        }
    }
}

/// A musical key: its tonic as a pitch class (0 = C … 11 = B) and mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Key {
    pub tonic: u8,
    pub minor: bool,
}

impl Key {
    pub fn name(self) -> String {
        const NAMES: [&str; 12] = [
            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
        ];
        let mode = if self.minor { "minor" } else { "major" };
        format!("{} {mode}", NAMES[usize::from(self.tonic % 12)])
    }

    /// The fewest semitones (−5 to +6) from this key's tonic to `other`'s.
    pub fn semitones_to(self, other: Key) -> i32 {
        let up = (i32::from(other.tonic) - i32::from(self.tonic)).rem_euclid(12);
        if up > 6 { up - 12 } else { up }
    }
}

/// A key as detected, with how sure the detection is (0–1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyEstimate {
    pub key: Key,
    pub confidence: f32,
}

/// The loudest note found in a region.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fundamental {
    pub frequency: f32,
    pub note: Note,
}

/// One half-second frame of analysis.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// Power per band of the [`tonal_grid`] (mean square contributed).
    pub bands: Vec<f32>,
    /// Power per pitch class, C to B.
    pub chroma: [f32; 12],
    /// K-weighted mean square: the frame's loudness, about LUFS once in dB.
    pub loudness_power: f32,
    /// The low end (below the mono cutoff): the mean of L·R, L² and R² over
    /// the frame.
    pub low_lr: [f64; 3],
    /// The Sub and Bass bands' peak (squared) and mean square over the frame.
    pub crest: [[f32; 2]; 2],
}

impl Frame {
    /// The frame's loudness in LUFS (approximate: K-weighted in the frequency domain).
    pub fn loudness(&self) -> f32 {
        lufs(self.loudness_power)
    }
}

/// Frames summed: what the Meter compares and a Target's song keeps.
#[derive(Clone, Debug, PartialEq)]
pub struct Summary {
    /// Level per [`tonal_grid`] point in dB per octave, relative to the
    /// loudness (shape only).
    pub curve: Vec<f32>,
    /// Level per [`Region`] in dB, relative to the loudness.
    pub regions: [f32; 5],
    /// The loudness the curve was normalised by, about LUFS.
    pub loudness: f32,
    /// Correlation of Left and Right below the mono cutoff, −1 to +1;
    /// `None` when the low end was silent.
    pub low_end_mono: Option<f32>,
    /// The Sub's and Bass's crest factor (peak over RMS) in dB.
    pub low_end_crest: [Option<f32>; 2],
    pub sub: Option<Fundamental>,
    pub bass: Option<Fundamental>,
    pub key: Option<KeyEstimate>,
    /// Seconds of (non-silent) audio summed.
    pub seconds: f64,
    /// The chromagram summed, for combining summaries.
    pub chroma: [f32; 12],
}

/// The grid a curve is given on: 1/24 octave from 20 Hz to 300 Hz, 1/12 to
/// 2 kHz, 1/6 to 20 kHz. Each point is the centre of its band.
pub fn tonal_grid() -> Vec<f32> {
    grid_bands().into_iter().map(|(f, _, _)| f).collect()
}

/// Each grid point: centre, lower and upper edge.
fn grid_bands() -> Vec<(f32, f32, f32)> {
    let mut bands = Vec::new();
    let mut f = GRID_RANGE.0;
    while f <= GRID_RANGE.1 {
        let per_octave = if f < FINE_BELOW_HZ {
            24.0
        } else if f < MEDIUM_BELOW_HZ {
            12.0
        } else {
            6.0
        };
        let half = 2f32.powf(0.5 / per_octave);
        bands.push((f, f / half, f * half));
        f *= 2f32.powf(1.0 / per_octave);
    }
    bands
}

/// The index range of grid points below [`FINE_BELOW_HZ`]: the 1/24-octave
/// part, where a semitone is two points and key shifts move the curve.
pub fn fine_points() -> usize {
    tonal_grid().iter().filter(|&&f| f < FINE_BELOW_HZ).count()
}

/// Moves the curve's fine part (below 300 Hz) by `semitones`, two grid points
/// each, holding its end values; the rest stays. Used to put songs in a
/// common key and a Target in the mix's.
pub fn shift_low_end(curve: &[f32], semitones: i32) -> Vec<f32> {
    let fine = fine_points().min(curve.len());
    let steps = semitones * 2;
    let mut out = curve.to_vec();
    if fine == 0 || steps == 0 {
        return out;
    }
    for (i, value) in out.iter_mut().enumerate().take(fine) {
        let from = (i as i32 - steps).clamp(0, fine as i32 - 1) as usize;
        *value = curve[from];
    }
    out
}

/// The squared magnitude of BS.1770's K-weighting at `f` Hz (its 48 kHz
/// coefficients, evaluated in the frequency domain).
fn k_weight(f: f64) -> f64 {
    let biquad = |b: [f64; 3], a: [f64; 3]| {
        let w = TAU * f / 48_000.0;
        let (c1, s1, c2, s2) = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
        let num = (b[0] + b[1] * c1 + b[2] * c2, -(b[1] * s1 + b[2] * s2));
        let den = (a[0] + a[1] * c1 + a[2] * c2, -(a[1] * s1 + a[2] * s2));
        (num.0 * num.0 + num.1 * num.1) / (den.0 * den.0 + den.1 * den.1)
    };
    let shelf = biquad(
        [
            1.535_124_859_586_97,
            -2.691_696_189_406_38,
            1.198_392_810_852_85,
        ],
        [1.0, -1.690_659_293_182_41, 0.732_480_774_215_85],
    );
    let high_pass = biquad(
        [1.0, -2.0, 1.0],
        [1.0, -1.990_047_454_833_98, 0.990_072_250_366_21],
    );
    shelf * high_pass
}

fn lufs(power: f32) -> f32 {
    if power > 0.0 {
        -0.691 + 10.0 * power.log10()
    } else {
        f32::NEG_INFINITY
    }
}

fn db(power: f64) -> f32 {
    if power > 0.0 {
        (10.0 * power.log10()) as f32
    } else {
        -200.0
    }
}

/// Krumhansl–Kessler key profiles, from C.
const MAJOR: [f32; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const MINOR: [f32; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

/// The key whose profile best matches the chromagram, and how far ahead of
/// the next best it is. `None` for an empty chromagram.
pub fn detect_key(chroma: &[f32; 12]) -> Option<KeyEstimate> {
    let total: f32 = chroma.iter().sum();
    if total <= 0.0 {
        return None;
    }
    let correlate = |profile: &[f32; 12], tonic: usize| -> f32 {
        let n = 12.0;
        let (mut sx, mut sy, mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (i, &x) in chroma.iter().enumerate() {
            let y = profile[(i + 12 - tonic) % 12];
            sx += x;
            sy += y;
            sxy += x * y;
            sxx += x * x;
            syy += y * y;
        }
        let cov = sxy - sx * sy / n;
        let var = (sxx - sx * sx / n) * (syy - sy * sy / n);
        if var > 0.0 { cov / var.sqrt() } else { 0.0 }
    };
    let mut scores: Vec<(f32, Key)> = (0..12)
        .flat_map(|tonic| {
            [
                (
                    correlate(&MAJOR, tonic),
                    Key {
                        tonic: tonic as u8,
                        minor: false,
                    },
                ),
                (
                    correlate(&MINOR, tonic),
                    Key {
                        tonic: tonic as u8,
                        minor: true,
                    },
                ),
            ]
        })
        .collect();
    scores.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (best, key) = scores[0];
    // The next best that isn't the relative major/minor (same notes).
    let relative = |k: Key| {
        if key.minor {
            !k.minor && k.tonic == (key.tonic + 3) % 12
        } else {
            k.minor && k.tonic == (key.tonic + 9) % 12
        }
    };
    let next = scores[1..]
        .iter()
        .find(|(_, k)| !relative(*k))
        .map_or(0.0, |(s, _)| *s);
    let confidence = ((best - next) * 4.0).clamp(0.0, 1.0) * best.max(0.0);
    Some(KeyEstimate { key, confidence })
}

/// Stereo audio in, [`Frame`]s out.
pub struct FrameAnalyser {
    sample_rate: u32,
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    /// Converts |X|² to its share of the mean square.
    scale: f32,
    /// The last FFT-size samples of Left and Right. Their powers add, as
    /// loudness measurement adds channels, so out-of-phase audio still counts.
    history: [Vec<f32>; 2],
    write: usize,
    /// Samples since the last frame.
    since: usize,
    hop: usize,
    input: Vec<f32>,
    output: Vec<Complex32>,
    scratch: Vec<Complex32>,
    /// For each grid band: its first and end bin, and its width in octaves.
    bands: Vec<(usize, usize, f32, f32)>,
    /// Each bin's K-weighting and pitch class (if in the chroma range).
    k: Vec<f32>,
    pitch: Vec<Option<u8>>,
    low: LowEnd,
    frames: Vec<Frame>,
}

impl FrameAnalyser {
    pub fn new(sample_rate: u32, mono_cutoff: f32) -> FrameAnalyser {
        let size = ((f64::from(sample_rate) * FFT_SECONDS).ceil() as usize).next_power_of_two();
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(size);
        let window: Vec<f32> = (0..size)
            .map(|i| (0.5 - 0.5 * (TAU * i as f64 / size as f64).cos()) as f32)
            .collect();
        let energy: f64 = window.iter().map(|&w| f64::from(w * w)).sum();
        let bin_hz = f64::from(sample_rate) / size as f64;
        let bins = size / 2 + 1;
        let bands = grid_bands()
            .into_iter()
            .map(|(f, low, high)| {
                let first = ((f64::from(low) / bin_hz).ceil() as usize).clamp(1, bins - 1);
                let end = ((f64::from(high) / bin_hz).ceil() as usize).clamp(first, bins);
                let octaves = (high / low).log2();
                (first, end, octaves, (f64::from(f) / bin_hz) as f32)
            })
            .collect();
        let k = (0..bins)
            .map(|i| k_weight(i as f64 * bin_hz) as f32)
            .collect();
        let pitch = (0..bins)
            .map(|i| {
                let f = i as f32 * bin_hz as f32;
                (CHROMA_RANGE.0..=CHROMA_RANGE.1)
                    .contains(&f)
                    .then(|| ((12.0 * (f / 440.0).log2()).round() as i32 + 69).rem_euclid(12) as u8)
            })
            .collect();
        FrameAnalyser {
            sample_rate,
            input: fft.make_input_vec(),
            output: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
            window,
            scale: (2.0 / (size as f64 * energy)) as f32,
            history: [vec![0.0; size], vec![0.0; size]],
            write: 0,
            since: 0,
            hop: (f64::from(sample_rate) * FRAME_SECONDS).round() as usize,
            bands,
            k,
            pitch,
            low: LowEnd::new(sample_rate, mono_cutoff),
            frames: Vec::new(),
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Feeds interleaved stereo frames; finished [`Frame`]s are collected
    /// for [`take_frames`](Self::take_frames).
    pub fn process(&mut self, interleaved: &[f32]) {
        let size = self.history[0].len();
        for pair in interleaved.chunks_exact(2) {
            let (l, r) = (pair[0], pair[1]);
            self.history[0][self.write] = l;
            self.history[1][self.write] = r;
            self.write = (self.write + 1) % size;
            self.low.push(l, r);
            self.since += 1;
            if self.since >= self.hop {
                self.since = 0;
                let frame = self.frame();
                self.frames.push(frame);
            }
        }
    }

    /// The frames finished since the last call.
    pub fn take_frames(&mut self) -> Vec<Frame> {
        std::mem::take(&mut self.frames)
    }

    fn frame(&mut self) -> Frame {
        let size = self.history[0].len();
        // Each channel's power per bin, averaged: the mean square of the two.
        let mut power = vec![0.0f32; self.output.len()];
        for channel in 0..2 {
            let history = &self.history[channel];
            for (i, (sample, w)) in self.input.iter_mut().zip(&self.window).enumerate() {
                *sample = history[(self.write + i) % size] * w;
            }
            self.fft
                .process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch)
                .expect("buffers sized by the planner");
            for (p, x) in power.iter_mut().zip(&self.output).skip(1) {
                *p += 0.5 * x.norm_sqr() * self.scale;
            }
        }
        let bands = self
            .bands
            .iter()
            .map(|&(first, end, _, centre)| {
                if first < end {
                    power[first..end].iter().sum()
                } else {
                    // Narrower than a bin: the bins either side, shared out.
                    let below = (centre.floor() as usize).clamp(1, power.len() - 2);
                    let t = centre - below as f32;
                    (power[below] * (1.0 - t) + power[below + 1] * t) * 0.5
                }
            })
            .collect();
        let mut chroma = [0.0f32; 12];
        for (p, class) in power.iter().zip(&self.pitch) {
            if let Some(class) = class {
                chroma[usize::from(*class)] += p;
            }
        }
        let loudness_power = power.iter().zip(&self.k).map(|(p, k)| p * k).sum();
        let (low_lr, crest) = self.low.take();
        Frame {
            bands,
            chroma,
            loudness_power,
            low_lr,
            crest,
        }
    }
}

/// The low end's running figures between frames: correlation sums below the
/// mono cutoff, and the Sub and Bass bands' peak and mean square.
struct LowEnd {
    mono: [Biquad; 4],
    sub: [Biquad; 2],
    bass: [Biquad; 2],
    sums: [f64; 3],
    peaks: [f32; 2],
    squares: [f64; 2],
    count: u64,
}

impl LowEnd {
    fn new(sample_rate: u32, cutoff: f32) -> LowEnd {
        let rate = f64::from(sample_rate);
        let low = |f: f64| Biquad::low_pass(rate, f);
        let high = |f: f64| Biquad::high_pass(rate, f);
        let cutoff = f64::from(cutoff);
        LowEnd {
            // Left twice, Right twice: 24 dB per octave each.
            mono: [low(cutoff), low(cutoff), low(cutoff), low(cutoff)],
            sub: [high(20.0), low(60.0)],
            bass: [high(60.0), low(120.0)],
            sums: [0.0; 3],
            peaks: [0.0; 2],
            squares: [0.0; 2],
            count: 0,
        }
    }

    fn push(&mut self, l: f32, r: f32) {
        let l_low = self.mono[0].run(l);
        let l_low = self.mono[1].run(l_low);
        let r_low = self.mono[2].run(r);
        let r_low = self.mono[3].run(r_low);
        self.sums[0] += f64::from(l_low * r_low);
        self.sums[1] += f64::from(l_low * l_low);
        self.sums[2] += f64::from(r_low * r_low);
        let mid = 0.5 * (l + r);
        let sub = self.sub[0].run(mid);
        let sub = self.sub[1].run(sub);
        let bass = self.bass[0].run(mid);
        let bass = self.bass[1].run(bass);
        for (i, x) in [sub, bass].into_iter().enumerate() {
            self.peaks[i] = self.peaks[i].max(x * x);
            self.squares[i] += f64::from(x * x);
        }
        self.count += 1;
    }

    fn take(&mut self) -> ([f64; 3], [[f32; 2]; 2]) {
        let n = self.count.max(1) as f64;
        let crest = [
            [self.peaks[0], (self.squares[0] / n) as f32],
            [self.peaks[1], (self.squares[1] / n) as f32],
        ];
        let sums = self.sums.map(|s| s / n);
        self.sums = [0.0; 3];
        self.peaks = [0.0; 2];
        self.squares = [0.0; 2];
        self.count = 0;
        (sums, crest)
    }
}

/// A second-order Butterworth section (RBJ cookbook).
#[derive(Clone, Copy)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl Biquad {
    fn new(rate: f64, f: f64, high: bool) -> Biquad {
        let w = TAU * f / rate;
        let alpha = w.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
        let cos = w.cos();
        let a0 = 1.0 + alpha;
        let (b0, b1) = if high {
            ((1.0 + cos) / 2.0, -(1.0 + cos))
        } else {
            ((1.0 - cos) / 2.0, 1.0 - cos)
        };
        Biquad {
            b: [(b0 / a0) as f32, (b1 / a0) as f32, (b0 / a0) as f32],
            a: [(-2.0 * cos / a0) as f32, ((1.0 - alpha) / a0) as f32],
            z: [0.0; 2],
        }
    }

    fn low_pass(rate: f64, f: f64) -> Biquad {
        Biquad::new(rate, f, false)
    }

    fn high_pass(rate: f64, f: f64) -> Biquad {
        Biquad::new(rate, f, true)
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// Which frames a summary takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parts {
    /// Every non-silent frame.
    Whole,
    /// Only frames whose short-term loudness is within [`LOUD_PARTS_LU`] of
    /// the loudest: a song's drops and choruses.
    Loud,
}

/// Sums frames into a [`Summary`].
pub fn summarise(frames: &[Frame], parts: Parts) -> Option<Summary> {
    let heard: Vec<(usize, &Frame)> = frames
        .iter()
        .enumerate()
        .filter(|(_, f)| f.loudness() > SILENCE_LUFS)
        .collect();
    if heard.is_empty() {
        return None;
    }
    let chosen: Vec<&Frame> = match parts {
        Parts::Whole => heard.iter().map(|(_, f)| *f).collect(),
        Parts::Loud => {
            // Short-term loudness: the frame and the ones before it (3 s).
            let short: Vec<f32> = (0..frames.len())
                .map(|i| {
                    let from = i.saturating_sub(SHORT_TERM_FRAMES - 1);
                    let span = &frames[from..=i];
                    lufs(span.iter().map(|f| f.loudness_power).sum::<f32>() / span.len() as f32)
                })
                .collect();
            let loudest = heard
                .iter()
                .map(|&(i, _)| short[i])
                .fold(f32::MIN, f32::max);
            heard
                .iter()
                .filter(|&&(i, _)| short[i] >= loudest - LOUD_PARTS_LU)
                .map(|(_, f)| *f)
                .collect()
        }
    };
    Some(sum(&chosen))
}

fn sum(frames: &[&Frame]) -> Summary {
    let n = frames.len().max(1) as f64;
    let points = frames.first().map_or(0, |f| f.bands.len());
    let mut bands = vec![0.0f64; points];
    let mut chroma = [0.0f32; 12];
    let mut loudness_power = 0.0f64;
    let mut lr = [0.0f64; 3];
    let mut crest = [[0.0f64; 2]; 2];
    let mut crest_count = [0usize; 2];
    for frame in frames {
        for (sum, &p) in bands.iter_mut().zip(&frame.bands) {
            *sum += f64::from(p);
        }
        for (sum, &c) in chroma.iter_mut().zip(&frame.chroma) {
            *sum += c;
        }
        loudness_power += f64::from(frame.loudness_power);
        for (sum, &v) in lr.iter_mut().zip(&frame.low_lr) {
            *sum += v;
        }
        for (i, &[peak, square]) in frame.crest.iter().enumerate() {
            if square > 1e-10 {
                // Each frame's crest in dB, averaged: a single click
                // doesn't decide it.
                crest[i][0] += f64::from(10.0 * (peak / square).log10());
                crest_count[i] += 1;
            }
        }
    }
    let loudness = lufs((loudness_power / n) as f32);
    let grid = grid_bands();
    let curve: Vec<f32> = bands
        .iter()
        .zip(&grid)
        .map(|(&p, &(_, low, high))| db(p / n / f64::from((high / low).log2())) - loudness)
        .collect();
    let regions = Region::ALL.map(|region| {
        let (low, high) = region.range();
        let power: f64 = bands
            .iter()
            .zip(&grid)
            .filter(|(_, (f, _, _))| (low..high).contains(f))
            .map(|(&p, _)| p / n)
            .sum();
        db(power) - loudness
    });
    // Below −80 dBFS of mean square the low end counts as silent.
    let low_end_mono = (lr[1] / n > 1e-8 && lr[2] / n > 1e-8)
        .then(|| (lr[0] / (lr[1] * lr[2]).sqrt()).clamp(-1.0, 1.0) as f32);
    let low_end_crest =
        [0, 1].map(|i| (crest_count[i] > 0).then(|| (crest[i][0] / crest_count[i] as f64) as f32));
    let fundamental = |region: Region| -> Option<Fundamental> {
        let (low, high) = region.range();
        let (i, _) = grid
            .iter()
            .enumerate()
            .filter(|(_, (f, _, _))| (low..high).contains(f))
            .max_by(|a, b| bands[a.0].total_cmp(&bands[b.0]))?;
        if bands[i] <= 0.0 {
            return None;
        }
        // Refined between grid points by a parabola through the log levels.
        let at = |j: usize| db(bands[j.clamp(0, bands.len() - 1)]);
        let (l, c, r) = (at(i.saturating_sub(1)), at(i), at(i + 1));
        let curve = l - 2.0 * c + r;
        let offset = if curve < 0.0 {
            (0.5 * (l - r) / curve).clamp(-0.5, 0.5)
        } else {
            0.0
        };
        let step = (grid[(i + 1).min(grid.len() - 1)].0 / grid[i].0).log2();
        let frequency = grid[i].0 * 2f32.powf(offset * step);
        Some(Fundamental {
            frequency,
            note: note_name(frequency)?,
        })
    };
    Summary {
        curve,
        regions,
        loudness,
        low_end_mono,
        low_end_crest,
        sub: fundamental(Region::Sub),
        bass: fundamental(Region::Bass),
        key: detect_key(&chroma),
        seconds: frames.len() as f64 * FRAME_SECONDS,
        chroma,
    }
}

/// The live side: the frames of a sliding span, or (Whole) every frame
/// since a reset, up to [`WHOLE_LIMIT_FRAMES`]; summed on demand. Silence is
/// left out.
pub struct LiveSummary {
    averaging: LongTerm,
    frames: VecDeque<Frame>,
}

/// Whole keeps at most two hours of frames, dropping the oldest.
pub const WHOLE_LIMIT_FRAMES: usize = (2.0 * 3600.0 / FRAME_SECONDS) as usize;

impl LiveSummary {
    pub fn new(averaging: LongTerm) -> LiveSummary {
        LiveSummary {
            averaging,
            frames: VecDeque::new(),
        }
    }

    pub fn reset(&mut self) {
        self.frames.clear();
    }

    pub fn push(&mut self, frame: Frame) {
        if frame.loudness() <= SILENCE_LUFS {
            return;
        }
        let keep = match self.averaging {
            LongTerm::Off => return,
            LongTerm::Whole => WHOLE_LIMIT_FRAMES,
            LongTerm::Seconds(seconds) => {
                ((f64::from(seconds) / FRAME_SECONDS).round() as usize).max(1)
            }
        };
        self.frames.push_back(frame);
        while self.frames.len() > keep {
            self.frames.pop_front();
        }
    }

    pub fn summary(&self) -> Option<Summary> {
        if self.frames.is_empty() {
            return None;
        }
        let frames: Vec<&Frame> = self.frames.iter().collect();
        Some(sum(&frames))
    }

    /// The frames held, oldest first: a Target's song taught by playing.
    pub fn frames(&self) -> impl Iterator<Item = &Frame> {
        self.frames.iter()
    }
}
