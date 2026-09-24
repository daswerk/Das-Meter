# Das-Meter

A standalone, open-source audio visualizer for music producers on macOS and Windows: Waveform, Spectrum, Loudness Meter and Stereometer, fed by what your computer plays or by a Send Plugin in your DAW.

Das-Meter is in early development. The v1 spec is [issue #25](https://github.com/daswerk/Das-Meter/issues/25).

## Building from source

You need a recent stable [Rust](https://rustup.rs) toolchain (1.85 or newer).

```sh
cargo build --workspace   # build everything
cargo test --workspace    # run the tests
cargo run --bin das-meter # run the app
```

The workspace has one crate per module:

| Crate | What it is |
|---|---|
| `crates/transport` | The shared-memory table that carries audio from Send Plugins to the app |
| `crates/analysis` | Meter analysis: audio frames in, Meter readings out |
| `crates/core` | The headless app core: all app behaviour, driven by events |
| `crates/app` | The app: platform shell and Meter renderers |
| `crates/send-plugin` | The Send Plugin (CLAP, wrapped to VST3 and AU) |

`crates/analysis` and `crates/core` must stay free of platform, GPU and audio-device code, so they can be tested anywhere. `scripts/check-headless-deps.sh` checks this in CI, and `cargo deny check` checks dependency licences.

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option. Contributions are accepted under the same terms.
