# Audio files are decoded with Symphonia, allowing MPL-2.0 for it alone

The Tonal Balance Meter reads audio files: songs that teach a Target (**Add files…** and files or folders dropped on the Meter), and a bounce of the mix to compare (**Compare file…**). That needs WAV, AIFF, FLAC, MP3, Ogg Vorbis, AAC/M4A and ALAC. We decode them with **Symphonia** (pure Rust, no system libraries, the same on macOS and Windows), which is licensed **MPL-2.0**. Das-Meter is MIT OR Apache-2.0 and `deny.toml` allowed only permissive licences, so this needed a decision.

MPL-2.0 is copyleft per file: changes to Symphonia's own files must stay MPL-2.0 and their source available, but code that only uses it as a library can be under any licence, and a binary that includes it can ship under ours as long as Symphonia's source is available (it is, on crates.io and GitHub). We use it unmodified, from crates.io.

So `deny.toml` allows MPL-2.0 **for the Symphonia crates only**, listed one by one as exceptions. Any other crate bringing in MPL-2.0 is refused by CI until it has its own decision. `CLAUDE.md` says the same.

We rejected the platform decoders (Core Audio's ExtAudioFile, Windows Media Foundation): two code paths to keep alike, and their format support differs. Wrapping FFmpeg was rejected for its size and its LGPL/GPL build options.

## Consequences

- Released builds must say that they include Symphonia under MPL-2.0 and where its source is. Das-Meter has no third-party licence notice yet; adding one (for all dependencies) is a follow-up before the next release.
- If we ever change a Symphonia file, the change is published under MPL-2.0 (in practice: sent upstream).
- Patent-encumbered codecs (AAC, MP3) are decoded in software as every player does; no encoder is shipped.

Decided in [Tonal Balance Meter with loadable Targets](https://github.com/daswerk/Das-Meter/issues/105).
