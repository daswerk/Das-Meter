# Das-Meter

A standalone, highly customizable audio visualizer for music producers. It shows live meters for audio coming from the computer or from a DAW.

## Language

### Audio in

**Source**:
Where the audio a Das-Meter window is showing comes from: either System Capture or a Send Plugin.
_Avoid_: Input, device, feed

**System Capture**:
The default Source: whatever audio the computer is currently playing.
_Avoid_: Loopback, desktop audio

**Send Plugin**:
A DAW plugin with no visuals of its own that forwards the audio of the track it sits on to the Das-Meter app.
_Avoid_: Server plugin, bridge

### Visuals

**Meter**:
One visualization of the audio, such as a Waveform or a Spectrum, with its own settings.
_Avoid_: Module, visualizer, widget

**Preset**:
A saved setup: which Meters are shown, how they are arranged, their settings and the look.
_Avoid_: Profile, scene
