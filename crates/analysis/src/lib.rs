//! Meter analysis: stereo frames and a sample rate in, Meter readings out.
//!
//! Pure maths only. This crate must not depend on platform, GPU or audio-device
//! code (checked by `scripts/check-headless-deps.sh`). Measurement definitions
//! are in ADR 0005 (`docs/adr/0005-measurement-definitions-and-verification.md`).

pub mod cepstrum;
mod channel_view;
pub mod loudness;
pub mod seconds;
pub mod signals;
pub mod spectrum;
pub mod stereometer;
pub mod test_signal;
pub mod waveform;

pub use cepstrum::{Cepstrum, CepstrumAnalyser, CepstrumSettings, Pitch};
pub use channel_view::ChannelView;

pub use loudness::{
    ChannelLevels, LoudnessAnalyser, LoudnessReadings, LoudnessSettings, PeakHold, RmsMode,
};
pub use spectrum::{
    Note, Spectrum, SpectrumAnalyser, SpectrumSettings, SpectrumStyle, SpectrumTrace,
    WindowFunction, note_name,
};
pub use stereometer::{
    StereoReadings, StereoScaling, StereoView, StereometerAnalyser, StereometerSettings,
};
pub use waveform::{WaveformAnalyser, WaveformColumn, WaveformScale, WaveformSettings};
