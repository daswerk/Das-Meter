//! Synthetic test signals for golden tests and benchmarks.
//!
//! Everything is generated here, so tests need no audio files. Levels are in
//! dBFS of the peak amplitude: a 0 dBFS sine peaks at ±1.0.

use std::f64::consts::TAU;

/// Linear amplitude of a level in dBFS.
pub fn amplitude(dbfs: f64) -> f64 {
    10f64.powf(dbfs / 20.0)
}

/// A mono sine at `frequency` Hz, peaking at `dbfs`, starting at `phase` radians.
pub fn sine(sample_rate: u32, frequency: f64, dbfs: f64, phase: f64, frames: usize) -> Vec<f32> {
    let gain = amplitude(dbfs);
    let step = TAU * frequency / f64::from(sample_rate);
    (0..frames)
        .map(|n| (gain * (phase + step * n as f64).sin()) as f32)
        .collect()
}

/// Mono silence.
pub fn silence(frames: usize) -> Vec<f32> {
    vec![0.0; frames]
}

/// Uniform white noise in [−1, 1), deterministic for a given `seed`.
fn white(seed: u64) -> impl FnMut() -> f64 {
    let mut state = seed | 1;
    move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
    }
}

/// Mono white noise, deterministic for a given `seed`, peaking near `dbfs`.
pub fn white_noise(frames: usize, dbfs: f64, seed: u64) -> Vec<f32> {
    let mut next = white(seed);
    let gain = amplitude(dbfs);
    (0..frames).map(|_| (next() * gain) as f32).collect()
}

/// Mono pink noise (−3 dB per octave), deterministic for a given `seed`, peaking near `dbfs`.
///
/// White noise from a xorshift generator through Paul Kellet's pink filter.
pub fn pink_noise(frames: usize, dbfs: f64, seed: u64) -> Vec<f32> {
    let mut white = white(seed);
    let mut b = [0.0f64; 7];
    let mut out: Vec<f64> = (0..frames)
        .map(|_| {
            let w = white();
            b[0] = 0.99886 * b[0] + w * 0.0555179;
            b[1] = 0.99332 * b[1] + w * 0.0750759;
            b[2] = 0.96900 * b[2] + w * 0.1538520;
            b[3] = 0.86650 * b[3] + w * 0.3104856;
            b[4] = 0.55000 * b[4] + w * 0.5329522;
            b[5] = -0.7616 * b[5] - w * 0.0168980;
            let pink = b.iter().sum::<f64>() + w * 0.5362;
            b[6] = w * 0.115926;
            pink
        })
        .collect();
    let peak = out.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    let gain = if peak > 0.0 {
        amplitude(dbfs) / peak
    } else {
        0.0
    };
    for x in &mut out {
        *x *= gain;
    }
    out.into_iter().map(|x| x as f32).collect()
}

/// Two mono channels as interleaved stereo frames. The shorter channel sets the length.
pub fn stereo(left: &[f32], right: &[f32]) -> Vec<f32> {
    left.iter().zip(right).flat_map(|(&l, &r)| [l, r]).collect()
}

/// The same mono signal on both channels, as interleaved stereo frames.
pub fn both(mono: &[f32]) -> Vec<f32> {
    stereo(mono, mono)
}

/// Joins interleaved signals one after another.
pub fn concat(parts: &[Vec<f32>]) -> Vec<f32> {
    parts.concat()
}

/// Frames in `seconds` at `sample_rate`.
pub fn frames(sample_rate: u32, seconds: f64) -> usize {
    (f64::from(sample_rate) * seconds).round() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sine_peaks_at_its_level() {
        let s = sine(48_000, 1_000.0, -6.0, 0.0, 48_000);
        let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!((f64::from(peak) - amplitude(-6.0)).abs() < 1e-6);
    }

    #[test]
    fn pink_noise_is_deterministic_and_scaled() {
        let a = pink_noise(10_000, -3.0, 7);
        assert_eq!(a, pink_noise(10_000, -3.0, 7));
        assert_ne!(a, pink_noise(10_000, -3.0, 8));
        let peak = a.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!((f64::from(peak) - amplitude(-3.0)).abs() < 1e-6);
    }

    #[test]
    fn stereo_interleaves_left_then_right() {
        assert_eq!(stereo(&[1.0, 2.0], &[3.0, 4.0, 5.0]), [1.0, 3.0, 2.0, 4.0]);
        assert_eq!(both(&[1.0]), [1.0, 1.0]);
    }
}
