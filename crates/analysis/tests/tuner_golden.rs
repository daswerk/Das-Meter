//! The Tuner (#103): notes and cents of harmonic tones across the range,
//! the reference pitch, and holding the last reading.

use std::f64::consts::TAU;

use dasmeter_analysis::{CepstrumSettings, TunerAnalyser, TunerReading};

const RATE: u32 = 48_000;

fn settings() -> CepstrumSettings {
    CepstrumSettings {
        fft_size: 8_192,
        pitch_range: (30.0, 1_000.0),
        ..CepstrumSettings::default()
    }
}

/// A tone at `hz` with eight harmonics falling off, `seconds` long, stereo.
fn tone(hz: f64, seconds: f64) -> Vec<f32> {
    let frames = (seconds * f64::from(RATE)) as usize;
    (0..frames)
        .flat_map(|i| {
            let t = i as f64 / f64::from(RATE);
            let x: f64 = (1..=8)
                .map(|k| 0.3 / f64::from(k) * (TAU * hz * f64::from(k) * t).sin())
                .sum();
            [x as f32, x as f32]
        })
        .collect()
}

/// Plays `audio` in 20 ms blocks, updating after each; the last reading.
fn play(tuner: &mut TunerAnalyser, audio: &[f32], a4: f32) -> Option<TunerReading> {
    let mut reading = None;
    for block in audio.chunks(2 * RATE as usize / 50) {
        tuner.process(block);
        reading = tuner.update(a4);
    }
    reading
}

fn cents_from(hz: f64, cents: f64) -> f64 {
    hz * 2f64.powf(cents / 1_200.0)
}

#[test]
fn notes_and_cents_across_the_range() {
    for (hz, name, octave) in [
        (41.203, "E", 1),
        (55.0, "A", 1),
        (110.0, "A", 2),
        (196.0, "G", 3),
        (440.0, "A", 4),
        (880.0, "A", 5),
    ] {
        for off in [-20.0, 0.0, 7.0] {
            let mut tuner = TunerAnalyser::new(RATE, settings());
            let reading = play(&mut tuner, &tone(cents_from(hz, off), 1.5), 440.0)
                .unwrap_or_else(|| panic!("no pitch at {hz} Hz {off:+} ¢"));
            assert_eq!(
                (reading.note.name, reading.note.octave),
                (name, octave),
                "{hz} Hz {off:+} ¢"
            );
            assert!(
                (f64::from(reading.note.cents) - off).abs() < 2.0,
                "{hz} Hz {off:+} ¢ read {:+.1} ¢",
                reading.note.cents
            );
            assert!(!reading.held);
        }
    }
}

#[test]
fn the_reference_pitch_moves_the_notes() {
    let mut tuner = TunerAnalyser::new(RATE, settings());
    let at_440 = play(&mut tuner, &tone(442.0, 1.5), 440.0).unwrap();
    assert!(
        (at_440.note.cents - 7.85).abs() < 2.0,
        "{}",
        at_440.note.cents
    );
    let at_442 = play(&mut tuner, &tone(442.0, 0.5), 442.0).unwrap();
    assert_eq!(at_442.note.name, "A");
    assert!(at_442.note.cents.abs() < 2.0, "{}", at_442.note.cents);
}

#[test]
fn the_last_reading_is_held_for_a_second_then_dropped() {
    let mut tuner = TunerAnalyser::new(RATE, settings());
    play(&mut tuner, &tone(110.0, 1.0), 440.0).unwrap();
    // Half a second of silence (stereo samples), then a second more.
    let half = vec![0.0f32; RATE as usize];
    let held = play(&mut tuner, &half, 440.0).expect("held");
    assert!(held.held);
    assert_eq!(held.note.name, "A");
    let second = vec![0.0f32; 2 * RATE as usize];
    assert_eq!(play(&mut tuner, &second, 440.0), None);
}

#[test]
fn a_new_note_is_read_at_once() {
    let mut tuner = TunerAnalyser::new(RATE, settings());
    play(&mut tuner, &tone(110.0, 1.0), 440.0).unwrap();
    // A quarter second of the next note: a smoothed reading would still
    // be on its way.
    let reading = play(&mut tuner, &tone(146.83, 0.25), 440.0).unwrap();
    assert_eq!(reading.note.name, "D");
    assert!(reading.note.cents.abs() < 5.0, "{}", reading.note.cents);
}
