//! Meter analysis: stereo frames and a sample rate in, Meter readings out.
//!
//! Pure maths only. This crate must not depend on platform, GPU or audio-device
//! code (checked by `scripts/check-headless-deps.sh`). Measurement definitions
//! are in ADR 0005 (`docs/adr/0005-measurement-definitions-and-verification.md`).

pub mod cepstrum;
mod channel_view;
pub mod loudness;
pub mod needle;
pub mod oscilloscope;
pub mod scales;
pub mod seconds;
pub mod signals;
pub mod spectrogram;
pub mod spectrum;
pub mod stereo_bands;
pub mod stereometer;
pub mod test_signal;
pub mod tonal_balance;
pub mod tuner;
pub mod waveform;

pub use cepstrum::{Cepstrum, CepstrumAnalyser, CepstrumSettings, Pitch};
pub use channel_view::ChannelView;

pub use loudness::{
    ChannelLevels, LoudnessAnalyser, LoudnessReadings, LoudnessSettings, PeakHold, RmsMode,
};
pub use needle::{NeedleAnalyser, NeedleReading};
pub use oscilloscope::{Oscilloscope, OscilloscopeAnalyser, OscilloscopeSettings};
pub use scales::{Ballistics, Scale};
pub use spectrogram::{SpectrogramAnalyser, SpectrogramSettings};
pub use spectrum::{
    LongTerm, Note, REFERENCE_POINTS_PER_OCTAVE, REFERENCE_RANGE, Spectrum, SpectrumAnalyser,
    SpectrumSettings, SpectrumStyle, SpectrumTrace, WindowFunction, catmull_rom, collisions,
    note_name, note_name_at, reference_at, reference_grid,
};
pub use stereo_bands::{BandReading, StereoBandsAnalyser};
pub use stereometer::{
    StereoReadings, StereoScaling, StereoView, StereometerAnalyser, StereometerSettings,
};
pub use tuner::{TunerAnalyser, TunerReading};
pub use waveform::{Lr4, WaveformAnalyser, WaveformColumn, WaveformScale, WaveformSettings};
