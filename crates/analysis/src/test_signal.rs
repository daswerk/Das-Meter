//! The benchmark's test signal: about a minute of pink noise, a sine sweep,
//! panned and out-of-phase noise, and clicks, generated the same way every time.

use std::f64::consts::TAU;

use crate::signals::{amplitude, frames, pink_noise, stereo};

/// Peak levels of the sections, in dBFS.
pub const NOISE_DBFS: f64 = -12.0;
pub const SWEEP_DBFS: f64 = -12.0;
pub const CLICK_DBFS: f64 = -6.0;
/// Seconds between clicks.
pub const CLICK_EVERY: f64 = 0.5;

/// What a stretch of the signal is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionKind {
    /// Pink noise, independent on each channel (wide stereo).
    Pink,
    /// A logarithmic sine sweep from 20 Hz to 20 kHz, the same on both channels.
    Sweep,
    /// Pink noise on the left channel only.
    Left,
    /// Pink noise on the right channel only.
    Right,
    /// The same pink noise on both channels (mono in the middle).
    Centre,
    /// Pink noise with the right channel inverted (correlation −1).
    OutOfPhase,
    /// Single-sample clicks every half second in silence, for latency.
    Clicks,
}

/// Where a section starts and ends, in frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section {
    pub kind: SectionKind,
    pub start: usize,
    pub end: usize,
}

/// The signal, with where its sections and clicks are.
#[derive(Clone, Debug, PartialEq)]
pub struct TestSignal {
    pub sample_rate: u32,
    /// Interleaved stereo frames.
    pub frames: Vec<f32>,
    pub sections: Vec<Section>,
    /// The frame of each click.
    pub clicks: Vec<usize>,
}

/// The sections and their lengths in seconds: 60 s in all.
const LAYOUT: [(SectionKind, f64); 7] = [
    (SectionKind::Pink, 12.0),
    (SectionKind::Sweep, 12.0),
    (SectionKind::Left, 6.0),
    (SectionKind::Right, 6.0),
    (SectionKind::Centre, 6.0),
    (SectionKind::OutOfPhase, 6.0),
    (SectionKind::Clicks, 12.0),
];

fn sweep(sample_rate: u32, n: usize) -> Vec<f32> {
    let (low, high) = (20.0f64, 20_000.0f64);
    let seconds = n as f64 / f64::from(sample_rate);
    let rate = (high / low).ln() / seconds;
    let gain = amplitude(SWEEP_DBFS);
    (0..n)
        .map(|i| {
            let t = i as f64 / f64::from(sample_rate);
            // The phase of an exponential sweep: the integral of its frequency.
            let phase = TAU * low * ((rate * t).exp() - 1.0) / rate;
            (gain * phase.sin()) as f32
        })
        .collect()
}

impl TestSignal {
    /// The signal at `sample_rate`.
    pub fn new(sample_rate: u32) -> TestSignal {
        let mut out = Vec::new();
        let mut sections = Vec::new();
        let mut clicks = Vec::new();
        for (index, (kind, seconds)) in LAYOUT.into_iter().enumerate() {
            let n = frames(sample_rate, seconds);
            let start = out.len() / 2;
            let seed = 1 + index as u64 * 2;
            let noise = || pink_noise(n, NOISE_DBFS, seed);
            let silent = vec![0.0f32; n];
            let part = match kind {
                SectionKind::Pink => stereo(&noise(), &pink_noise(n, NOISE_DBFS, seed + 1)),
                SectionKind::Sweep => {
                    let s = sweep(sample_rate, n);
                    stereo(&s, &s)
                }
                SectionKind::Left => stereo(&noise(), &silent),
                SectionKind::Right => stereo(&silent, &noise()),
                SectionKind::Centre => {
                    let s = noise();
                    stereo(&s, &s)
                }
                SectionKind::OutOfPhase => {
                    let s = noise();
                    let inverted: Vec<f32> = s.iter().map(|x| -x).collect();
                    stereo(&s, &inverted)
                }
                SectionKind::Clicks => {
                    let mut s = silent.clone();
                    let every = frames(sample_rate, CLICK_EVERY);
                    let click = amplitude(CLICK_DBFS) as f32;
                    // The first a quarter-interval in, so none sits on the edge.
                    let mut at = every / 4;
                    while at < n {
                        s[at] = click;
                        clicks.push(start + at);
                        at += every;
                    }
                    stereo(&s, &s)
                }
            };
            out.extend(part);
            sections.push(Section {
                kind,
                start,
                end: out.len() / 2,
            });
        }
        TestSignal {
            sample_rate,
            frames: out,
            sections,
            clicks,
        }
    }

