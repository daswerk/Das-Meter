//! The Spectrogram: one column per span / COLUMNS of audio, each the
//! Spectrum's calibrated levels at log-spaced frequencies, scaled to 0–255
//! over the dB range.

use std::time::Duration;

use dasmeter_analysis::signals::{both, concat, frames, silence, sine};
use dasmeter_analysis::spectrogram::{COLUMNS, ROWS};
use dasmeter_analysis::{SpectrogramAnalyser, SpectrogramSettings};

const RATE: u32 = 48_000;

fn settings() -> SpectrogramSettings {
    SpectrogramSettings {
        slope: 0.0,
        span: Duration::from_secs(4),
        ..SpectrogramSettings::default()
    }
}

#[test]
fn a_column_every_span_over_columns_and_no_more_than_columns_kept() {
    let mut a = SpectrogramAnalyser::new(RATE, settings());
    // 4 s / 256 columns = 750 frames a column; fed in odd-sized blocks.
    let audio = both(&sine(RATE, 1_000.0, -6.0, 0.0, 750 * 10 + 10));
    for block in audio.chunks(2 * 333) {
        a.process(block);
    }
    assert_eq!(a.completed(), 10);
    assert_eq!(a.columns().count(), 10);
    assert!(a.columns().all(|c| c.len() == ROWS));

    a.process(&both(&silence(frames(RATE, 5.0))));
    assert_eq!(a.columns().count(), COLUMNS);
}

#[test]
fn a_sine_lights_the_row_at_its_frequency_and_silence_is_dark() {
    let mut a = SpectrogramAnalyser::new(RATE, settings());
    let audio = concat(&[
        both(&sine(RATE, 1_000.0, -6.0, 0.0, frames(RATE, 0.5))),
        both(&silence(frames(RATE, 0.5))),
    ]);
    a.process(&audio);
    let columns: Vec<&Vec<u8>> = a.columns().collect();
    let frequencies = a.frequencies().to_vec();
    assert_eq!(frequencies.len(), ROWS);
    assert!(frequencies.windows(2).all(|w| w[0] < w[1]), "lowest first");

    // Well inside the tone: the brightest row is the one nearest 1 kHz, at
    // about −6 dB on a −90..0 scale (0.93 of 255).
    let tone = columns[columns.len() / 4];
    let (row, &level) = tone.iter().enumerate().max_by_key(|(_, l)| **l).unwrap();
    let nearest = frequencies
        .iter()
        .enumerate()
        .min_by(|a, b| {
            (a.1 / 1_000.0)
                .ln()
                .abs()
                .total_cmp(&(b.1 / 1_000.0).ln().abs())
        })
        .unwrap()
        .0;
    assert!(
        row.abs_diff(nearest) <= 1,
        "row {row}, 1 kHz is row {nearest}"
    );
    let expected = (84.0 / 90.0 * 255.0) as i32;
    assert!((i32::from(level) - expected).abs() <= 5, "level {level}");

    // Once the FFT holds only silence, every row is at the floor.
    let last = columns.last().unwrap();
    assert!(last.iter().all(|&l| l == 0));
}
