//! Golden tests for the Spectrum at every supported sample rate, with the
//! tolerances from ADR 0005.

use std::time::Duration;

use dasmeter_analysis::loudness::PeakHold;
use dasmeter_analysis::signals::{both, silence, sine, stereo};
use dasmeter_analysis::{
    ChannelView, SpectrumAnalyser, SpectrumSettings, SpectrumStyle, WindowFunction, note_name,
};

const RATES: [u32; 4] = [44_100, 48_000, 96_000, 192_000];
const FFT: usize = 8_192;
/// Hann's worst-case scalloping loss, half a bin off.
const HANN_SCALLOP_DB: f32 = 1.42;

fn flat() -> SpectrumSettings {
    SpectrumSettings {
        slope: 0.0,
        ..Default::default()
    }
}

/// The frequency of the bin nearest to `near` Hz.
fn on_bin(rate: u32, near: f64) -> f64 {
    let bin_hz = f64::from(rate) / FFT as f64;
    (near / bin_hz).round() * bin_hz
}

fn analyse(rate: u32, settings: SpectrumSettings, audio: &[f32]) -> SpectrumAnalyser {
    let mut analyser = SpectrumAnalyser::new(rate, settings);
    analyser.process(audio);
    analyser.update();
    analyser
}

fn max(values: &[f32]) -> f32 {
    values.iter().copied().fold(f32::MIN, f32::max)
}

#[track_caller]
fn assert_near(what: &str, rate: u32, got: f32, want: f32, tolerance: f32) {
    assert!(
        (got - want).abs() <= tolerance,
        "{what} at {rate} Hz: got {got:.3}, want {want:.3} ±{tolerance}"
    );
}

#[test]
fn a_sine_on_a_bin_reads_its_level() {
    for rate in RATES {
        for window in [
            WindowFunction::Hann,
            WindowFunction::BlackmanHarris,
            WindowFunction::Rectangular,
        ] {
            for dbfs in [0.0, -20.0] {
                let f = on_bin(rate, 1_000.0);
                let audio = both(&sine(rate, f, dbfs, 0.3, FFT * 2));
                let analyser = analyse(rate, SpectrumSettings { window, ..flat() }, &audio);
                let what = format!("{window:?} {dbfs} dBFS bin level");
                assert_near(&what, rate, max(analyser.bin_levels(0)), dbfs as f32, 0.1);
                let drawn = max(&analyser.spectrum().traces[0].levels);
                assert_near(
                    &format!("{window:?} {dbfs} dBFS drawn level"),
                    rate,
                    drawn,
                    dbfs as f32,
                    0.1,
                );
            }
        }
    }
}

#[test]
fn a_sine_half_a_bin_off_stays_within_the_scalloping_loss() {
    for rate in RATES {
        let bin_hz = f64::from(rate) / FFT as f64;
        let f = on_bin(rate, 1_000.0) + bin_hz / 2.0;
        let audio = both(&sine(rate, f, 0.0, 0.0, FFT * 2));
        let analyser = analyse(rate, flat(), &audio);
        let level = max(analyser.bin_levels(0));
        assert!(
            (-HANN_SCALLOP_DB - 0.01..=0.0).contains(&level),
            "half a bin off at {rate} Hz: {level:.3} dB"
        );
        let found = f64::from(analyser.peak_frequency(0));
        assert!(
            (found - f).abs() <= bin_hz / 2.0 + 1e-3,
            "peak {found} Hz for {f} Hz at {rate} Hz"
        );
    }
}

#[test]
fn the_peak_frequency_is_within_half_a_bin() {
    for rate in RATES {
        let bin_hz = f64::from(rate) / FFT as f64;
        for f in [50.0, 440.0, 3_210.0, 15_000.0] {
            let audio = both(&sine(rate, f, -6.0, 0.0, FFT * 2));
            let found = f64::from(analyse(rate, flat(), &audio).peak_frequency(0));
            assert!(
                (found - f).abs() <= bin_hz / 2.0,
                "peak {found} Hz for {f} Hz at {rate} Hz"
            );
        }
    }
}

