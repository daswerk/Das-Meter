//! The Needle Meter (#106): its needles, the overload lamp and the dial.

use dasmeter_analysis::signals::{both, sine, stereo};
use dasmeter_analysis::{NeedleAnalyser, Scale};

const RATE: u32 = 48_000;

#[test]
fn a_sine_at_the_reference_reads_0_vu_on_both_needles() {
    let mut needle = NeedleAnalyser::new(RATE, Scale::Vu, -18.0, false);
    needle.process(&both(&sine(RATE, 1_000.0, -18.0, 0.0, RATE as usize * 2)));
    let readings = needle.readings();
    assert_eq!(readings.len(), 2);
    for r in readings {
        assert!((r.level - -18.0).abs() < 0.2, "{}", r.level);
        assert!(!r.over);
    }
}

#[test]
fn each_needle_follows_its_channel_or_the_mono_sum() {
    let left = sine(RATE, 1_000.0, -18.0, 0.0, RATE as usize * 2);
    let right = vec![0.0; left.len()];
    let mut stereo_needles = NeedleAnalyser::new(RATE, Scale::PpmEbu, -18.0, false);
    stereo_needles.process(&stereo(&left, &right));
    let r = stereo_needles.readings();
    assert!((r[0].level - -18.0).abs() < 0.3);
    assert_eq!(r[1].level, f64::NEG_INFINITY);
    let mut mono = NeedleAnalyser::new(RATE, Scale::PpmEbu, -18.0, true);
    mono.process(&stereo(&left, &right));
    let r = mono.readings();
    assert_eq!(r.len(), 1);
    // Half of one channel: 6 dB down.
    assert!((r[0].level - -24.0).abs() < 0.3, "{}", r[0].level);
}

#[test]
fn the_lamp_lights_at_the_scales_end_and_goes_out_after_a_second() {
    let mut needle = NeedleAnalyser::new(RATE, Scale::Vu, -18.0, true);
    // −10 dBFS is +8 VU: past +3.
    needle.process(&both(&sine(RATE, 1_000.0, -10.0, 0.0, RATE as usize)));
    assert!(needle.readings()[0].over);
    let quiet = both(&sine(RATE, 1_000.0, -40.0, 0.0, RATE as usize / 2));
    needle.process(&quiet);
    assert!(needle.readings()[0].over, "still lit half a second on");
    needle.process(&quiet);
    needle.process(&quiet);
    assert!(!needle.readings()[0].over);
}

#[test]
fn a_vu_dial_is_spaced_by_voltage_and_a_ppm_dial_by_db() {
    let vu = |db| Scale::Vu.dial_position(db, -18.0);
    assert_eq!(vu(-38.0), 0.0);
    assert!((vu(-15.0) - 1.0).abs() < 1e-9);
    // 0 VU about two thirds across, −10 VU much nearer the start than the middle.
    assert!((vu(-18.0) - 0.686).abs() < 0.01, "{}", vu(-18.0));
    assert!(vu(-28.0) < 0.2, "{}", vu(-28.0));
    let ebu = |db| Scale::PpmEbu.dial_position(db, -18.0);
    assert!((ebu(-18.0) - 0.5).abs() < 1e-9);
    assert_eq!(Scale::Vu.red_from(-18.0), -18.0);
    assert_eq!(Scale::PpmBbc.red_from(-18.0), -9.0);
}
