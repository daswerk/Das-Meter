//! The Spectrum's long-term average and Reference curves (#101).

use dasmeter_analysis::signals::{both, silence, sine};
use dasmeter_analysis::{
    LongTerm, SpectrumAnalyser, SpectrumSettings, reference_at, reference_grid,
};

const RATE: u32 = 48_000;
const BLOCK: usize = 1_024;
/// On an FFT bin at the default 8192 points, so no scalloping.
const TONE: f64 = 48_000.0 / 8_192.0 * 171.0;

fn settings(long_term: LongTerm) -> SpectrumSettings {
    SpectrumSettings {
        slope: 0.0,
        long_term,
        ..Default::default()
    }
}

/// Feeds `audio` (stereo) in blocks, updating after each like the app does.
fn feed(analyser: &mut SpectrumAnalyser, audio: &[f32]) {
    for block in audio.chunks(BLOCK * 2) {
        analyser.process(block);
        analyser.update();
    }
}

/// The Reference curve's loudest level within a semitone of `hz`.
fn curve_at(curve: &[f32], hz: f32) -> f32 {
    let semitone = 2f32.powf(1.0 / 12.0);
    reference_grid()
        .iter()
        .zip(curve)
        .filter(|(f, _)| (hz / semitone..=hz * semitone).contains(*f))
        .map(|(_, &level)| level)
        .fold(f32::MIN, f32::max)
}

#[test]
fn the_reference_grid_is_a_24th_octave_from_20_hz_to_20_khz() {
    let grid = reference_grid();
    assert_eq!(grid.first(), Some(&20.0));
    assert!((grid.last().unwrap() - 20_000.0).abs() < 300.0);
    let step = grid[1] / grid[0];
    assert!((step - 2f32.powf(1.0 / 24.0)).abs() < 1e-5);
    assert_eq!(grid.len(), 240);
}

#[test]
fn off_draws_no_long_term_average_and_has_no_curve() {
    let mut analyser = SpectrumAnalyser::new(RATE, settings(LongTerm::Off));
    feed(
        &mut analyser,
        &both(&sine(RATE, TONE, -20.0, 0.0, RATE as usize)),
    );
    assert!(analyser.spectrum().traces[0].long_term.is_empty());
    assert_eq!(analyser.long_term_curve(), None);
}

#[test]
fn a_steady_sine_reads_its_level_in_the_curve() {
    let mut analyser = SpectrumAnalyser::new(RATE, settings(LongTerm::Whole));
    feed(
        &mut analyser,
        &both(&sine(RATE, TONE, -20.0, 0.0, 2 * RATE as usize)),
    );
    let curve = analyser.long_term_curve().expect("it heard a sine");
    assert_eq!(curve.len(), reference_grid().len());
    // Each grid point takes its loudest bin, as the Spectrum's line does.
    let at = curve_at(&curve, 1_000.0);
    assert!((at + 20.0).abs() < 0.1, "1 kHz read {at:.2} dB");
    assert!(curve_at(&curve, 100.0) < -60.0);
    assert!(curve_at(&curve, 10_000.0) < -60.0);
}

#[test]
fn silence_leaves_the_average_as_it_was() {
    let mut analyser = SpectrumAnalyser::new(RATE, settings(LongTerm::Whole));
    feed(
        &mut analyser,
        &both(&sine(RATE, TONE, -20.0, 0.0, RATE as usize)),
    );
    // The FFT window still holds the tone's end for a moment: let it pass.
    feed(&mut analyser, &silence(RATE as usize * 2));
    let before = analyser.long_term_curve().unwrap();
    let seconds = analyser.long_term_seconds();
    feed(&mut analyser, &silence(5 * RATE as usize * 2));
    assert_eq!(analyser.long_term_curve().unwrap(), before);
    assert_eq!(analyser.long_term_seconds(), seconds);
}

