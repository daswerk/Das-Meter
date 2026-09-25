//! Golden tests for the Loudness Meter at every supported sample rate, with the
//! tolerances from ADR 0005. Signal choices follow EBU Tech 3341/3342 where a
//! test case fits: the EBU files themselves are never committed.

use std::time::Duration;

use dasmeter_analysis::signals::{both, concat, frames, silence, sine, stereo};
use dasmeter_analysis::{LoudnessAnalyser, LoudnessSettings, PeakHold, RmsMode};

const RATES: [u32; 4] = [44_100, 48_000, 96_000, 192_000];

const LUFS_TOLERANCE: f64 = 0.1;
const LRA_TOLERANCE: f64 = 1.0;
const SAMPLE_PEAK_TOLERANCE: f64 = 0.01;
const RMS_TOLERANCE: f64 = 0.05;

#[track_caller]
fn assert_near(what: &str, rate: u32, got: f64, want: f64, tolerance: f64) {
    assert!(
        (got - want).abs() <= tolerance,
        "{what} at {rate} Hz: got {got:.4}, want {want} ±{tolerance}"
    );
}

fn analyse(rate: u32, settings: LoudnessSettings, audio: &[f32]) -> LoudnessAnalyser {
    let mut analyser = LoudnessAnalyser::new(rate, settings);
    // Feed in DAW-sized blocks, as the app will.
    for block in audio.chunks(2 * 512) {
        analyser.process(block);
    }
    analyser
}

/// A stereo 1 kHz sine with the same level on both channels.
fn stereo_sine(rate: u32, dbfs: f64, seconds: f64) -> Vec<f32> {
    both(&sine(rate, 1_000.0, dbfs, 0.0, frames(rate, seconds)))
}

#[test]
fn a_full_scale_sine_reads_0_db_rms_aes17_and_0_dbfs_peak() {
    for rate in RATES {
        for dbfs in [0.0, -20.0] {
            let audio = stereo_sine(rate, dbfs, 1.0);
            let aes17 = analyse(rate, LoudnessSettings::default(), &audio).readings();
            let plain = analyse(
                rate,
                LoudnessSettings {
                    rms_mode: RmsMode::Plain,
                    ..Default::default()
                },
                &audio,
            )
            .readings();
            for levels in [aes17.left, aes17.right] {
                assert_near("RMS (AES17)", rate, levels.rms, dbfs, RMS_TOLERANCE);
                assert_near(
                    "sample peak",
                    rate,
                    levels.peak,
                    dbfs,
                    SAMPLE_PEAK_TOLERANCE,
                );
                assert_near(
                    "peak max",
                    rate,
                    levels.peak_max,
                    dbfs,
                    SAMPLE_PEAK_TOLERANCE,
                );
            }
            assert_near(
                "plain RMS",
                rate,
                plain.left.rms,
                dbfs - 3.0103,
                RMS_TOLERANCE,
            );
        }
    }
}

#[test]
fn channels_are_measured_independently() {
    for rate in RATES {
        let n = frames(rate, 1.0);
        let audio = stereo(&sine(rate, 1_000.0, -6.0, 0.0, n), &silence(n));
        let readings = analyse(rate, LoudnessSettings::default(), &audio).readings();
        assert_near("left RMS", rate, readings.left.rms, -6.0, RMS_TOLERANCE);
        assert_eq!(readings.right.rms, f64::NEG_INFINITY);
        assert_eq!(readings.right.peak, f64::NEG_INFINITY);
    }
}

#[test]
fn a_stereo_sine_reads_its_lufs() {
    // EBU Tech 3341 cases 1 and 2: a stereo 1 kHz sine at −23 / −33 dBFS reads −23 / −33 LUFS.
    for rate in RATES {
        for level in [-23.0, -33.0] {
            let readings = analyse(
                rate,
                LoudnessSettings::default(),
                &stereo_sine(rate, level, 10.0),
            )
            .readings();
            assert_near("momentary", rate, readings.momentary, level, LUFS_TOLERANCE);
            assert_near(
                "short-term",
                rate,
                readings.short_term,
                level,
                LUFS_TOLERANCE,
            );
            assert_near(
                "integrated",
                rate,
                readings.integrated,
                level,
                LUFS_TOLERANCE,
            );
        }
    }
}

