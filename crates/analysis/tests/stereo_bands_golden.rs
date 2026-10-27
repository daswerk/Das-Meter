//! Stereo Bands (#100): band correlation and width per octave band.

use std::time::Duration;

use dasmeter_analysis::signals::{pink_noise, sine, stereo};
use dasmeter_analysis::{BandReading, StereoBandsAnalyser};

const RATE: u32 = 48_000;

fn analyse(left: &[f32], right: &[f32]) -> Vec<BandReading> {
    let mut analyser = StereoBandsAnalyser::new(RATE, Duration::from_millis(300));
    analyser.process(&stereo(left, right));
    analyser.readings()
}

fn band(readings: &[BandReading], centre: f32) -> BandReading {
    *readings
        .iter()
        .find(|b| b.centre == centre)
        .expect("a band")
}

const N: usize = RATE as usize * 2;

#[test]
fn a_mono_bass_under_wide_highs_reads_mono_low_and_wide_high() {
    let bass = sine(RATE, 60.0, -12.0, 0.0, N);
    let (hiss_l, hiss_r) = (pink_noise(N, -30.0, 1), pink_noise(N, -30.0, 2));
    let left: Vec<f32> = bass.iter().zip(&hiss_l).map(|(a, b)| a + b).collect();
    let right: Vec<f32> = bass.iter().zip(&hiss_r).map(|(a, b)| a + b).collect();
    let readings = analyse(&left, &right);
    let low = band(&readings, 62.5);
    assert!(low.correlation.unwrap() > 0.98, "{low:?}");
    assert!(low.width.unwrap() < 0.02, "{low:?}");
    let high = band(&readings, 4_000.0);
    assert!(high.correlation.unwrap().abs() < 0.2, "{high:?}");
    assert!((high.width.unwrap() - 0.5).abs() < 0.1, "{high:?}");
}

#[test]
fn opposite_polarity_reads_minus_one_and_full_width() {
    let tone = sine(RATE, 1_000.0, -12.0, 0.0, N);
    let flipped: Vec<f32> = tone.iter().map(|x| -x).collect();
    let b = band(&analyse(&tone, &flipped), 1_000.0);
    assert!(b.correlation.unwrap() < -0.98, "{b:?}");
    assert!(b.width.unwrap() > 0.98, "{b:?}");
}

#[test]
fn silent_bands_read_nothing_and_quiet_ones_still_read() {
    let tone = sine(RATE, 1_000.0, -60.0, 0.0, N);
    let readings = analyse(&tone, &tone);
    let at_tone = band(&readings, 1_000.0);
    assert!(at_tone.correlation.unwrap() > 0.98);
    let far = band(&readings, 62.5);
    assert_eq!((far.correlation, far.width), (None, None));
}

#[test]
fn a_band_hears_only_its_octave() {
    // A tone at 250 Hz: its band is loud, two octaves away is 30 dB down or more.
    let tone = sine(RATE, 250.0, -12.0, 0.0, N);
    let readings = analyse(&tone, &tone);
    // A −12 dBFS sine is −15 dB of mean square; the four filters take about
    // 1 dB each half an octave from their edges.
    let own = band(&readings, 250.0).level;
    assert!((own - -18.9).abs() < 1.0, "{own}");
    for far in [62.5, 1_000.0] {
        let level = band(&readings, far).level;
        assert!(own - level > 30.0, "{far} Hz: {level} vs {own}");
    }
}

#[test]
fn bands_over_nyquist_are_left_out() {
    let analyser = StereoBandsAnalyser::new(22_050, Duration::from_millis(300));
    let readings = analyser.readings();
    assert_eq!(readings.last().unwrap().centre, 4_000.0);
}
