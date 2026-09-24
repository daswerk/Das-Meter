# Das-Meter

A standalone, highly customizable audio visualizer for music producers. It shows live meters for audio coming from the computer or from a DAW.

## Language

### Audio in

**Listen to**:
The app-wide choice of where all Meters get their audio: System Capture, or Send Plugins. Meters never mix the two.
_Avoid_: Input mode, Source Mode

**Source**:
Where the audio a Meter shows comes from. When the app listens to System Capture, every Meter's Source is System Capture; when it listens to Send Plugins, each Meter picks its own Send Plugin as its Source.
_Avoid_: Input, device, feed

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

**Stereometer**:
The Meter for stereo image: where the sound sits between left and right, with the phase correlation shown under it.
_Avoid_: Goniometer, vectorscope, correlometer

**Channel View**:
Which pair of channels a Waveform or Spectrum shows: the mono sum, Left and Right together, or Mid and Side together. A Meter never shows a single channel on its own.
_Avoid_: Channel mode, routing

**Preset**:
A saved setup: which Meters are shown, how they are arranged, their settings and the look.
_Avoid_: Profile, scene

### Layout

**Bar**:
A strip of Meters docked to one screen edge. On Windows it can reserve its strip so other windows make room; on macOS it floats.
_Avoid_: Dock, strip, toolbar

**Pop-out**:
A single Meter taken out of the Bar into its own window; it can be docked back.
_Avoid_: Detached Meter, floating window

**Window mode**:
The layout where all Meters share one ordinary window, split into resizable panes.
_Avoid_: Tiled mode, grid
