//! The Das-Meter Send Plugin: forwards its track's audio to the app.
//!
//! Built as one CLAP with `clack-plugin` and wrapped to VST3 and AU with
//! `clap-wrapper-rs` (ADR 0001, `docs/adr/0001-rust-stack.md`).

#[cfg(test)]
mod tests {
    #[test]
    fn crate_builds() {}
}
