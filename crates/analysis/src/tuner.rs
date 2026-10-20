//! The Tuner's analyser: the Cepstrum's pitch (#103), steadied for reading
//! as a note and how many cents it's off.
//!
//! The pitch is smoothed in cents over about [`STEADY_SECONDS`] while it stays
//! near one note, and jumps at once to a new note. When the pitch is lost
//! (silence, noise, a chord) the last reading is held, marked as held, for
//! [`HOLD_SECONDS`] of audio.

use crate::cepstrum::{CepstrumAnalyser, CepstrumSettings};
use crate::spectrum::{Note, note_name_at};

/// The smoothing's time constant.
pub const STEADY_SECONDS: f32 = 0.15;
/// How long the last reading stays after the pitch is lost.
pub const HOLD_SECONDS: f32 = 1.0;
/// A pitch this many cents from the reading is a new note, not a wobble.
const JUMP_CENTS: f32 = 50.0;

/// What the Tuner shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TunerReading {
    /// The (steadied) pitch in Hz.
    pub frequency: f32,
    /// The nearest note at the reference pitch, and the cents off it.
    pub note: Note,
    /// The pitch was lost: this is the last one, held.
    pub held: bool,
}

/// Stereo frames in, a [`TunerReading`] out.
pub struct TunerAnalyser {
    cepstrum: CepstrumAnalyser,
    /// The steadied pitch, as cents above A4 = 440 Hz.
    cents: Option<f32>,
    /// Seconds of audio since the pitch was last found.
    lost_for: f32,
    /// Seconds of audio since the last update.
    pending: f32,
}

impl TunerAnalyser {
    pub fn new(sample_rate: u32, settings: CepstrumSettings) -> TunerAnalyser {
        TunerAnalyser {
            cepstrum: CepstrumAnalyser::new(sample_rate, settings),
            cents: None,
            lost_for: 0.0,
            pending: 0.0,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.cepstrum.sample_rate()
    }

    pub fn settings(&self) -> &CepstrumSettings {
        self.cepstrum.settings()
    }

    pub fn set_settings(&mut self, settings: CepstrumSettings) {
        self.cepstrum.set_settings(settings);
    }

    pub fn process(&mut self, interleaved: &[f32]) {
        self.cepstrum.process(interleaved);
        self.pending += (interleaved.len() / 2) as f32 / self.sample_rate() as f32;
    }

    /// The reading now, with A4 at `a4` Hz; `None` when there's no pitch
    /// and nothing is held.
    pub fn update(&mut self, a4: f32) -> Option<TunerReading> {
        let elapsed = std::mem::take(&mut self.pending);
        let pitch = self.cepstrum.update().pitch;
        match pitch {
            Some(pitch) => {
                let found = 1_200.0 * (pitch.frequency / 440.0).log2();
                self.lost_for = 0.0;
                self.cents = Some(match self.cents {
                    Some(steady) if (found - steady).abs() < JUMP_CENTS => {
                        let alpha = 1.0 - (-elapsed / STEADY_SECONDS).exp();
                        steady + alpha * (found - steady)
                    }
                    _ => found,
                });
            }
            None => {
                self.lost_for += elapsed;
                if self.lost_for > HOLD_SECONDS {
                    self.cents = None;
                }
            }
        }
        let frequency = 440.0 * 2f32.powf(self.cents? / 1_200.0);
        Some(TunerReading {
            frequency,
            note: note_name_at(frequency, a4)?,
            held: pitch.is_none(),
        })
    }
}
