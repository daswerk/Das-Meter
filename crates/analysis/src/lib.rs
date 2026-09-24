//! Meter analysis: stereo frames and a sample rate in, Meter readings out.
//!
//! Pure maths only. This crate must not depend on platform, GPU or audio-device
//! code (enforced in `deny.toml`). Measurement definitions are in ADR 0005
//! (`docs/adr/0005-measurement-definitions-and-verification.md`).

#[cfg(test)]
mod tests {
    #[test]
    fn crate_builds() {}
}
