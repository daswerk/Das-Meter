# Listen to

**Listen to** decides where every Meter gets its audio. It's app-wide: Meters
never mix the two.

- **System Capture** (the default): everything your computer plays through its
  default output.
- **Send Plugins**: the audio of the DAW tracks you put a
  [Send Plugin](send-plugins.md) on. Each Meter picks its own.

Switch in **Listen to** in the menu bar (macOS) or in the settings. While you
listen to Send Plugins, System Capture pauses.

## System Capture

System Capture follows your default output device. If you switch outputs or
the sample rate changes, the Meters start over and say "Output changed: reset".

## macOS permission

On macOS, System Capture needs **Screen & System Audio Recording** permission.
macOS asks when you press **Start listening**. Das-Meter only reads the audio
to draw Meters; nothing is recorded or saved.

If you said no, or the Meters stay flat: open **System Settings ▸ Privacy &
Security ▸ Screen & System Audio Recording** and turn Das-Meter on. The
"Hearing nothing?" hint has a button that opens this page.

## Hearing nothing

After about 10 seconds of silence on System Capture, Das-Meter shows a hint.
Common reasons:

- Nothing is playing, or it's playing on another output device than the
  default one.
- **Your DAW uses ASIO** (Windows) or another exclusive-mode driver. That audio
  bypasses the system mixer, so System Capture can't hear it. Use
  [Send Plugins](send-plugins.md) instead: the hint switches in one click.
- On macOS, the permission is off (see above).
