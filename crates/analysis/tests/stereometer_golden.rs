//! Golden tests for the Stereometer at every supported sample rate, with the
//! tolerances from ADR 0005 (correlation ±0.02).

use std::time::Duration;

use dasmeter_analysis::signals::{both, frames, silence, sine, stereo, white_noise};
use dasmeter_analysis::{StereoScaling, StereoView, StereometerAnalyser, StereometerSettings};

const RATES: [u32; 4] = [44_100, 48_000, 96_000, 192_000];
const CORRELATION_TOLERANCE: f32 = 0.02;

fn analyse(rate: u32, audio: &[f32]) -> StereometerAnalyser {
    let mut analyser = StereometerAnalyser::new(rate, StereometerSettings::default());
    for block in audio.chunks(1024) {
        analyser.process(block);
    }
    analyser
}

fn tone(rate: u32, dbfs: f64) -> Vec<f32> {
    sine(rate, 440.0, dbfs, 0.0, frames(rate, 2.0))
}

#[track_caller]
fn assert_near(what: &str, rate: u32, got: f32, want: f32, tolerance: f32) {
    assert!(
        (got - want).abs() <= tolerance,
        "{what} at {rate} Hz: got {got:.4}, want {want} ±{tolerance}"
    );
}

#[test]
fn identical_channels_read_plus_one() {
    for rate in RATES {
        let readings = analyse(rate, &both(&tone(rate, -6.0))).readings();
        assert_near(
            "correlation",
            rate,
            readings.correlation,
            1.0,
            CORRELATION_TOLERANCE,
        );
        assert!(!readings.no_signal);
        assert_near("balance", rate, readings.balance, 0.0, 0.01);
    }
}

#[test]
fn inverted_channels_read_minus_one() {
    for rate in RATES {
        let left = tone(rate, -6.0);
        let right: Vec<f32> = left.iter().map(|x| -x).collect();
        let readings = analyse(rate, &stereo(&left, &right)).readings();
        assert_near(
            "correlation",
            rate,
            readings.correlation,
            -1.0,
            CORRELATION_TOLERANCE,
        );
    }
}

#[test]
fn independent_noise_reads_about_zero() {
    for rate in RATES {
        let n = frames(rate, 2.0);
        let audio = stereo(&white_noise(n, -6.0, 11), &white_noise(n, -6.0, 12));
        let readings = analyse(rate, &audio).readings();
        assert_near(
            "correlation",
            rate,
            readings.correlation,
            0.0,
            CORRELATION_TOLERANCE,
        );
    }
}

#[test]
fn silence_gives_the_no_signal_flag() {
    for rate in RATES {
        let quiet = both(&silence(frames(rate, 1.0)));
        let readings = analyse(rate, &quiet).readings();
        assert!(readings.no_signal);
        assert_eq!(readings.correlation, 0.0);
        assert_eq!(readings.balance, 0.0);

        // Below −90 dBFS counts as silence too.
        let readings = analyse(rate, &both(&tone(rate, -100.0))).readings();
        assert!(readings.no_signal, "−100 dBFS at {rate} Hz");
        assert_eq!(readings.correlation, 0.0);
    }
}

#[test]
fn a_one_sided_signal_reads_zero() {
    for rate in RATES {
        let left = tone(rate, -6.0);
        let readings = analyse(rate, &stereo(&left, &silence(left.len()))).readings();
        assert_eq!(readings.correlation, 0.0);
        assert!(!readings.no_signal);
        assert_near("balance", rate, readings.balance, -1.0, 0.01);
    }
}

#[test]
fn balance_follows_the_louder_channel() {
    let rate = 48_000;
    let audio = stereo(&tone(rate, -12.0), &tone(rate, -6.0));
    let balance = analyse(rate, &audio).readings().balance;
    // Right is twice the amplitude: (2 − 1) / (2 + 1).
    assert_near("balance", rate, balance, 1.0 / 3.0, 0.01);
}

#[test]
fn correlation_follows_its_time_constant() {
    let rate = 48_000;
    let settings = StereometerSettings {
        correlation_time: Duration::from_millis(50),
        ..Default::default()
    };
    let mut fast = StereometerAnalyser::new(rate, settings);
    let mut slow = StereometerAnalyser::new(rate, StereometerSettings::default());
    let mono = both(&tone(rate, -6.0));
    let left = tone(rate, -6.0);
    let inverted = stereo(&left, &left.iter().map(|x| -x).collect::<Vec<_>>());
    for analyser in [&mut fast, &mut slow] {
        analyser.process(&mono);
        analyser.process(&inverted[..frames(rate, 0.1) * 2]);
    }
    // 100 ms after flipping phase, the 50 ms average has turned; the 300 ms one hasn't yet.
    assert!(
        fast.readings().correlation < -0.5,
        "fast: {}",
        fast.readings().correlation
    );
    assert!(
        slow.readings().correlation > -0.5,
        "slow: {}",
        slow.readings().correlation
    );
}

#[test]
fn the_mono_flag_passes_through() {
    let mut analyser = StereometerAnalyser::new(48_000, StereometerSettings::default());
    assert!(!analyser.readings().mono);
    analyser.set_mono(true);
    assert!(analyser.readings().mono);
    analyser.set_sample_rate(96_000);
    assert!(analyser.readings().mono, "kept across a sample-rate change");
}

#[test]
fn points_place_mono_up_and_left_left() {
    let rate = 48_000;
    let mut out = Vec::new();

    // Mono: straight up in both views.
    let mono = analyse(rate, &both(&tone(rate, -6.0)));
    mono.points(StereoView::Lissajous, &mut out);
    assert_eq!(out.len(), 2_048);
    assert!(out.iter().all(|[x, _]| x.abs() < 1e-6));

    // Left only: up and to the left in Polar.
    let n = frames(rate, 1.0);
    let left_only = analyse(rate, &stereo(&tone(rate, -6.0)[..n], &silence(n)));
    left_only.points(StereoView::Polar, &mut out);
    assert!(out.iter().all(|&[x, y]| y >= 0.0 && x <= 1e-6));
    assert!(out.iter().any(|&[x, y]| x < -0.5 && y > 0.5));

    // Out of phase: flat along the base line in Polar.
    let left = tone(rate, -6.0);
    let inverted = analyse(
        rate,
        &stereo(&left, &left.iter().map(|x| -x).collect::<Vec<_>>()),
    );
    inverted.points(StereoView::Polar, &mut out);
    assert!(out.iter().all(|[_, y]| y.abs() < 1e-6));
}

#[test]
fn auto_gain_fills_the_scope_and_fixed_gain_does_not() {
    let rate = 48_000;
    let quiet = both(&tone(rate, -30.0));
    let mut out = Vec::new();
    let reach = |out: &Vec<[f32; 2]>| out.iter().map(|[x, y]| x.hypot(*y)).fold(0.0f32, f32::max);

    analyse(rate, &quiet).points(StereoView::Lissajous, &mut out);
    assert!(
        (reach(&out) - 1.0).abs() < 0.02,
        "auto-gain reach {}",
        reach(&out)
    );

    let fixed = StereometerSettings {
        scaling: StereoScaling::Fixed { gain: 1.0 },
        ..Default::default()
    };
    let mut analyser = StereometerAnalyser::new(rate, fixed);
    analyser.process(&quiet);
    analyser.points(StereoView::Lissajous, &mut out);
    // A −30 dBFS mono sine: mid peaks at √2 · 0.0316.
    assert!(
        (reach(&out) - 0.0447).abs() < 0.001,
        "fixed reach {}",
        reach(&out)
    );
}
