# Rust stack: clack Send Plugin wrapped to VST3/AU, winit + wgpu app

Das-Meter is written in Rust as one Cargo workspace, with the transport between the Send Plugin and the app in a shared crate. We chose Rust over C++/JUCE because JUCE draws on the CPU on macOS and has no System Capture support, while Rust has crates for every capture API, runs Meters on the GPU through wgpu, and allows a permissive licence. We decided without a benchmark first.

- **Send Plugin**: built as a single CLAP with `clack-plugin` (thin bindings, no framework). nih-plug was rejected: its upstream is in maintenance mode, its fork couldn't be checked, and a plugin with no Meters and almost no parameters gains little from it. If the plugin ever needs a text field, it gets a tiny window of its own (e.g. baseview + egui).
- **VST3 and AUv2**: wrapped from that one CLAP with `clap-wrapper-rs`, so Cargo stays the only build tool. Fallback if the young crate breaks: the C++ clap-wrapper through CMake. No AUv3.
- **App**: winit + wgpu directly. Each Meter is its own GPU renderer (a shader fed a small buffer), and the app redraws only when audio arrives; egui draws only menus and the settings panel onto the same surface. eframe was rejected because it relayouts every frame and hides the window handle that the Windows AppBar API needs; iced because it calls itself experimental.
- **Maths**: `rustfft`/`realfft` for the Spectrum and `ebur128` for LUFS, rather than hand-written maths.

Research behind this: [`docs/research/rust-vs-juce.md`](../research/rust-vs-juce.md).
