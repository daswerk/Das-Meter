//! The Loudness Meter's scales (#104): each ballistic checked against its
//! definition (ADR 0007), and the scales' marks.

use std::f64::consts::TAU;

use dasmeter_analysis::scales::{DIN_ZERO, PPM_ALIGNMENT};
use dasmeter_analysis::{Ballistics, LoudnessAnalyser, LoudnessSettings, Scale};

const RATE: u32 = 48_000;

fn sine_sample(hz: f64, peak: f64, i: usize) -> f32 {
    (peak * (TAU * hz * i as f64 / f64::from(RATE)).sin()) as f32
}

fn db(linear: f64) -> f64 {
    20.0 * linear.log10()
}

/// The highest reading while a 5 kHz burst of `ms` plays (and just after),
/// against the steady tone's, in dB.
fn burst_reading(scale: Scale, ms: f64) -> f64 {
    let n = (ms / 1_000.0 * f64::from(RATE)) as usize;
    let mut meter = Ballistics::new(scale, RATE);
    let mut highest = f64::NEG_INFINITY;
    for i in 0..n + RATE as usize / 10 {
        let x = if i < n {
            sine_sample(5_000.0, 0.5, i)
        } else {
            0.0
        };
        meter.push(x);
        highest = highest.max(meter.db());
    }
    highest - db(0.5)
}

#[test]
fn ppm_type_i_reads_80_percent_of_a_5_ms_burst() {
    for scale in [Scale::PpmDin, Scale::PpmNordic] {
        let reading = burst_reading(scale, 5.0);
        assert!(
            (reading - db(0.8)).abs() < 0.3,
            "{scale:?}: {reading:.2} dB"
        );
    }
}

#[test]
fn ppm_type_ii_reads_80_percent_of_a_10_ms_burst() {
    for scale in [Scale::PpmBbc, Scale::PpmEbu] {
        let reading = burst_reading(scale, 10.0);
        assert!(
            (reading - db(0.8)).abs() < 0.3,
            "{scale:?}: {reading:.2} dB"
        );
    }
}

#[test]
fn a_steady_sine_reads_its_peak_on_every_ballistic_scale() {
    for scale in [Scale::Vu, Scale::PpmDin, Scale::PpmBbc] {
        let mut meter = Ballistics::new(scale, RATE);
        for i in 0..RATE as usize * 3 {
            meter.push(sine_sample(1_000.0, 0.25, i));
        }
        let reading = meter.db();
        assert!((reading - db(0.25)).abs() < 0.3, "{scale:?}: {reading:.2}");
    }
}

/// How long after the tone stops the reading falls `by` dB.
fn fall_time(scale: Scale, by: f64) -> f64 {
    let mut meter = Ballistics::new(scale, RATE);
    for i in 0..RATE as usize {
        meter.push(sine_sample(1_000.0, 0.5, i));
    }
    let start = meter.db();
    for i in 0..RATE as usize * 5 {
        meter.push(0.0);
        if meter.db() <= start - by {
            return i as f64 / f64::from(RATE);
        }
    }
    f64::INFINITY
}

#[test]
fn ppms_fall_back_at_their_rates() {
    for (scale, by, seconds) in [
        (Scale::PpmDin, 20.0, 1.7),
        (Scale::PpmNordic, 20.0, 1.7),
        (Scale::PpmBbc, 24.0, 2.8),
        (Scale::PpmEbu, 24.0, 2.8),
    ] {
        let t = fall_time(scale, by);
        assert!((t - seconds).abs() < 0.05, "{scale:?}: {t:.3} s");
    }
}

#[test]
fn vu_reaches_99_percent_in_300_ms_and_overshoots_by_1_to_1_5_percent() {
    // A step of a 1 kHz tone: the reading in linear terms against the steady one.
    let mut meter = Ballistics::new(Scale::Vu, RATE);
    let steady = 0.5;
    let mut reached = None;
    let mut highest = 0.0f64;
    for i in 0..RATE as usize * 2 {
        meter.push(sine_sample(1_000.0, steady, i));
        let level = 10f64.powf(meter.db() / 20.0) / steady;
        if reached.is_none() && level >= 0.99 {
            reached = Some(i as f64 / f64::from(RATE));
        }
        highest = highest.max(level);
    }
    let reached = reached.expect("reaches 99%");
    assert!((reached - 0.3).abs() < 0.015, "99% at {reached:.3} s");
    let overshoot = (highest - 1.0) * 100.0;
    assert!(
        (0.9..=1.6).contains(&overshoot),
        "overshoot {overshoot:.2}%"
    );
}

#[test]
fn the_analyser_reports_the_scale_level_only_for_ballistic_scales() {
    let audio: Vec<f32> = (0..RATE as usize * 2)
        .flat_map(|i| {
            let x = sine_sample(1_000.0, 0.125, i);
            [x, x]
        })
        .collect();
    for (scale, ballistic) in [
        (Scale::Peak, false),
        (Scale::K14, false),
        (Scale::PpmEbu, true),
    ] {
        let mut analyser = LoudnessAnalyser::new(
            RATE,
            LoudnessSettings {
                scale,
                ..LoudnessSettings::default()
            },
        );
        analyser.process(&audio);
        let level = analyser.readings().left.scale_level;
        if ballistic {
            assert!((level - db(0.125)).abs() < 0.3, "{scale:?}: {level}");
        } else {
            assert_eq!(level, f64::NEG_INFINITY, "{scale:?}");
        }
    }
}

#[test]
fn the_scales_put_their_marks_where_the_standards_do() {
    let mark = |scale: Scale, label: &str| {
        scale
            .marks(-18.0)
            .into_iter()
            .find(|(_, l)| l == label)
            .map(|(at, _)| at)
    };
    assert_eq!(mark(Scale::Vu, "0"), Some(-18.0));
    assert_eq!(mark(Scale::PpmDin, "0"), Some(DIN_ZERO));
    assert_eq!(mark(Scale::PpmNordic, "0"), Some(PPM_ALIGNMENT));
    assert_eq!(mark(Scale::PpmBbc, "4"), Some(PPM_ALIGNMENT));
    assert_eq!(mark(Scale::PpmBbc, "6"), Some(PPM_ALIGNMENT + 8.0));
    assert_eq!(mark(Scale::PpmEbu, "+8"), Some(PPM_ALIGNMENT + 8.0));
    assert_eq!(mark(Scale::K20, "0"), Some(-20.0));
    assert_eq!(mark(Scale::K20, "+20"), Some(0.0));
    assert_eq!(Scale::K14.range(-18.0), Some((-44.0, 0.0)));
}
