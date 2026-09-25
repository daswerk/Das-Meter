//! Golden tests for the Cepstrum: harmonic tones give their pitch, noise and
//! silence give none.

use dasmeter_analysis::signals::{both, frames, pink_noise, silence};
use dasmeter_analysis::{CepstrumAnalyser, CepstrumSettings};

const RATES: [u32; 3] = [44_100, 48_000, 96_000];

/// A harmonic tone: `f0` and its partials up to Nyquist, falling 6 dB per
/// octave (a band-limited sawtooth), at about −12 dBFS.
fn harmonic(rate: u32, f0: f64, seconds: f64) -> Vec<f32> {
    let n = frames(rate, seconds);
    let partials = ((f64::from(rate) / 2.0) / f0).floor() as usize;
    (0..n)
        .map(|i| {
            let t = i as f64 / f64::from(rate);
            let x: f64 = (1..=partials)
                .map(|k| (std::f64::consts::TAU * f0 * k as f64 * t).sin() / k as f64)
                .sum();
            (0.15 * x) as f32
        })
        .collect()
}

fn analyse(rate: u32, audio: &[f32]) -> CepstrumAnalyser {
    let mut analyser = CepstrumAnalyser::new(rate, CepstrumSettings::default());
    for block in audio.chunks(2 * 512) {
        analyser.process(block);
        analyser.update();
    }
    analyser
}

#[test]
fn a_harmonic_tone_gives_its_pitch() {
    for rate in RATES {
        for f0 in [82.41, 110.0, 220.0, 440.0, 660.0] {
            let mut analyser = analyse(rate, &both(&harmonic(rate, f0, 1.0)));
            let cepstrum = analyser.update().clone();
            let pitch = cepstrum
                .pitch
                .unwrap_or_else(|| panic!("{rate} Hz, {f0} Hz: no pitch"));
            let cents = 1_200.0 * (f64::from(pitch.frequency) / f0).log2();
            assert!(
                cents.abs() < 15.0,
                "{rate} Hz: {} Hz for {f0} Hz ({cents:+.1} cents)",
                pitch.frequency
            );
            // The curve's highest point is the pitch's period.
            let top = cepstrum
                .values
                .iter()
                .enumerate()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map(|(i, _)| i)
                .unwrap();
            let (short, long) = cepstrum.quefrency_range;
            let at = short + (top as f32 + 0.5) / cepstrum.values.len() as f32 * (long - short);
            assert!(
                (at * pitch.frequency - 1.0).abs() < 0.05,
                "{rate} Hz, {f0} Hz: the curve peaks at {at} s"
            );
        }
    }
}

#[test]
fn noise_has_no_pitch() {
    for rate in RATES {
        let n = frames(rate, 1.0);
        let noise = both(&pink_noise(n, -12.0, 7));
        let mut analyser = analyse(rate, &noise);
        assert_eq!(analyser.update().pitch, None, "{rate} Hz");
    }
}

#[test]
fn silence_has_no_pitch_and_a_flat_curve() {
    let rate = 48_000;
    let mut analyser = analyse(rate, &both(&harmonic(rate, 220.0, 0.5)));
    assert!(analyser.update().pitch.is_some());
    let mut analyser_after = analyser;
    for block in both(&silence(frames(rate, 2.0))).chunks(2 * 512) {
        analyser_after.process(block);
        analyser_after.update();
    }
    let cepstrum = analyser_after.update();
    assert_eq!(cepstrum.pitch, None);
    assert!(
        cepstrum.values.iter().all(|&v| v < 0.01),
        "{:?}",
        &cepstrum.values[..8]
    );
}

#[test]
fn a_pitch_outside_the_range_isnt_reported_as_itself() {
    let rate = 48_000;
    // 1500 Hz is above the default 50–1000 Hz range.
    let mut analyser = analyse(rate, &both(&harmonic(rate, 1_500.0, 1.0)));
    if let Some(pitch) = analyser.update().pitch {
        assert!(pitch.frequency <= 1_000.5, "{}", pitch.frequency);
    }
}
