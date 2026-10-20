# Das-Meter

**Live audio Meters for producers: eleven Meters, two Looks, any colour you
like.** Das-Meter shows whatever your computer plays, or single tracks from
your DAW, as Waveform, Spectrum, Spectrogram, Loudness, Stereometer, Phase
Scope, Cepstrum, Oscilloscope, Tonal Balance, Tuner and Stereo Bands. Put them in a Bar along the edge of your
screen or in a window of their own, and make them look the way you want.
It's free and open source, for macOS and Linux, with Windows on the way.

![Das-Meter in the Smooth Look: Spectrum and Spectrogram on top; Waveform, Phase Scope, Loudness Meter and Stereometer below](docs/images/smooth.png)

**[Download](https://github.com/daswerk/Das-Meter/releases/latest)** ·
**[Install guide](INSTALL.md)** ·
**[Docs](https://daswerk.github.io/Das-Meter/)**

## Two Looks

Every Theme is drawn in one of two **Looks**, and switching is one click in
**Settings ▸ Theme**.

- **Smooth** (the default): floating rounded panels on a soft gradient,
  grids that fade out toward the edges, a soft glow under the traces, fills
  that fade to transparent, and Meters that dim gently in silence.
- **Classic**: the flat, plain look of earlier versions, pixel for pixel.

![The same Meters in the Classic Look, on the Dark Theme](docs/images/classic.png)

## The Meters

Eleven kinds, as many of each as you like. Right-click any Meter for its
settings.

<table>
<tr>
<td width="50%"><img src="docs/images/meter-waveform.png" alt="The Waveform"></td>
<td width="50%"><img src="docs/images/meter-spectrum.png" alt="The Spectrum, with its peak line naming A4"></td>
</tr>
<tr>
<td><b>Waveform</b>: the audio over time, coloured by low, mid and high band; mono, Left/Right or Mid/Side; 1–30 s.</td>
<td><b>Spectrum</b>: how loud each frequency is, as a line or bars, with a slope, peak hold and a peak line that names the loudest note.</td>
</tr>
<tr>
<td><img src="docs/images/meter-spectrogram.png" alt="The Spectrogram"></td>
<td><img src="docs/images/meter-loudness.png" alt="The Loudness Meter's big numbers and readings"></td>
</tr>
<tr>
<td><b>Spectrogram</b>: the Spectrum over time, scrolling. Drag a box to zoom in.</td>
<td><b>Loudness Meter</b>: momentary, short-term and integrated LUFS, LRA, true peak, PLR and PSR, as big numbers, bars or a loudness graph, with a target line. It measures to ITU-R BS.1770-5 and EBU Tech 3341/3342, checked against the EBU test set (<a href="https://daswerk.github.io/Das-Meter/measurements.html">what that means</a>).</td>
</tr>
<tr>
<td><img src="docs/images/meter-stereometer.png" alt="The Stereometer and its correlation bar"></td>
<td><img src="docs/images/meter-phase-scope.png" alt="The Phase Scope with a kick and a bass overlaid, showing where their lows cancel"></td>
</tr>
<tr>
<td><b>Stereometer</b>: where the sound sits between left and right, with the phase correlation and an optional balance bar.</td>
<td><b>Phase Scope</b>: one beat or bar held still, so a kick on the beat stays put, following your DAW's tempo or a typed or tapped one. Overlay a second track (say, the bass under the kick) to see where their lows cancel, with the <b>Cycle correlation</b> and suggestions to fix it (flip, move or retune).</td>
</tr>
<tr>
<td><img src="docs/images/meter-cepstrum.png" alt="The Cepstrum finding a 220 Hz tone, A3"></td>
<td><img src="docs/images/meter-oscilloscope.png" alt="The Oscilloscope holding a steady tone still"></td>
</tr>
<tr>
<td><b>Cepstrum</b>: the pitch of a voice or instrument, with its note: handy for tuning.</td>
<td><b>Oscilloscope</b>: the wave itself over 1–200 ms, held still on a steady tone.</td>
</tr>
<tr>
<td><img src="docs/images/meter-tuner.png" alt="The Tuner reading A2, 7 cents sharp"></td>
<td><b>Tuner</b>: the note large, a cents bar and the pitch, refined from the harmonics to about a cent; A4 adjustable. Fits the Bar.</td>
</tr>
<tr>
<td><img src="docs/images/meter-stereo-bands.png" alt="Stereo Bands: the low end mono, the highs wide"></td>
<td><b>Stereo Bands</b>: correlation and width in ten octave bands, 31 Hz to 16 kHz: is the low end mono, and where is the width?</td>
</tr>
<tr>
<td colspan="2"><img src="docs/images/meter-tonal-balance.png" alt="The Tonal Balance Meter: the mix's curve over a Target's range, too much highs marked"></td>
</tr>
<tr>
<td colspan="2"><b>Tonal Balance</b>: your mix's balance against a <b>Target</b> made from your own reference songs (played, or analysed from files), finest in the low end (two points per semitone below 300 Hz), with the Sub and Bass notes, low end mono and punch, and the Target shifted to your mix's key (<a href="https://daswerk.github.io/Das-Meter/meters.html#tonal-balance">more</a>).</td>
</tr>
</table>

Drag a box over the Spectrum to zoom into it at a finer resolution, with its
own scale:

![Dragging a box over the Spectrum opens a zoom window with its own scale](docs/images/spectrum-zoom.png)

## Make it yours

**Full Theme customization.** Seven built-in Themes, and every one of them can
be changed: its Look, its layout and every colour. Change anything in a
built-in and it's saved as your own copy, which you can rename. A Preset can
follow the system's light/dark setting with a pair of Themes, and Themes are
plain TOML files you can share.

![All built-in Themes: Nocturne, Dark, Light, High contrast, Midnight Purple, Deep Turquoise and Graphite, and an edited Nocturne](docs/images/themes.png)

Everything is in **Settings ▸ Theme**, with a live preview:

![Settings ▸ Theme: the Look, its sliders, layout and colours with a live preview](docs/images/theme-editor.png)

### Look

**Smooth** or **Classic**, per Theme, in one click.

![The Spectrum in the Smooth and the Classic Look](docs/images/setting-look.png)

For Smooth, tune the **Glow** under traces and big numbers…

![Glow at 0, 0.6 and 1](docs/images/setting-glow.png)

…how far the **grid fades** out toward the edges…

![Grid fade at 0 and 0.4](docs/images/setting-grid-fade.png)

…and the background's **Gradient** and the panels' **Vignette**. **Dim when
silent** lets Meters dim gently when no audio arrives.

![Gradient and Vignette off and full](docs/images/setting-gradient-vignette.png)

### Layout

**Background opacity** makes the Bar or window see-through, down to only the
Meters' traces over your desktop:

![The Bar at background opacity 1 and 0.15, over a desktop picture](docs/images/setting-opacity.png)

**Line thickness**, for every trace and line:

![Thin, Normal and Thick lines, close up](docs/images/setting-lines.png)

The **gap** between Meters and their **corner radius**:

![Gap 0 with square corners, and gap 16 with corner radius 24](docs/images/setting-gap-radius.png)

And the **text size**, from 0.9 to 1.5:

![The Loudness Meter at text size 0.9 and 1.5](docs/images/setting-text-size.png)

**Shape cues besides colour** add patterns for colour-blind viewers (High
contrast has them on).

### Colours

Every colour has a role: background, panels, grid, text and accent, and each
Meter's own colours (Waveform bands, Spectrum line and fill, peak dots and
harmonics, loudness bars and peaks, correlation, the Stereometer, Cepstrum,
Phase Scope and Oscilloscope traces, and more). A single Meter can override
any of them, so one Meter can stand apart from its Theme:

![All Meters in Theme colours, and the Waveform with its own colours](docs/images/setting-meter-colours.png)

## Layouts

- **Bar**: a strip of Meters docked to a screen edge, as many as you like of
  any kind. Resize it, share out its space, or ⌘-drag it anywhere. On Windows
  it can reserve its space so other windows make room; it can float on top and
  stay over fullscreen apps.
- **Pop-outs**: pull any Meter out of the Bar into its own window, and dock it
  back.
- **Window mode**: all Meters in one window, split into resizable panes.
- **Multiple displays**: each window remembers its display and comes back to
  it.

![The Bar along the bottom of the screen](docs/images/bar.png)

## Presets

A **Preset** saves which Meters you see, how they're arranged, their settings
and their Theme. Changes save on their own, and **Revert Preset** undoes them.
Switch with ⌘1–⌘9. Built-ins for **Bar**, **Mixing**, **Mastering**,
**Mastering advanced**, **Producing** and **Compact** get you started.
**Export Preset…** writes one `.dasmeter-preset` file with its Theme inside,
for someone else to import.

## Where the audio comes from

- **System Capture** (the default): everything your computer plays, with no
  driver to install.
- **Send Plugins**: put the Das-Meter Send Plugin (CLAP, VST3 and AU) on DAW
  tracks, and each Meter picks which track it shows. They work behind ASIO
  too, and tell the Phase Scope the DAW's tempo and beat. Each one is named
  after its track and takes the track's colour.

Das-Meter is light on your computer: it draws only when something changes,
sleeps in silence, and does its analysis outside your DAW.

## Install

| | |
|---|---|
| **macOS** 14.6+, Apple silicon and Intel | Download `Das-Meter-<version>.dmg`, drag Das-Meter to Applications, then allow it once under **System Settings ▸ Privacy & Security ▸ Open Anyway**. |
| **Linux** x86_64 (Arch, Fedora, Ubuntu 24.04+…), PipeWire | Download `das-meter-<version>-linux-x86_64.tar.gz`, unpack it and run `./das-meter`. There's no Send Plugin for Linux yet. |
| **Windows** 10 and 11 | Coming soon. |

The [install guide](INSTALL.md) has every step, including the Send Plugin,
updates, uninstalling and building from source. The
[docs](https://daswerk.github.io/Das-Meter/) explain every Meter and setting.

## Building from source

You need a recent stable [Rust](https://rustup.rs) (1.95 or newer). On Linux
you also need a few system libraries; see
[Building from source](INSTALL.md#building-from-source).

```sh
cargo run --release --bin das-meter   # run the app
cargo test --workspace                # run the tests
```

On macOS, `scripts/macos-bundle.sh` builds `Das-Meter.app`. System Capture only
works from the app bundle. To serve the docs site locally, run
`mdbook serve docs/site`.

The screenshots here are drawn offscreen by the app itself, from generated
audio. To redo them after a visual change, build the app and run
`scripts/readme-screenshots.py` (it needs Pillow).

| Path | What it is |
|---|---|
| `crates/transport` | The shared-memory table that carries audio from Send Plugins to the app |
| `crates/analysis` | Meter analysis: audio frames in, Meter readings out |
| `crates/core` | The headless app core: all app behaviour, driven by events |
| `crates/app` | The app: the platform shell and the Meter renderers |
| `crates/send-plugin` | The Send Plugin (CLAP, wrapped to VST3 and AU) |
| `docs/site` | The [docs site](https://daswerk.github.io/Das-Meter/) (mdBook) |
| `docs/adr` | Architecture decisions, with the [research](docs/research) behind them |
| `docs/release-checklist.md` | How to make a release |

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option. Contributions are accepted under the same terms.