#[test]
fn the_slope_pivots_at_1_khz() {
    for rate in RATES {
        for slope in [0.0f32, 3.0, 4.5, 6.0] {
            for octaves in [-3i32, -1, 0, 1, 3] {
                let f = on_bin(rate, 1_000.0 * 2f64.powi(octaves));
                let audio = both(&sine(rate, f, -12.0, 0.0, FFT * 2));
                let settings = SpectrumSettings { slope, ..flat() };
                let got = max(analyse(rate, settings, &audio).bin_levels(0));
                let want = -12.0 + slope * (f / 1_000.0).log2() as f32;
                assert_near(&format!("slope {slope} at {f:.1} Hz"), rate, got, want, 0.1);
            }
        }
    }
}

#[test]
fn channel_views_split_the_stereo_signal() {
    let rate = 48_000;
    let f = on_bin(rate, 1_000.0);
    let tone = sine(rate, f, 0.0, 0.0, FFT * 2);
    let quiet = silence(FFT * 2);

    // M+S of a mono signal: full mid, silent side.
    let settings = SpectrumSettings {
        channel_view: ChannelView::MidSide,
        ..flat()
    };
    let mid_side = analyse(rate, settings, &both(&tone));
    assert_near("mid", rate, max(mid_side.bin_levels(0)), 0.0, 0.1);
    assert!(
        max(mid_side.bin_levels(1)) < -150.0,
        "side of a mono signal"
    );
    assert_eq!(mid_side.spectrum().traces.len(), 2);

    // L+R with the tone on the left only.
    let settings = SpectrumSettings {
        channel_view: ChannelView::LeftRight,
        ..flat()
    };
    let left_right = analyse(rate, settings, &stereo(&tone, &quiet));
    assert_near("left", rate, max(left_right.bin_levels(0)), 0.0, 0.1);
    assert!(max(left_right.bin_levels(1)) < -150.0, "silent right");

    // Mono sum of a left-only tone is half the amplitude.
    let mono = analyse(rate, flat(), &stereo(&tone, &quiet));
    assert_eq!(mono.spectrum().traces.len(), 1);
    assert_near("mono sum", rate, max(mono.bin_levels(0)), -6.02, 0.1);
}

#[test]
fn bars_aggregate_fractional_octave_bands() {
    for rate in RATES {
        for (bands_per_octave, count) in [(3, 29), (6, 59), (12, 119)] {
            let settings = SpectrumSettings {
                style: SpectrumStyle::Bars { bands_per_octave },
                ..flat()
            };
            let audio = both(&sine(rate, on_bin(rate, 1_000.0), 0.0, 0.0, FFT * 2));
            let analyser = analyse(rate, settings, &audio);
            let spectrum = analyser.spectrum();
            assert_eq!(
                spectrum.frequencies.len(),
                count,
                "1/{bands_per_octave} octave bands at {rate} Hz"
            );
            let at_1k = spectrum
                .frequencies
                .iter()
                .position(|&f| (f - 1_000.0).abs() < 0.01)
                .expect("a band centred on 1 kHz");
            let levels = &spectrum.traces[0].levels;
            assert_near("1 kHz band", rate, levels[at_1k], 0.0, 0.1);
            let far = levels[at_1k + bands_per_octave as usize];
            assert!(
                far < -40.0,
                "band an octave up reads {far:.1} dB at {rate} Hz"
            );
        }
    }
}

#[test]
fn frequency_smoothing_spreads_a_sine() {
    let rate = 48_000;
    let audio = both(&sine(rate, 1_000.0, 0.0, 0.0, FFT * 2));
    let sharp = analyse(rate, flat(), &audio);
    let smoothed = analyse(
        rate,
        SpectrumSettings {
            smoothing_octaves: 1.0 / 3.0,
            ..flat()
        },
        &audio,
    );
    let width = |a: &SpectrumAnalyser| {
        a.spectrum().traces[0]
            .levels
            .iter()
            .filter(|&&l| l > -30.0)
            .count()
    };
    assert!(
        width(&smoothed) > width(&sharp),
        "smoothing widens the peak"
    );
}

