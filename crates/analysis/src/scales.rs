//! The Loudness Meter's bar scales (#104) and their ballistics: VU, the
//! PPMs of IEC 60268-10 and the K-System. Definitions (ADR 0007):
//!
//! - **VU** (IEC 60268-17): the full-wave rectified signal through a
//!   second-order filter that first reaches 99% of a step in 300 ms and
//!   overshoots it by 1.25% (the standard asks 1–1.5%). Scaled so a steady
//!   sine reads its peak level, and 0 VU is the reference (a sine peaking at
//!   −18 dBFS by default).
//! - **PPM Type I** (DIN, Nordic): quasi-peak; a 5 ms burst of 5 kHz reads
//!   80% (−1.9 dB) of the steady tone; it falls back 20 dB in 1.7 s.
//! - **PPM Type II** (BBC, EBU): a 10 ms burst reads 80%; it falls back
//!   24 dB in 2.8 s.
//! - **K-12, K-14, K-20**: the Meter's RMS (AES17 by default) with 0 at
//!   −12, −14 or −20 dB, and the sample peak over it.
//!
//! A ballistic level is kept in dBFS of a sine's peak, so every scale can
//! place it; [`Scale::marks`] says where its own numbers go.

use std::f64::consts::{FRAC_PI_2, LN_10};

/// The scales the bars can use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum Scale {
    /// Sample peak and RMS in dBFS, over the bar range. The default.
    #[default]
    Peak,
    Vu,
    PpmDin,
    PpmNordic,
    PpmBbc,
    PpmEbu,
    K12,
    K14,
    K20,
}

/// The VU reference when none is chosen: 0 VU for a sine peaking at −18 dBFS.
pub const DEFAULT_VU_REFERENCE: f64 = -18.0;
/// The PPMs' alignment level (EBU R68): a sine peaking at −18 dBFS.
pub const PPM_ALIGNMENT: f64 = -18.0;
/// The DIN PPM's 0 dB: the permitted maximum level, 9 dB over alignment.
pub const DIN_ZERO: f64 = -9.0;

/// Type I's attack time constant: a 5 ms burst reaches 80% (calibrated).
const TYPE_I_ATTACK: f64 = 0.001_243_5;
/// Type II's: a 10 ms burst reaches 80%.
const TYPE_II_ATTACK: f64 = 0.002_486_7;
/// The VU's damping (1.25% overshoot) and natural frequency (99% at 300 ms).
const VU_DAMPING: f64 = 0.812_717;
const VU_NATURAL: f64 = 13.512_05;

