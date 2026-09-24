//! The headless app core: all app behaviour, driven by events.
//!
//! It takes events in (audio, Send Plugins, displays, user actions) and puts a
//! scene description and persistence writes out, so it can be tested without
//! windows, a GPU or audio devices. This crate must not depend on platform, GPU
//! or audio-device code (enforced in `deny.toml`).

#[cfg(test)]
mod tests {
    #[test]
    fn crate_builds() {}
}
