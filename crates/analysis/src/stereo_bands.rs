//! The Stereo Bands Meter's analysis (#100): per octave band, how alike Left
//! and Right are (band correlation) and how much of the band is Side (width).
//!
//! Each band is Left and Right through the same octave band-pass (a 4th-order
//! Butterworth high-pass at the band's low edge and low-pass at its high
//! edge), with the means of L·R, L² and R² averaged exponentially (τ = 300 ms
//! by default, as the Stereometer). Then, per band:
//!
//! - **band correlation** `Σ(L·R)/√(Σ(L²)·Σ(R²))`, −1 to +1, the
//!   Stereometer's Correlation (ADR 0005) on the band alone;
//! - **width** `S²/(M²+S²)` with `M = (L+R)/2`, `S = (L−R)/2`: 0 mono, ½ for
//!   unrelated channels of equal level, 1 for opposite polarity.
//!
//! A band quieter than −80 dBFS (mean square) has neither. Tonal Balance's
//! Low end mono uses the same [`correlation`].

use std::time::Duration;

use crate::tonal_balance::Biquad;

/// The bands' centres, an octave apart: 31 Hz to 16 kHz.
pub const CENTRES: [f32; 10] = [
    31.25, 62.5, 125.0, 250.0, 500.0, 1_000.0, 2_000.0, 4_000.0, 8_000.0, 16_000.0,
];
/// Below this mean square (−80 dBFS) a band reads as silent.
const SILENT: f64 = 1e-8;

/// The correlation of L and R from the means of L·R, L² and R²; `None` when
/// either channel is silent.
pub fn correlation(lr: f64, ll: f64, rr: f64) -> Option<f32> {
    (ll > SILENT && rr > SILENT).then(|| (lr / (ll * rr).sqrt()).clamp(-1.0, 1.0) as f32)
}

/// The width `S²/(M²+S²)` from the same means; `None` in silence.
pub fn width(lr: f64, ll: f64, rr: f64) -> Option<f32> {
    // M² = (L² + 2LR + R²)/4, S² = (L² − 2LR + R²)/4.
    let (mid, side) = ((ll + 2.0 * lr + rr) / 4.0, (ll - 2.0 * lr + rr) / 4.0);
    (mid + side > SILENT).then(|| (side.max(0.0) / (mid + side)).clamp(0.0, 1.0) as f32)
}

/// One band's reading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BandReading {
    pub centre: f32,
    pub correlation: Option<f32>,
    pub width: Option<f32>,
    /// The band's level (mean square of L and R together) in dBFS.
    pub level: f32,
}

struct Band {
    /// Left and Right, each through a high-pass pair and a low-pass pair.
    filters: [[Biquad; 4]; 2],
    /// Averaged L·R, L², R².
    sums: [f64; 3],
}

/// Stereo frames in, a reading per band out.
pub struct StereoBandsAnalyser {
    sample_rate: u32,
    time: Duration,
    alpha: f64,
    bands: Vec<Band>,
}

impl StereoBandsAnalyser {
    /// `time` is the averaging's time constant. Bands at or over Nyquist are left out.
    pub fn new(sample_rate: u32, time: Duration) -> StereoBandsAnalyser {
        let rate = f64::from(sample_rate);
        let bands = CENTRES
            .iter()
            .map(|&centre| f64::from(centre))
            .filter(|&centre| centre * std::f64::consts::SQRT_2 < rate / 2.0)
            .map(|centre| {
                let (low, high) = (
                    centre / std::f64::consts::SQRT_2,
                    centre * std::f64::consts::SQRT_2,
                );
                let chain = [
                    Biquad::high_pass(rate, low),
                    Biquad::high_pass(rate, low),
                    Biquad::low_pass(rate, high),
                    Biquad::low_pass(rate, high),
                ];
                Band {
                    filters: [chain, chain],
                    sums: [0.0; 3],
                }
            })
            .collect();
        StereoBandsAnalyser {
            sample_rate,
            time,
            alpha: 1.0 - (-1.0 / (rate * time.as_secs_f64().max(1e-3))).exp(),
            bands,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn time(&self) -> Duration {
        self.time
    }

    pub fn process(&mut self, interleaved: &[f32]) {
        let alpha = self.alpha;
        for band in &mut self.bands {
            for frame in interleaved.chunks_exact(2) {
                let [left, right] = &mut band.filters;
                let l = left.iter_mut().fold(frame[0], |x, f| f.run(x));
                let r = right.iter_mut().fold(frame[1], |x, f| f.run(x));
                let (l, r) = (f64::from(l), f64::from(r));
                for (sum, value) in band.sums.iter_mut().zip([l * r, l * l, r * r]) {
                    *sum += alpha * (value - *sum);
                }
            }
        }
    }

    pub fn readings(&self) -> Vec<BandReading> {
        self.bands
            .iter()
            .zip(CENTRES)
            .map(|(band, centre)| {
                let [lr, ll, rr] = band.sums;
                let power = (ll + rr) / 2.0;
                BandReading {
                    centre,
                    correlation: correlation(lr, ll, rr),
                    width: width(lr, ll, rr),
                    level: if power > 0.0 {
                        (10.0 * power.log10()) as f32
                    } else {
                        f32::NEG_INFINITY
                    },
                }
            })
            .collect()
    }
}