impl Scale {
    pub const ALL: [Scale; 9] = [
        Scale::Peak,
        Scale::Vu,
        Scale::PpmDin,
        Scale::PpmNordic,
        Scale::PpmBbc,
        Scale::PpmEbu,
        Scale::K12,
        Scale::K14,
        Scale::K20,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Scale::Peak => "Peak",
            Scale::Vu => "VU",
            Scale::PpmDin => "PPM DIN",
            Scale::PpmNordic => "PPM Nordic",
            Scale::PpmBbc => "PPM BBC",
            Scale::PpmEbu => "PPM EBU",
            Scale::K12 => "K-12",
            Scale::K14 => "K-14",
            Scale::K20 => "K-20",
        }
    }

    /// Whether the bars show a ballistic reading (VU or PPM) instead of RMS.
    pub fn ballistic(self) -> bool {
        matches!(
            self,
            Scale::Vu | Scale::PpmDin | Scale::PpmNordic | Scale::PpmBbc | Scale::PpmEbu
        )
    }

    /// The K-System's 0, in dB of RMS, for a K scale.
    pub fn k_zero(self) -> Option<f64> {
        match self {
            Scale::K12 => Some(-12.0),
            Scale::K14 => Some(-14.0),
            Scale::K20 => Some(-20.0),
            _ => None,
        }
    }

    /// The bars' range in dBFS (`None` for Peak, which has its own setting).
    pub fn range(self, vu_reference: f64) -> Option<(f64, f64)> {
        Some(match self {
            Scale::Peak => return None,
            Scale::Vu => (vu_reference - 20.0, vu_reference + 3.0),
            Scale::PpmDin => (DIN_ZERO - 50.0, DIN_ZERO + 5.0),
            Scale::PpmNordic => (PPM_ALIGNMENT - 36.0, PPM_ALIGNMENT + 9.0),
            // Mark 1 to mark 7, 4 dB apart, mark 4 at alignment; a little room each end.
            Scale::PpmBbc => (PPM_ALIGNMENT - 14.0, PPM_ALIGNMENT + 14.0),
            Scale::PpmEbu => (PPM_ALIGNMENT - 12.0, PPM_ALIGNMENT + 12.0),
            Scale::K12 | Scale::K14 | Scale::K20 => {
                let zero = self.k_zero().expect("a K scale");
                (zero - 30.0, 0.0)
            }
        })
    }

    /// The scale's own numbers: where each goes (dBFS) and what it says.
    pub fn marks(self, vu_reference: f64) -> Vec<(f64, String)> {
        let at = |zero: f64, values: &[i32]| -> Vec<(f64, String)> {
            values
                .iter()
                .map(|&v| {
                    let label = if v > 0 {
                        format!("+{v}")
                    } else {
                        v.to_string()
                    };
                    (zero + f64::from(v), label)
                })
                .collect()
        };
        match self {
            Scale::Peak => Vec::new(),
            Scale::Vu => at(vu_reference, &[-20, -10, -7, -5, -3, -2, -1, 0, 1, 2, 3]),
            Scale::PpmDin => at(DIN_ZERO, &[-50, -40, -30, -20, -10, -5, 0, 5]),
            Scale::PpmNordic => at(PPM_ALIGNMENT, &[-36, -30, -24, -18, -12, -6, 0, 6, 9]),
            Scale::PpmBbc => (1..=7)
                .map(|mark| (PPM_ALIGNMENT + 4.0 * f64::from(mark - 4), mark.to_string()))
                .collect(),
            Scale::PpmEbu => at(PPM_ALIGNMENT, &[-12, -8, -4, 0, 4, 8, 12]),
            Scale::K12 | Scale::K14 | Scale::K20 => {
                let zero = self.k_zero().expect("a K scale");
                let mut marks = at(zero, &[-30, -20, -10, -6, -3, 0, 4]);
                let top = -zero as i32;
                if top > 4 {
                    marks.push((0.0, format!("+{top}")));
                }
                marks
            }
        }
    }
}

/// One channel's VU or PPM, per sample.
#[derive(Clone, Copy, Debug)]
pub struct Ballistics {
    scale: Scale,
    dt: f64,
    /// PPM: the reading (linear). VU: the filter's output and its rate.
    level: f64,
    rate: f64,
    attack: f64,
    release: f64,
}

impl Ballistics {
    pub fn new(scale: Scale, sample_rate: u32) -> Ballistics {
        let dt = 1.0 / f64::from(sample_rate.max(1));
        let (attack_time, fall_db, fall_seconds) = match scale {
            Scale::PpmDin | Scale::PpmNordic => (TYPE_I_ATTACK, 20.0, 1.7),
            Scale::PpmBbc | Scale::PpmEbu => (TYPE_II_ATTACK, 24.0, 2.8),
            _ => (1.0, 0.0, 1.0),
        };
        Ballistics {
            scale,
            dt,
            level: 0.0,
            rate: 0.0,
            attack: 1.0 - (-dt / attack_time).exp(),
            // Falls `fall_db` in `fall_seconds`: straight in dB.
            release: (-fall_db / 20.0 * LN_10 * dt / fall_seconds).exp(),
        }
    }

    pub fn push(&mut self, x: f32) {
        let x = f64::from(x).abs();
        match self.scale {
            Scale::Vu => {
                let (z, w) = (VU_DAMPING, VU_NATURAL);
                let acceleration = w * w * (x - self.level) - 2.0 * z * w * self.rate;
                self.rate += acceleration * self.dt;
                self.level += self.rate * self.dt;
            }
            _ if self.scale.ballistic() => {
                if x > self.level {
                    self.level += self.attack * (x - self.level);
                } else {
                    self.level *= self.release;
                }
            }
            _ => {}
        }
    }

    /// The reading in dBFS of a sine's peak: a steady sine reads its peak.
    pub fn db(&self) -> f64 {
        let linear = match self.scale {
            // A full-wave rectified sine averages 2/π of its peak.
            Scale::Vu => self.level.max(0.0) * FRAC_PI_2,
            _ => self.level,
        };
        if linear > 1e-9 {
            20.0 * linear.log10()
        } else {
            f64::NEG_INFINITY
        }
    }
}
