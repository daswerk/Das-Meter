# Das-Meter

Live Meters for what your computer plays: a Waveform, a Spectrum, a Loudness
Meter and a Stereometer, on macOS and Windows. Fed by the system's output, or
by a Send Plugin in your DAW.

The Loudness Meter measures to ITU-R BS.1770-5 and EBU Tech 3341/3342, checked
against the EBU test set ([what that means](https://daswerk.github.io/Das-Meter/measurements.html)).

**Docs:** <https://daswerk.github.io/Das-Meter/>

## Download

Get the latest release from [GitHub Releases](https://github.com/daswerk/Das-Meter/releases/latest).
On macOS, open the DMG and drag Das-Meter to Applications.

**macOS asks once:** the first time you open Das-Meter, macOS says it can't
check it. Open **System Settings ▸ Privacy & Security**, scroll down and click
**Open Anyway**. Das-Meter is open source and signed with its own certificate,
not an Apple Developer ID yet ([why](https://daswerk.github.io/Das-Meter/getting-started.html#open-anyway)).

## Building from source

You need a recent stable [Rust](https://rustup.rs) toolchain (1.95 or newer for the app).

```sh
cargo build --workspace   # build everything
cargo test --workspace    # run the tests
cargo run --bin das-meter # run the app
```

On macOS, `scripts/macos-bundle.sh` builds `Das-Meter.app` (System Capture
only works from the app bundle). The docs site is in `docs/site`
(`mdbook serve docs/site`).

| Crate | What it is |
|---|---|
| `crates/transport` | The shared-memory table that carries audio from Send Plugins to the app |
| `crates/analysis` | Meter analysis: audio frames in, Meter readings out |
| `crates/core` | The headless app core: all app behaviour, driven by events |
| `crates/app` | The app: platform shell and Meter renderers |
| `crates/send-plugin` | The Send Plugin (CLAP, wrapped to VST3 and AU) |

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option. Contributions are accepted under the same terms.
