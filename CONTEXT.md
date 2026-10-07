# Das-Meter

A standalone, highly customizable audio visualizer for music producers. It shows live meters for audio coming from the computer or from a DAW.

## Language

### Audio in

**Listen to**:
The app-wide choice of where all Meters get their audio: System Capture, or Send Plugins. Meters never mix the two.
_Avoid_: Input mode, Source Mode

**Source**:
Where the audio a Meter shows comes from. When the app listens to System Capture, every Meter's Source is System Capture; when it listens to Send Plugins, each Meter picks its own Send Plugin as its Source. A Phase Scope can also show an Overlay Source beside it.
_Avoid_: Input, device, feed

**Overlay Source**:
A second Send Plugin that a Phase Scope draws over its main Source, so the two can be compared. Only Send Plugins can be an Overlay Source; with System Capture the Phase Scope shows its one Source alone.
_Avoid_: Sidechain, second input

**System Capture**:
The default Source: the mix of everything the computer plays through its default output. Audio that bypasses the operating system's mixer, such as a DAW using ASIO on Windows, is not heard; Send Plugins cover that case.
_Avoid_: Loopback, desktop audio

**Send Plugin**:
A DAW plugin that forwards the audio of the track it sits on to the Das-Meter app. It shows no Meters of its own. Each one has a name (typed by the user, else the DAW's track name, else an animal name such as "Otter") and a colour (the DAW's track colour, else a random one), and keeps both across project reloads.
_Avoid_: Server plugin, bridge

### Visuals

**Meter**:
One visualization of the audio, such as a Waveform or a Spectrum, with its own settings.
_Avoid_: Module, visualizer, widget

**Loudness Meter**:
The Meter for how loud the audio is: peak and RMS levels together with LUFS loudness.
_Avoid_: Level meter, LUFS meter, VU

**Spectrogram**:
The Meter for how the spectrum changes over time: frequency up, time across, loudness as brightness.
_Avoid_: Waterfall, sonogram

**Phase Scope**:
The Meter that shows the waveform over one Cycle, held still so each beat lands in the same place. It can draw an Overlay Source on top to show where two tracks, such as kick and bass, push together or cancel out.
_Avoid_: Oscilloscope, occularScope, cycle scope

**Cycle**:
The stretch of time a Phase Scope shows: one beat or one bar. It follows the DAW's tempo when listening to Send Plugins, and a typed-in or tapped tempo with System Capture.
_Avoid_: Window, period, sweep

**Stereometer**:
The Meter for stereo image: where the sound sits between left and right, with the phase correlation shown under it.
_Avoid_: Goniometer, vectorscope, correlometer

**Channel View**:
Which pair of channels a Waveform or Spectrum shows: the mono sum, Left and Right together, or Mid and Side together. A Meter never shows a single channel on its own.
_Avoid_: Channel mode, routing

**Preset**:
A saved setup: which Meters are shown, how they are arranged, their settings and which Theme they use.
_Avoid_: Profile, scene

**Theme**:
A named set of colours and styling (background opacity, line thickness, spacing, text size, Look) that decides how Meters and the app look. Each Preset picks a Theme, or a light/dark pair that follows the system; several Presets can share one.
_Avoid_: Skin, style, colour scheme

**Look**:
How a Theme is drawn, part of its styling: **Classic** (flat panels, plain lines) or **Smooth** (depth, faded grids, soft glows, floating panels). Any Theme can be drawn either way; colours stay the Theme's.
_Avoid_: Skin, mode, version

### Layout

**Bar**:
A strip of Meters docked to one screen edge, or ⌘-dragged off it to anywhere on its display (it keeps lying along its edge's direction). Docked on Windows it can reserve its strip so other windows make room; on macOS it floats.
_Avoid_: Dock, strip, toolbar

**Pop-out**:
A single Meter taken out of the Bar into its own window; it can be docked back.
_Avoid_: Detached Meter, floating window

**Window mode**:
The layout where all Meters share one ordinary window, split into resizable panes.
_Avoid_: Tiled mode, grid
