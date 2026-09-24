//! The Stereometer's analyser: a point buffer for Polar and Lissajous,
//! correlation, balance and the mono flag.
//!
//! Correlation per ADR 0005: Pearson `Σ(L·R)/√(Σ(L²)·Σ(R²))`, no DC removal,
//! the sums averaged exponentially with τ = 300 ms by default.

use std::f32::consts::FRAC_1_SQRT_2;
use std::time::Duration;

/// Below this level (dBFS, as mean-square power) a channel counts as silent.
pub const SILENCE_DB: f64 = -90.0;

/// Auto-gain never amplifies by more than this (+60 dB), so the noise floor stays small.
const MAX_AUTO_GAIN: f32 = 1_000.0;

/// How fast auto-gain lets its peak fall.
const AUTO_GAIN_RELEASE: Duration = Duration::from_secs(1);

/// How the point buffer is scaled for display.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum StereoScaling {
    /// Follows the recent peak, so quiet material fills the scope. The default.
    #[default]
    Auto,
    /// A fixed linear gain: 1.0 puts full scale at the edge.
    Fixed { gain: f32 },
}

/// How the points are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum StereoView {
    /// A half circle: mono points straight up, out-of-phase ones along the base line. The default.
    #[default]
    Polar,
    /// The classic goniometer, rotated 45° so mono is vertical.
    Lissajous,
}

/// The Stereometer's settings that affect analysis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StereometerSettings {
    /// Correlation averaging time constant. Default 300 ms.
    pub correlation_time: Duration,
    pub scaling: StereoScaling,
    /// How many of the newest frames the point buffer keeps. Default 2048.
    pub points: usize,
}

impl Default for StereometerSettings {
    fn default() -> Self {
        StereometerSettings {
            correlation_time: Duration::from_millis(300),
            scaling: StereoScaling::Auto,
            points: 2_048,
        }
    }
}

/// The Stereometer's numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StereoReadings {
    /// −1 (out of phase) to +1 (mono). 0 when there is no signal or only one channel has signal.
    pub correlation: f32,
    /// Both channels are below −90 dBFS: the bar dims as "no signal".
    pub no_signal: bool,
    /// −1 (all left) to +1 (all right), from the channels' RMS. 0 when silent.
    pub balance: f32,
    /// The Source says it's mono (a Send Plugin's mono flag), so the UI can say so.
    pub mono: bool,
}

/// Stereo frames in, points and [`StereoReadings`] out.
pub struct StereometerAnalyser {
    sample_rate: u32,
    settings: StereometerSettings,
    /// Per-sample weight of the exponential average.
    alpha: f64,
    sum_lr: f64,
    sum_ll: f64,
    sum_rr: f64,
    /// Newest frames as (side, mid), unscaled, in a ring.
    points: Vec<[f32; 2]>,
    next: usize,
    filled: usize,
    peak: f32,
    peak_decay: f32,
    mono: bool,
}

impl StereometerAnalyser {
    pub fn new(sample_rate: u32, settings: StereometerSettings) -> StereometerAnalyser {
        let rate = f64::from(sample_rate.max(1));
        let tau = settings.correlation_time.as_secs_f64().max(1e-6);
        let release = AUTO_GAIN_RELEASE.as_secs_f64();
        StereometerAnalyser {
            sample_rate,
            settings,
            alpha: 1.0 - (-1.0 / (tau * rate)).exp(),
            sum_lr: 0.0,
            sum_ll: 0.0,
            sum_rr: 0.0,
            points: vec![[0.0; 2]; settings.points.max(1)],
            next: 0,
            filled: 0,
            peak: 0.0,
            peak_decay: (-1.0 / (release * rate)).exp() as f32,
            mono: false,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Restarts for a new sample rate. The mono flag is kept.
    pub fn set_sample_rate(&mut self, sample_rate: u32) {
        let mono = self.mono;
        *self = StereometerAnalyser::new(sample_rate, self.settings);
        self.mono = mono;
    }

    /// Applies new settings. The analyser restarts; the mono flag is kept.
    pub fn set_settings(&mut self, settings: StereometerSettings) {
        let mono = self.mono;
        *self = StereometerAnalyser::new(self.sample_rate, settings);
        self.mono = mono;
    }

    /// Passes through the Source's mono flag.
    pub fn set_mono(&mut self, mono: bool) {
        self.mono = mono;
    }

    /// Feeds interleaved stereo frames. A trailing half frame is ignored.
    pub fn process(&mut self, interleaved: &[f32]) {
        let a = self.alpha;
        for frame in interleaved.chunks_exact(2) {
            let (l, r) = (frame[0], frame[1]);
            let (lf, rf) = (f64::from(l), f64::from(r));
            self.sum_lr += a * (lf * rf - self.sum_lr);
            self.sum_ll += a * (lf * lf - self.sum_ll);
            self.sum_rr += a * (rf * rf - self.sum_rr);

            let side = (r - l) * FRAC_1_SQRT_2;
            let mid = (l + r) * FRAC_1_SQRT_2;
            self.points[self.next] = [side, mid];
            self.next = (self.next + 1) % self.points.len();
            self.filled = (self.filled + 1).min(self.points.len());
            self.peak = (self.peak * self.peak_decay).max(side.abs()).max(mid.abs());
        }
    }

    pub fn readings(&self) -> StereoReadings {
        let silent =
            |mean_square: f64| mean_square <= 0.0 || 10.0 * mean_square.log10() < SILENCE_DB;
        let (left_silent, right_silent) = (silent(self.sum_ll), silent(self.sum_rr));
        let correlation = if left_silent || right_silent {
            0.0
        } else {
            (self.sum_lr / (self.sum_ll * self.sum_rr).sqrt()).clamp(-1.0, 1.0) as f32
        };
        let (left, right) = (self.sum_ll.max(0.0).sqrt(), self.sum_rr.max(0.0).sqrt());
        let balance = if left_silent && right_silent {
            0.0
        } else {
            ((right - left) / (right + left)) as f32
        };
        StereoReadings {
            correlation,
            no_signal: left_silent && right_silent,
            balance,
            mono: self.mono,
        }
    }

    /// The display gain in use: fixed, or following the recent peak.
    pub fn gain(&self) -> f32 {
        match self.settings.scaling {
            StereoScaling::Fixed { gain } => gain,
            StereoScaling::Auto if self.peak > 0.0 => (1.0 / self.peak).min(MAX_AUTO_GAIN),
            StereoScaling::Auto => 1.0,
        }
    }

    /// Writes the buffered points, oldest first, as `[x, y]` in −1..1 for `view`,
    /// scaled by [`gain`](Self::gain) and clamped to the edge.
    ///
    /// Lissajous: x is side (left is negative), y is mid. Polar: the same, folded
    /// into the upper half so mono points up and out-of-phase audio lies flat.
    pub fn points(&self, view: StereoView, out: &mut Vec<[f32; 2]>) {
        out.clear();
        let gain = self.gain();
        let len = self.points.len();
        let start = (self.next + len - self.filled) % len;
        out.extend((0..self.filled).map(|i| {
            let [side, mid] = self.points[(start + i) % len];
            let [x, y] = match view {
                StereoView::Lissajous => [side, mid],
                StereoView::Polar if mid < 0.0 => [-side, -mid],
                StereoView::Polar => [side, mid],
            };
            [(x * gain).clamp(-1.0, 1.0), (y * gain).clamp(-1.0, 1.0)]
        }));
    }
}
