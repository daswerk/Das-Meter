//! The Das-Meter Send Plugin: forwards its track's audio to the app.
//!
//! Built as one CLAP with `clack-plugin` and wrapped to VST3 and AU with
//! `clap-wrapper-rs` (ADR 0001, `docs/adr/0001-rust-stack.md`).
//!
//! - [`identity`]: the ID, name and colour, and the saved state.
//! - [`link`]: the main thread's slot in the transport, heartbeat and status.
//! - [`send`]: the audio thread's path into the transport.
//! - [`plugin`]: the CLAP glue.
//! - [`window`]: the plugin window.

mod animals;
pub mod identity;
pub mod link;
pub mod plugin;
mod reaper;
pub mod send;
pub mod window;

// VST3 (`GetPluginFactory`) and, on macOS, AUv2 (`GetPluginFactoryAUV2`) entry
// points that wrap the CLAP entry above (ADR 0001).
clap_wrapper::export_vst3!();
clap_wrapper::export_auv2!();
