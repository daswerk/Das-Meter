# Presets and sharing

A **Preset** is a saved setup: which Meters you see, how they're arranged, their
settings and their Theme. Changes save on their own a moment after you make
them; **Revert Preset** goes back to how the Preset was when you opened it.

- Switch Presets from the **Presets** menu, or with **⌘1–⌘9** (Ctrl on
  Windows) for the first nine.
- **Save as New Preset** keeps the current setup under a new name.
- The built-in Presets can be reset to how they came:
  - **Bar**: Waveform, Spectrum, Stereometer and Loudness Meter along an edge.
  - **Mixing**: Spectrum and Phase Scope on top; Waveform, Loudness Meter and
    Stereometer below.
  - **Mastering**: a large Loudness Meter beside Spectrum and Stereometer.
  - **Mastering advanced**: a large Loudness Meter with its graph and PLR/PSR,
    beside Spectrum, Spectrogram, Waveform and Stereometer.
  - **Producing**: Spectrum on top; Phase Scope, Cepstrum (for tuning) and a
    small Loudness Meter below.
  - **Compact**: a thin Bar with the loudness numbers, Spectrum and Stereometer.

  An update that brings new built-ins adds them once, at the end of the list;
  a Mixing you never changed becomes the new one.

Presets are files in the presets folder:

- macOS: `~/Library/Application Support/Das-Meter/presets`
- Windows: `%APPDATA%\Das-Meter\presets`
- Linux: `~/.config/das-meter/presets`

## Sharing

**Export Preset…** saves the current Preset as one `.dasmeter-preset` file,
with its Theme inside. Send it to someone; they open it with **Import
Preset…**, by dropping it on Das-Meter or, on macOS, by double-clicking it.

A shared Preset leaves out what's personal to your setup, such as which
monitor serial number a window was on.
