//! The Loudness Meter's counters (#102): clips (sample peaks at or over
//! 0 dBFS) and true peaks over a ceiling, per channel, each run counted once.

use std::f64::consts::{FRAC_PI_4, TAU};

use dasmeter_analysis::signals::{both, sine, stereo};
use dasmeter_analysis::{LoudnessAnalyser, LoudnessSettings};

const RATE: u32 = 48_000;

fn analyse(audio: &[f32], block_frames: usize, settings: LoudnessSettings) -> LoudnessAnalyser {
    let mut analyser = LoudnessAnalyser::new(RATE, settings);
    for block in audio.chunks(block_frames * 2) {
        analyser.process(block);
    }
    analyser
}

/// A quiet tone with `bursts` short runs of full-scale samples, `gap` seconds apart.
fn with_clips(bursts: usize, gap: f64) -> Vec<f32> {
    let frames = (bursts as f64 * gap + 0.5) as usize * RATE as usize + RATE as usize;
    let mut mono = sine(RATE, 440.0, -20.0, 0.0, frames);
    for b in 0..bursts {
        let start = ((b as f64 + 0.5) * gap * f64::from(RATE)) as usize;
        for s in &mut mono[start..start + 5] {
            *s = 1.0;
        }
    }
    mono
}

#[test]
fn each_run_of_clipped_samples_counts_once_on_its_channel() {
    let left = with_clips(3, 0.5);
    let right = vec![0.0; left.len()];
    let readings = analyse(&stereo(&left, &right), 1024, LoudnessSettings::default()).readings();
    assert_eq!(readings.left.clips, 3);
    assert_eq!(readings.right.clips, 0);
}

#[test]
fn a_loud_tone_is_one_true_peak_over_and_a_quiet_one_none() {
    let loud = both(&sine(RATE, 1_000.0, -0.5, 0.0, RATE as usize * 2));
    let readings = analyse(&loud, 512, LoudnessSettings::default()).readings();
    assert_eq!(readings.left.true_peak_overs, 1, "one run, however long");
    assert_eq!(readings.left.clips, 0);

    let quiet = both(&sine(RATE, 1_000.0, -2.0, 0.0, RATE as usize * 2));
    let readings = analyse(&quiet, 512, LoudnessSettings::default()).readings();
    assert_eq!(readings.left.true_peak_overs, 0);

    // The ceiling decides: at 0 dBTP the loud tone isn't over.
    let at_zero = LoudnessSettings {
        ceiling: 0.0,
        ..LoudnessSettings::default()
    };
    assert_eq!(
        analyse(&loud, 512, at_zero).readings().left.true_peak_overs,
        0
    );
}

#[test]
fn a_peak_between_samples_is_a_true_peak_over_but_not_a_clip() {
    // At a quarter of the rate, 45° off: every sample is at 0.707 of the
    // peak, so 1.2 peaks at 0.85 in the samples and +1.6 dBTP between them.
    let frames = RATE as usize;
    let mono: Vec<f32> = (0..frames)
        .map(|i| (1.2 * (TAU * 0.25 * i as f64 + FRAC_PI_4).sin()) as f32)
        .collect();
    let readings = analyse(&both(&mono), 1024, LoudnessSettings::default()).readings();
    assert_eq!(readings.left.clips, 0);
    assert_eq!(readings.left.true_peak_overs, 1);
}

#[test]
fn overs_are_counted_alike_whatever_the_block_size() {
    // Three short bursts well over the ceiling, half a second apart.
    let mut mono = sine(RATE, 1_000.0, -30.0, 0.0, RATE as usize * 2);
    let loud = sine(RATE, 1_000.0, -0.2, 0.0, 2_400);
    for b in 0..3 {
        let start = (b * RATE as usize) / 2 + 4_800;
        mono[start..start + loud.len()].copy_from_slice(&loud);
    }
    let audio = both(&mono);
    for block in [64, 441, 1024, 4096] {
        let readings = analyse(&audio, block, LoudnessSettings::default()).readings();
        assert_eq!(readings.left.true_peak_overs, 3, "blocks of {block}");
        assert_eq!(readings.right.true_peak_overs, 3, "blocks of {block}");
    }
}

#[test]
fn reset_clears_the_counters() {
    let left = with_clips(2, 0.5);
    let mut analyser = analyse(&both(&left), 1024, LoudnessSettings::default());
    assert_eq!(analyser.readings().left.clips, 2);
    analyser.reset();
    let readings = analyser.readings();
    assert_eq!((readings.left.clips, readings.left.true_peak_overs), (0, 0));
    analyser.process(&both(&with_clips(1, 0.5)));
    assert_eq!(analyser.readings().left.clips, 1);
}