#[test]
fn two_levels_give_their_loudness_range() {
    // EBU Tech 3342 cases 1 and 2: 20 s at −20 dBFS, then 20 s at −30 / −15 dBFS.
    for rate in RATES {
        for (second, want) in [(-30.0, 10.0), (-15.0, 5.0)] {
            let audio = concat(&[
                stereo_sine(rate, -20.0, 20.0),
                stereo_sine(rate, second, 20.0),
            ]);
            let readings = analyse(rate, LoudnessSettings::default(), &audio).readings();
            assert_near("LRA", rate, readings.range, want, LRA_TOLERANCE);
        }
    }
}

#[test]
fn true_peak_finds_the_peak_between_samples() {
    // A sine at a quarter of the sample rate with a 45° phase: every sample lands
    // 3.01 dB below the real peak (EBU Tech 3341 case 15, at −6 dBFS).
    for rate in RATES {
        let n = frames(rate, 1.0);
        let quarter = f64::from(rate) / 4.0;
        let audio = both(&sine(rate, quarter, -6.0, std::f64::consts::FRAC_PI_4, n));
        let readings = analyse(rate, LoudnessSettings::default(), &audio).readings();
        assert_near(
            "sample peak",
            rate,
            readings.left.peak_max,
            -9.0103,
            SAMPLE_PEAK_TOLERANCE,
        );
        if rate < 192_000 {
            assert!(readings.true_peak_oversampled);
            let tp = readings.true_peak_max;
            assert!(
                (-6.4..=-5.8).contains(&tp),
                "true peak at {rate} Hz: got {tp:.3}, want −6 +0.2/−0.4"
            );
        } else {
            // Not oversampled: the "TP" label becomes "Peak".
            assert!(!readings.true_peak_oversampled);
        }
    }
}

#[test]
fn reset_clears_integrated_range_and_maxima() {
    for rate in RATES {
        let mut analyser = analyse(
            rate,
            LoudnessSettings::default(),
            &stereo_sine(rate, -20.0, 5.0),
        );
        assert_near(
            "integrated before reset",
            rate,
            analyser.readings().integrated,
            -20.0,
            LUFS_TOLERANCE,
        );

        analyser.reset();
        let cleared = analyser.readings();
        assert_eq!(cleared.integrated, f64::NEG_INFINITY);
        assert_eq!(cleared.true_peak_max, f64::NEG_INFINITY);
        assert_eq!(cleared.left.peak_max, f64::NEG_INFINITY);

        for block in stereo_sine(rate, -30.0, 5.0).chunks(1024) {
            analyser.process(block);
        }
        let after = analyser.readings();
        assert_near(
            "integrated after reset",
            rate,
            after.integrated,
            -30.0,
            LUFS_TOLERANCE,
        );
        assert_near(
            "peak max after reset",
            rate,
            after.left.peak_max,
            -30.0,
            SAMPLE_PEAK_TOLERANCE,
        );
        assert!(
            after.true_peak_max < -29.0,
            "true peak after reset at {rate} Hz: {}",
            after.true_peak_max
        );
    }
}

#[test]
fn a_sample_rate_change_restarts_the_analyser() {
    let mut analyser = analyse(
        48_000,
        LoudnessSettings::default(),
        &stereo_sine(48_000, -20.0, 5.0),
    );
    analyser.set_sample_rate(96_000);
    assert_eq!(analyser.sample_rate(), 96_000);
    let restarted = analyser.readings();
    assert_eq!(restarted.integrated, f64::NEG_INFINITY);
    assert_eq!(restarted.left.peak_hold, f64::NEG_INFINITY);
    assert_eq!(restarted.left.peak_max, f64::NEG_INFINITY);

    for block in stereo_sine(96_000, -30.0, 5.0).chunks(1024) {
        analyser.process(block);
    }
    let after = analyser.readings();
    assert_near(
        "integrated",
        96_000,
        after.integrated,
        -30.0,
        LUFS_TOLERANCE,
    );
    assert_near("RMS", 96_000, after.left.rms, -30.0, RMS_TOLERANCE);

    analyser.set_sample_rate(192_000);
    assert!(!analyser.readings().true_peak_oversampled);
}