    /// The interleaved frames of a section.
    pub fn section(&self, kind: SectionKind) -> &[f32] {
        let s = self
            .sections
            .iter()
            .find(|s| s.kind == kind)
            .expect("every kind has a section");
        &self.frames[2 * s.start..2 * s.end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn peak_db(channel: impl Iterator<Item = f32>) -> f64 {
        20.0 * f64::from(channel.fold(0.0f32, |m, x| m.max(x.abs()))).log10()
    }

    fn correlation(frames: &[f32]) -> f64 {
        let (mut lr, mut ll, mut rr) = (0.0f64, 0.0f64, 0.0f64);
        for f in frames.chunks(2) {
            let (l, r) = (f64::from(f[0]), f64::from(f[1]));
            lr += l * r;
            ll += l * l;
            rr += r * r;
        }
        if ll == 0.0 || rr == 0.0 {
            0.0
        } else {
            lr / (ll * rr).sqrt()
        }
    }

    fn left(frames: &[f32]) -> impl Iterator<Item = f32> + '_ {
        frames.iter().step_by(2).copied()
    }

    fn right(frames: &[f32]) -> impl Iterator<Item = f32> + '_ {
        frames.iter().skip(1).step_by(2).copied()
    }

    #[test]
    fn it_is_the_same_every_time() {
        assert_eq!(TestSignal::new(RATE), TestSignal::new(RATE));
    }

    #[test]
    fn it_lasts_a_minute_with_every_section() {
        let signal = TestSignal::new(RATE);
        assert_eq!(signal.frames.len() / 2, 60 * RATE as usize);
        assert_eq!(signal.sections.len(), 7);
        assert_eq!(signal.sections.last().unwrap().end, 60 * RATE as usize);
    }

    #[test]
    fn the_sections_have_their_levels() {
        let signal = TestSignal::new(RATE);
        let near = |a: f64, b: f64| (a - b).abs() < 0.01;
        let pink = signal.section(SectionKind::Pink);
        assert!(near(peak_db(left(pink)), NOISE_DBFS) && near(peak_db(right(pink)), NOISE_DBFS));
        assert!(near(
            peak_db(left(signal.section(SectionKind::Sweep))),
            SWEEP_DBFS
        ));
        let left_only = signal.section(SectionKind::Left);
        assert!(near(peak_db(left(left_only)), NOISE_DBFS));
        assert!(right(left_only).all(|x| x == 0.0));
        let right_only = signal.section(SectionKind::Right);
        assert!(left(right_only).all(|x| x == 0.0));
        let clicks = signal.section(SectionKind::Clicks);
        assert!(near(peak_db(left(clicks)), CLICK_DBFS));
    }

    #[test]
    fn the_sections_have_their_correlation() {
        let signal = TestSignal::new(RATE);
        let c = |kind| correlation(signal.section(kind));
        assert!(c(SectionKind::Pink).abs() < 0.05, "independent channels");
        assert!((c(SectionKind::Sweep) - 1.0).abs() < 1e-6);
        assert!((c(SectionKind::Centre) - 1.0).abs() < 1e-6);
        assert!((c(SectionKind::OutOfPhase) + 1.0).abs() < 1e-6);
        assert_eq!(c(SectionKind::Left), 0.0, "one channel silent");
    }

    #[test]
    fn clicks_come_every_half_second_in_silence() {
        let signal = TestSignal::new(RATE);
        assert_eq!(signal.clicks.len(), 24);
        for pair in signal.clicks.windows(2) {
            assert_eq!(pair[1] - pair[0], RATE as usize / 2);
        }
        let clicks = signal.section(SectionKind::Clicks);
        let loud = left(clicks).filter(|x| *x != 0.0).count();
        assert_eq!(loud, 24, "only the clicks themselves");
    }
}
