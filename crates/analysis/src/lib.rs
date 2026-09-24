//! Meter analysis: stereo frames and a sample rate in, Meter readings out.
//!
//! Pure maths only. This crate must not depend on platform, GPU or audio-device
//! code (checked by `scripts/check-headless-deps.sh`). Measurement definitions
//! are in ADR 0005 (`docs/adr/0005-measurement-definitions-and-verification.md`).

pub mod loudness;
pub mod signals;

pub use loudness::{
    ChannelLevels, LoudnessAnalyser, LoudnessReadings, LoudnessSettings, PeakHold, RmsMode,
};