#[test]
fn release_and_peak_hold_follow_audio_time() {
    let rate = 48_000;
    let settings = SpectrumSettings {
        release: Duration::from_millis(100),
        peak_hold: PeakHold::For(Duration::from_secs(1)),
        ..flat()
    };
    let mut analyser = SpectrumAnalyser::new(rate, settings);
    analyser.process(&both(&sine(rate, on_bin(rate, 1_000.0), 0.0, 0.0, FFT * 2)));
    analyser.update();

    // 100 ms of silence (one time constant) after the window has emptied.
    analyser.process(&both(&silence(FFT)));
    analyser.update();
    analyser.process(&both(&silence(rate as usize / 10)));
    let spectrum = analyser.update().clone();
    let level = max(&spectrum.traces[0].levels);
    let held = max(&spectrum.traces[0].peak_hold);
    assert!(level < -20.0 && level > -200.0, "released to {level:.1} dB");
    assert!((held - 0.0).abs() < 0.2, "peak held at {held:.1} dB");

    analyser.process(&both(&silence(rate as usize * 2)));
    let spectrum = analyser.update();
    assert!(
        max(&spectrum.traces[0].peak_hold) < -20.0,
        "hold released after 1 s"
    );
}

#[test]
fn a_sample_rate_change_restarts_the_analyser() {
    let mut analyser = analyse(
        48_000,
        flat(),
        &both(&sine(48_000, 1_000.0, 0.0, 0.0, FFT * 2)),
    );
    analyser.set_sample_rate(96_000);
    assert_eq!(analyser.sample_rate(), 96_000);
    assert!(max(&analyser.update().traces[0].peak_hold) < -150.0);
}

#[test]
fn frequencies_are_named_as_notes() {
    let name = |f| note_name(f).unwrap().to_string();
    assert_eq!(name(440.0), "A4");
    assert_eq!(name(261.63), "C4");
    assert_eq!(name(27.5), "A0");
    assert_eq!(name(466.16), "A#4");
    assert_eq!(name(16.35), "C0");
    assert_eq!(name(15_804.0), "B9");
    let a = note_name(450.0).unwrap();
    assert_eq!(a.name, "A");
    assert!((a.cents - 38.9).abs() < 0.1, "{}", a.cents);
    assert!(note_name(0.0).is_none());
}

#[test]
fn the_peak_is_found_between_bins() {
    for rate in RATES {
        let bin = rate as f64 / FFT as f64;
        // Half a bin off, the worst case for a bin-only answer.
        let frequency = on_bin(rate, 1_000.0) + bin / 2.0;
        let audio = both(&sine(rate, frequency, -12.0, 0.0, FFT * 2));
        let mut analyser = analyse(rate, flat(), &audio);
        analyser.update();
        let (found, level) = analyser.peak(-100.0).expect("a peak");
        let error = (f64::from(found) - frequency).abs();
        assert!(
            error < bin * 0.1,
            "{rate} Hz: {found} for {frequency} (bin {bin})"
        );
        // The parabola also takes back most of the scalloping loss.
        assert!((level + 12.0).abs() < 0.5, "{rate} Hz: {level} dB");
    }
}

#[test]
fn the_louder_of_two_tones_is_the_peak() {
    let rate = 48_000;
    let quiet = sine(rate, 440.0, -30.0, 0.0, FFT * 2);
    let loud = sine(rate, 3_000.0, -12.0, 0.0, FFT * 2);
    let mixed: Vec<f32> = quiet.iter().zip(&loud).map(|(a, b)| a + b).collect();
    let mut analyser = analyse(rate, flat(), &both(&mixed));
    analyser.update();
    let (found, _) = analyser.peak(-100.0).unwrap();
    assert!((found - 3_000.0).abs() < 2.0, "{found}");
}

#[test]
fn silence_has_no_peak() {
    let rate = 48_000;
    let mut analyser = analyse(rate, flat(), &both(&silence(FFT * 2)));
    analyser.update();
    assert_eq!(analyser.peak(-100.0), None);
}
