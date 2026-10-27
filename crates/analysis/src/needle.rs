//! The Needle Meter's analyser (#106): a VU or PPM needle per channel (or
//! one on the mono sum), with the ballistics of [`crate::scales`], and an
//! overload lamp that lights when a needle reaches its scale's end and stays
//! lit for [`LAMP_SECONDS`].

use crate::scales::{Ballistics, Scale};

/// How long the overload lamp stays lit, in seconds of audio.
pub const LAMP_SECONDS: f64 = 1.0;

/// One needle as read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NeedleReading {
    /// dBFS of a sine's peak; [`f64::NEG_INFINITY`] at rest.
    pub level: f64,
    /// The overload lamp.
    pub over: bool,
}

/// Stereo frames in, a reading per needle out.
pub struct NeedleAnalyser {
    sample_rate: u32,
    scale: Scale,
    vu_reference: f64,
    mono: bool,
    needles: [Ballistics; 2],
    /// Seconds since each needle was last at its scale's end.
    since_over: [f64; 2],
}

impl NeedleAnalyser {
    /// `scale` should be VU or a PPM; any other falls back to VU.
    pub fn new(sample_rate: u32, scale: Scale, vu_reference: f64, mono: bool) -> NeedleAnalyser {
        let scale = if scale.ballistic() { scale } else { Scale::Vu };
        NeedleAnalyser {
            sample_rate,
            scale,
            vu_reference,
            mono,
            needles: [Ballistics::new(scale, sample_rate); 2],
            since_over: [f64::INFINITY; 2],
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Whether these settings need a new analyser.
    pub fn differs(&self, scale: Scale, vu_reference: f64, mono: bool) -> bool {
        self.scale != scale || self.vu_reference != vu_reference || self.mono != mono
    }

    pub fn process(&mut self, interleaved: &[f32]) {
        for frame in interleaved.chunks_exact(2) {
            if self.mono {
                self.needles[0].push(0.5 * (frame[0] + frame[1]));
            } else {
                self.needles[0].push(frame[0]);
                self.needles[1].push(frame[1]);
            }
        }
        let seconds = (interleaved.len() / 2) as f64 / f64::from(self.sample_rate);
        let top = self
            .scale
            .range(self.vu_reference)
            .map_or(f64::INFINITY, |r| r.1);
        for (needle, since) in self.needles.iter().zip(&mut self.since_over) {
            if needle.db() >= top {
                *since = 0.0;
            } else {
                *since += seconds;
            }
        }
    }

    /// One reading, or two (Left, Right).
    pub fn readings(&self) -> Vec<NeedleReading> {
        let count = if self.mono { 1 } else { 2 };
        (0..count)
            .map(|i| NeedleReading {
                level: self.needles[i].db(),
                over: self.since_over[i] <= LAMP_SECONDS,
            })
            .collect()
    }
}