#[test]
fn whole_weighs_every_moment_the_same() {
    // 2 s at −20 dB, then 2 s at −40 dB of the same tone: the power average
    // is (0.01 + 0.0001) / 2, about −23 dB.
    let mut analyser = SpectrumAnalyser::new(RATE, settings(LongTerm::Whole));
    feed(
        &mut analyser,
        &both(&sine(RATE, TONE, -20.0, 0.0, 2 * RATE as usize)),
    );
    let loud = curve_at(&analyser.long_term_curve().unwrap(), 1_000.0);
    feed(
        &mut analyser,
        &both(&sine(RATE, TONE, -40.0, 0.0, 2 * RATE as usize)),
    );
    let both_halves = curve_at(&analyser.long_term_curve().unwrap(), 1_000.0);
    let drop = loud - both_halves;
    assert!(
        (2.5..=3.5).contains(&drop),
        "fell {drop:.2} dB, want about 3"
    );
}

#[test]
fn a_sliding_average_forgets_the_old_audio() {
    let mut analyser = SpectrumAnalyser::new(RATE, settings(LongTerm::Seconds(3.0)));
    feed(
        &mut analyser,
        &both(&sine(RATE, TONE, -20.0, 0.0, 4 * RATE as usize)),
    );
    feed(
        &mut analyser,
        &both(&sine(RATE, TONE, -40.0, 0.0, 20 * RATE as usize)),
    );
    let at = curve_at(&analyser.long_term_curve().unwrap(), 1_000.0);
    // What's left of the loud start after 20 s, exp(−20/3) of a level
    // 20 dB up, lifts it about half a dB.
    assert!(
        (-40.1..=-39.3).contains(&at),
        "after 20 s at −40 dB read {at:.2}"
    );
}

#[test]
fn reset_starts_over() {
    let mut analyser = SpectrumAnalyser::new(RATE, settings(LongTerm::Whole));
    feed(
        &mut analyser,
        &both(&sine(RATE, TONE, -20.0, 0.0, RATE as usize)),
    );
    analyser.reset_long_term();
    assert_eq!(analyser.long_term_curve(), None);
    feed(
        &mut analyser,
        &both(&sine(RATE, TONE, -40.0, 0.0, RATE as usize)),
    );
    let at = curve_at(&analyser.long_term_curve().unwrap(), 1_000.0);
    assert!((at + 40.0).abs() < 0.1, "after a reset read {at:.2}");
}

#[test]
fn the_drawn_long_term_line_follows_the_points() {
    let mut analyser = SpectrumAnalyser::new(RATE, settings(LongTerm::Seconds(10.0)));
    feed(
        &mut analyser,
        &both(&sine(RATE, TONE, -20.0, 0.0, RATE as usize)),
    );
    let spectrum = analyser.spectrum();
    let trace = &spectrum.traces[0];
    assert_eq!(trace.long_term.len(), spectrum.frequencies.len());
    let loudest = trace.long_term.iter().copied().fold(f32::MIN, f32::max);
    assert!(
        (loudest + 20.0).abs() < 0.1,
        "the line's top read {loudest:.2}"
    );
}

#[test]
fn a_reference_is_drawn_with_the_spectrums_slope() {
    let grid = reference_grid();
    let flat = vec![-30.0; grid.len()];
    // 4.5 dB per octave around 1 kHz: +4.5 dB at 2 kHz, −4.5 dB at 500 Hz.
    let at = reference_at(&flat, &[500.0, 1_000.0, 2_000.0], 4.5);
    assert!((at[0] + 34.5).abs() < 1e-3);
    assert!((at[1] + 30.0).abs() < 1e-3);
    assert!((at[2] + 25.5).abs() < 1e-3);
    // Straight lines between grid points on a log axis.
    let ramp: Vec<f32> = (0..grid.len()).map(|i| i as f32).collect();
    let between = (grid[10] * grid[11]).sqrt();
    let got = reference_at(&ramp, &[between], 0.0)[0];
    assert!((got - 10.5).abs() < 1e-2, "between points read {got}");
    assert!(reference_at(&[], &[1_000.0], 0.0).is_empty());
}