#[test]
fn peak_hold_keeps_the_peak_for_its_hold_time() {
    for rate in RATES {
        let burst = stereo_sine(rate, -6.0, 0.5);
        let quiet = |seconds| both(&silence(frames(rate, seconds)));

        let settings = LoudnessSettings {
            peak_hold: PeakHold::For(Duration::from_secs(2)),
            ..Default::default()
        };
        let mut analyser = analyse(rate, settings, &burst);
        analyser.process(&quiet(1.5));
        let held = analyser.readings().left;
        assert_eq!(
            held.peak,
            f64::NEG_INFINITY,
            "window peak falls after the window"
        );
        assert_near(
            "held peak",
            rate,
            held.peak_hold,
            -6.0,
            SAMPLE_PEAK_TOLERANCE,
        );
        analyser.process(&quiet(1.0));
        let released = analyser.readings().left;
        assert_eq!(
            released.peak_hold,
            f64::NEG_INFINITY,
            "hold released after 2 s"
        );
        assert_near(
            "peak max",
            rate,
            released.peak_max,
            -6.0,
            SAMPLE_PEAK_TOLERANCE,
        );

        let infinite = LoudnessSettings {
            peak_hold: PeakHold::Infinite,
            ..Default::default()
        };
        let mut analyser = analyse(rate, infinite, &burst);
        analyser.process(&quiet(10.0));
        assert_near(
            "infinite hold",
            rate,
            analyser.readings().left.peak_hold,
            -6.0,
            SAMPLE_PEAK_TOLERANCE,
        );
    }
}

#[test]
fn the_rms_window_is_configurable() {
    let rate = 48_000;
    let settings = LoudnessSettings {
        rms_window: Duration::from_millis(50),
        ..Default::default()
    };
    let mut analyser = analyse(rate, settings, &stereo_sine(rate, -12.0, 0.5));
    assert_near(
        "RMS",
        rate,
        analyser.readings().left.rms,
        -12.0,
        RMS_TOLERANCE,
    );
    // 60 ms of silence empties a 50 ms window but not the default 300 ms one.
    analyser.process(&both(&silence(frames(rate, 0.06))));
    assert_eq!(analyser.readings().left.rms, f64::NEG_INFINITY);
}

#[test]
fn the_history_has_a_point_every_100_ms() {
    for rate in RATES {
        let analyser = analyse(
            rate,
            LoudnessSettings::default(),
            &stereo_sine(rate, -23.0, 5.0),
        );
        let history: Vec<(f64, f64)> = analyser.history().collect();
        // 512-frame blocks round each step up to the next block.
        assert!(
            (48..=50).contains(&history.len()),
            "{rate} Hz: {}",
            history.len()
        );
        let (momentary, short_term) = *history.last().unwrap();
        assert_near("history momentary", rate, momentary, -23.0, LUFS_TOLERANCE);
        assert_near(
            "history short-term",
            rate,
            short_term,
            -23.0,
            LUFS_TOLERANCE,
        );
    }
}

#[test]
fn the_history_keeps_two_minutes_and_a_reset_clears_it() {
    let rate = 48_000;
    let mut analyser = analyse(
        rate,
        LoudnessSettings::default(),
        &stereo_sine(rate, -23.0, 125.0),
    );
    assert_eq!(
        analyser.history().len(),
        dasmeter_analysis::loudness::HISTORY_STEPS
    );
    analyser.reset();
    assert_eq!(analyser.history().len(), 0);
}

#[test]
fn a_sine_has_no_headroom_between_peak_and_loudness() {
    // A stereo sine at X dBFS reads X LUFS with a true peak of X dBTP: PLR and PSR ≈ 0.
    for rate in [44_100, 48_000] {
        let readings = analyse(
            rate,
            LoudnessSettings::default(),
            &stereo_sine(rate, -18.0, 10.0),
        )
        .readings();
        assert_near("PLR", rate, readings.plr, 0.0, 0.3);
        assert_near("PSR", rate, readings.psr, 0.0, 0.3);
    }
}

#[test]
fn a_quiet_signal_with_loud_clicks_has_a_high_plr() {
    let rate = 48_000;
    // A −30 dBFS sine with a −6 dBFS click every second.
    let mut audio = stereo_sine(rate, -30.0, 10.0);
    for second in 0..10 {
        let at = 2 * (second * rate as usize + 100);
        audio[at] = 0.5;
        audio[at + 1] = 0.5;
    }
    let readings = analyse(rate, LoudnessSettings::default(), &audio).readings();
    assert!(readings.plr > 20.0, "PLR {}", readings.plr);
    assert!(readings.psr > 20.0, "PSR {}", readings.psr);
}

#[test]
fn silence_has_no_ratios() {
    let rate = 48_000;
    let readings = analyse(
        rate,
        LoudnessSettings::default(),
        &both(&silence(frames(rate, 5.0))),
    )
    .readings();
    assert_eq!(readings.plr, f64::NEG_INFINITY);
    assert_eq!(readings.psr, f64::NEG_INFINITY);
}
