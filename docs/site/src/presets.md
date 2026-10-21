# Presets and sharing

A **Preset** is a saved setup: which Meters you see, how they're arranged, their
settings and their Theme. Changes save on their own a moment after you make
them; **Revert Preset** goes back to how the Preset was when you opened it.

- Switch Presets from the **Presets** menu, or with **⌘1–⌘9** (Ctrl on
  Windows) for the first nine.
- **Save as New Preset** keeps the current setup under a new name.
- The built-in Presets can be reset to how they came. Each comes with its
  own Theme, of the same name:
  - **Producing Top-Bar 1.0**: a Bar along the top edge with Waveform,
    Spectrogram, Stereometer and a Mid/Side Spectrum.
  - **Mixing/Producing 1.0**: Spectrum and Stereo Bands on top; Waveform,
    Loudness Meter and Stereometer below.
  - **Mastering 1.0**: Spectrum over Tonal Balance; Stereo Bands, Waveform,
    Loudness Meter and Stereometer below.

  An update that brings new built-ins adds them once, at the end of the list.
  Presets made by an earlier version, its built-ins among them, stay as they
  are.

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
