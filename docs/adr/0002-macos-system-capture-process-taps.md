# macOS System Capture uses Core Audio process taps, not ScreenCaptureKit

On macOS, System Capture uses a Core Audio process tap on an aggregate device (through cpal), so the app needs macOS 14.6 or later. We chose this over ScreenCaptureKit, which MiniMeters uses and which works from macOS 13. With a tap, the user sees only the "system audio recording" prompt instead of "Screen Recording", doesn't have to restart the app after granting permission, and gets audio at the output device's own sample rate instead of at most 48 kHz. It's also a single code path through a crate the stack already uses. We ship no virtual driver such as BlackHole (GPL, and the user has to install it and route audio through it).

The costs: Macs older than 14.6 aren't supported at all, not even for Send Plugins, so the app has one OS floor. There's also no public API to check whether permission was granted. A denied tap just delivers silence, so the app shows a hint that points to System Settings after a stretch of silence.

Decided in [Choose the System Capture approach and minimum OS versions](https://github.com/daswerk/Das-Meter/issues/11). Research: [`docs/research/system-capture.md`](../research/system-capture.md).
