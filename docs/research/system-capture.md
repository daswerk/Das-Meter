# Research: how System Capture works on macOS and Windows

Ticket: [#3](https://github.com/daswerk/Das-Meter/issues/3). Researched 2026-09-24.

This note collects facts only. It does not pick an approach.
"System Capture" means what CONTEXT.md says: whatever audio the computer is playing.

**How sources were checked.** Apple facts come from Apple's documentation JSON (the data behind developer.apple.com, including the per-symbol "available since" versions). Microsoft facts come from Microsoft's own doc repos on GitHub (`MicrosoftDocs/win32`, `MicrosoftDocs/sdk-api`) and `microsoft/Windows-classic-samples`, because learn.microsoft.com was not reachable from the research machine. Library facts come from each library's source at the commit named. The network also blocked minimeters.app and support.apple.com, so the claims that rely on them come from search-result excerpts and are marked **(unverified)**.

---

## 1. macOS

There are three ways to capture system audio.

### 1a. ScreenCaptureKit (SCK) audio

| Fact | Source |
|---|---|
| The ScreenCaptureKit framework has existed since macOS 12.3. | [Apple: ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit) (platforms: macOS 12.3) |
| Audio capture (`SCStreamConfiguration.capturesAudio`, `SCStreamOutputType.audio`) needs **macOS 13.0**. "A stream doesn't capture audio by default." | [capturesAudio](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/capturesaudio), [SCStreamOutputType.audio](https://developer.apple.com/documentation/screencapturekit/scstreamoutputtype/audio) |
| `excludesCurrentProcessAudio` (macOS 13.0) keeps the app's own sound out of the capture. | [excludesCurrentProcessAudio](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/excludescurrentprocessaudio) |
| Supported sample rates are **8000, 16000, 24000 and 48000 Hz only**. The default is 48 kHz. | [sampleRate](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/samplerate) |
| The channel count is **1 or 2 only**. The default is stereo. | [channelCount](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/channelcount) |
| Audio arrives as `CMSampleBuffer`s that wrap an `AudioBufferList`. | [SCStreamOutputType.audio](https://developer.apple.com/documentation/screencapturekit/scstreamoutputtype/audio) |
| Microphone capture in the same stream (`captureMicrophone`) needs macOS 15.0. | [captureMicrophone](https://developer.apple.com/documentation/screencapturekit/scstreamconfiguration/capturemicrophone) |
| **Permission: Screen Recording.** Apple says: "Request screen recording permission from the person before capturing content", with an `NSScreenCaptureUsageDescription` Info.plist key. | [Apple: ScreenCaptureKit](https://developer.apple.com/documentation/screencapturekit) |
| Apple's sample code says: "The first time you run this sample, the system prompts you to grant the app Screen Recording permission. After you grant permission, **you need to restart the app** to enable capture." | [Capturing screen content in macOS](https://developer.apple.com/documentation/screencapturekit/capturing-screen-content-in-macos) |
| Apple recommends `SCContentSharingPicker` (macOS 14.0), the system picker, over an app's own selection UI. | [SCContentSharingPicker](https://developer.apple.com/documentation/screencapturekit/sccontentsharingpicker) |
| An SCK stream always needs a content filter for a display, window or app. MiniMeters "disregards the video information from the capture session, leaving only the audio". **(unverified: search excerpt of minimeters.app)** | [MiniMeters Help: Audio Routing (macOS)](https://minimeters.app/help/audio-macos/) |
| From macOS 15 (Sequoia), the Settings pane is called "Screen & System Audio Recording". Beta reports describe a recurring "Continue to allow" re-confirmation prompt for screen-recording apps. **(unverified: search excerpts; support.apple.com was unreachable)** | [Apple Support: Control access to screen and system audio recording](https://support.apple.com/en-gb/guide/mac-help/mchld6aa7d23/mac), [Apple Dev Forums 760112](https://developer.apple.com/forums/thread/760112) |
| Reported quirk: from macOS 14.2, the level of SCK-captured audio changed depending on the output device (-6 to -14 dB trims on some external interfaces). This also affects OBS. **(unverified: search excerpt of minimeters.app)** | [MiniMeters Help: Audio Routing (macOS)](https://minimeters.app/help/audio-macos/) |
| **Latency:** Apple publishes no latency figure for SCK audio. | (none found) |
| **Driver install:** none needed. | (part of the OS) |

### 1b. Core Audio process taps + aggregate device

| Fact | Source |
|---|---|
| `AudioHardwareCreateProcessTap` is available from **macOS 14.2**. | [AudioHardwareCreateProcessTap](https://developer.apple.com/documentation/coreaudio/audiohardwarecreateprocesstap(_:_:)) |
| How it works: create a `CATapDescription`, then call `AudioHardwareCreateProcessTap` to get a tap `AudioObjectID`. Next, create an aggregate device (`AudioHardwareCreateAggregateDevice`) and add the tap's UID to its `kAudioAggregateDeviceTapListKey`. The aggregate device can then be read "just like a microphone". | [Capturing system audio with Core Audio taps](https://developer.apple.com/documentation/coreaudio/capturing-system-audio-with-core-audio-taps) |
| A tap can capture one process, a group of processes, or everything except a list (`CATapDescription(stereoGlobalTapButExcludeProcesses:)`). It can mix down to mono or stereo. It can be private to the app that made it. It can also **mute** the tapped process, so its sound goes only to the tap. | Same article; [CATapDescription](https://developer.apple.com/documentation/coreaudio/catapdescription), [muteBehavior](https://developer.apple.com/documentation/coreaudio/catapdescription/mutebehavior) |
| **Permission: "system audio recording".** The Info.plist must have `NSAudioCaptureUsageDescription` (macOS 14.2+). "The first time you start recording from an aggregate device that contains a tap, the system prompts you to grant the app system audio recording permission." This permission is separate from Screen Recording. | [NSAudioCaptureUsageDescription](https://developer.apple.com/documentation/bundleresources/information-property-list/nsaudiocaptureusagedescription), [Core Audio taps article](https://developer.apple.com/documentation/coreaudio/capturing-system-audio-with-core-audio-taps) |
| "There's no public API to request audio recording permission or to check if the app has that permission." AudioCap uses a private TCC API for this, or it falls back to the prompt that appears when recording starts. | [insidegui/AudioCap README](https://github.com/insidegui/AudioCap) @ `6f609e8` (third-party reference project) |
| The minimum version differs between sources. Apple's API docs and sample say **14.2**. AudioCap says the API was "introduced" in **14.4**. cpal requires **14.6+** for its loopback. None of them explains the difference. | Apple links above; AudioCap README; [cpal README](https://github.com/RustAudio/cpal/blob/master/README.md) |
| **Latency:** Apple publishes no figure. The capture runs through a normal HAL IOProc on the aggregate device, so the buffer size is the device's I/O buffer size. This is inferred from the design, not documented. | (inference) |
| **Driver install:** none needed. | (part of the OS) |

### 1c. Virtual loopback driver (BlackHole, Loopback, Sound Siphon…)

| Fact | Source |
|---|---|
| BlackHole is "a modern macOS virtual audio loopback driver … with zero additional latency". It works on macOS 10.10 and later. It comes as a 2/16/64-channel build and supports sample rates from 8 kHz to 768 kHz. | [ExistentialAudio/BlackHole README](https://github.com/ExistentialAudio/BlackHole) @ `62953f5` |
| It needs an **installed driver**, from a .pkg installer or Homebrew, plus an uninstaller. To still hear the audio, the user must route output through it, for example with a Multi-Output device in Audio MIDI Setup. The README notes that macOS cannot change the volume of a Multi-Output device. | Same README |
| Licence: GPLv3. "A license is required for all non-GPLv3 projects." | Same README |
| No capture permission is needed. The app reads it like an ordinary input device, which only needs the normal microphone permission. (Inferred: a driver input is a normal input device.) | (inference) |
| MiniMeters recommends BlackHole/Loopback with Audio Hijack, or Sound Siphon, for macOS 12 and earlier, where it has no desktop capture. **(unverified: search excerpt)** | [MiniMeters Help: Audio Routing (macOS)](https://minimeters.app/help/audio-macos/) |

---

## 2. Windows

### 2a. WASAPI loopback (whole endpoint)

| Fact | Source |
|---|---|
| Open the **render** endpoint (`IMMDevice`) and call `IAudioClient::Initialize` with `AUDCLNT_STREAMFLAGS_LOOPBACK`. This captures "the system mix that is being played by the audio engine". | [MicrosoftDocs: Loopback Recording](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/CoreAudio/loopback-recording.md) (= learn.microsoft.com/windows/win32/coreaudio/loopback-recording) |
| **Shared mode only:** "Exclusive-mode streams cannot operate in loopback mode." | Same |
| Event-driven loopback capture works on **Windows 10 1703+**. Earlier versions needed a workaround that used a render stream to drive the events. | Same |
| It captures one endpoint at a time: the mix going to *that* output device. DRM-protected streams may be excluded by trusted drivers. | Same |
| No user permission prompt is documented for loopback. | (nothing found in the Loopback Recording page) |
| It does not capture audio sent to devices that bypass the Windows audio engine, such as **ASIO**. MiniMeters: "if you use ASIO with your DAW, MiniMeters cannot capture your desktop audio while ASIO is in use". It points those users to its DAW plugin. **(unverified: search excerpt)** | [MiniMeters Help: Audio Routing (Windows)](https://minimeters.app/help/audio-windows/) |
| **Driver install:** none needed. | (part of the OS) |

### 2b. Process loopback (per-application)

| Fact | Source |
|---|---|
| Call `ActivateAudioInterfaceAsync` with `AUDIOCLIENT_ACTIVATION_PARAMS { ActivationType = AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK }` and `AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS { TargetProcessId, ProcessLoopbackMode }`. | [MicrosoftDocs sdk-api: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS](https://github.com/MicrosoftDocs/sdk-api/blob/docs/sdk-api-src/content/audioclientactivationparams/ns-audioclientactivationparams-audioclient_process_loopback_params.md) |
| Two modes: `INCLUDE_TARGET_PROCESS_TREE` (only that process and its children) and `EXCLUDE_TARGET_PROCESS_TREE` (everything *except* them, which can leave out the app itself). | [PROCESS_LOOPBACK_MODE](https://github.com/MicrosoftDocs/sdk-api/blob/docs/sdk-api-src/content/audioclientactivationparams/ne-audioclientactivationparams-process_loopback_mode.md) |
| **Documented minimum: "Windows 10 Build 20348".** This is in `req.target-min-winverclnt` on all three API pages and in the sample README ("requires Windows 10 build 20348 or later"). The ticket's "Windows 10 2004" (build 19041) is **not** what Microsoft documents. Whether it works on 19041 in practice was not checked. | sdk-api pages above; [Windows-classic-samples ApplicationLoopback README](https://github.com/microsoft/Windows-classic-samples/tree/main/Samples/ApplicationLoopback) |
| "The capture is not tied to a specific audio endpoint." "If the processes whose audio will be captured does not have any audio rendering streams, then the capturing process receives silence." | ApplicationLoopback README |

---

## 3. Library support

### Rust

| Crate (version / commit checked) | macOS | Windows | Source |
|---|---|---|---|
| **cpal** (released 0.18.2, 2026-08-16; master 0.19.0-dev @ `79275c2`) | **Process tap loopback.** It was added in 0.17.0 ("Support for loopback recording … on macOS > 14.6"). Building an *input* stream on an *output* device creates a private global tap (empty process list, `exclusive = true`, unmuted) plus a private aggregate device, which is destroyed on drop. Tap bugs were fixed in 0.18.0, including "loopback capture returning silence due to disabled tap auto-start". README: CoreAudio backend min macOS 14.2, "loopback recording requires 14.6+". No ScreenCaptureKit path. | **Endpoint loopback.** Using an output device as an input "will transparently enable loopback mode" (`AUDCLNT_STREAMFLAGS_LOOPBACK`). No process-loopback API was found in `src/`. Min Windows 10. | [CHANGELOG](https://github.com/RustAudio/cpal/blob/master/CHANGELOG.md), `src/host/coreaudio/macos/loopback.rs`, `src/host/wasapi/mod.rs`, `src/host/wasapi/device.rs` |
| **screencapturekit** (screencapturekit-rs 10.0.3 @ `5e2c581`) | Wraps SCK, including system audio. The crate's real floor is **macOS 13.0** ("uses the 13.0 audio APIs unconditionally"). Needs Xcode Command Line Tools to build. It says SCK is "purely TCC-gated", with no entitlement. | n/a | [doom-fish/screencapturekit-rs README](https://github.com/doom-fish/screencapturekit-rs) |
| **coreaudio-rs** (0.14.2 @ `ee36eb9`) | Wraps AudioUnit/HAL. No tap or aggregate helpers (no matches for `ProcessTap`/`CATap`). For raw APIs, its README points to the **objc2-core-audio** crates. cpal's tap code uses these (`AudioHardwareCreateProcessTap`, `CATapDescription`). | n/a | [RustAudio/coreaudio-rs](https://github.com/RustAudio/coreaudio-rs) |
| **wasapi** (wasapi-rs 0.24.0 @ `63ad5a2`) | n/a | Endpoint loopback (example `loopback.rs`) **and** process loopback: `AudioClient::new_application_loopback_client(process_id, include_tree)` (example `record_application.rs`). Doc comments list many `IAudioClient` methods that return "Not implemented" on such clients. | [HEnquist/wasapi-rs](https://github.com/HEnquist/wasapi-rs), `src/api.rs` |

### C++ / JUCE

| Fact | Source |
|---|---|
| JUCE 9.0.2 (`72782788`, 2026-09-07): `juce_audio_devices` has **no** loopback, process-tap or ScreenCaptureKit support. No matches for `loopback`, `CATap`, `ProcessTap` or `ScreenCaptureKit`. The WASAPI stream flags there are only `EVENTCALLBACK` / `AUTOCONVERTPCM` / `SRC_DEFAULT_QUALITY`. A JUCE app would call the OS APIs itself. | [juce-framework/JUCE `modules/juce_audio_devices/native/juce_WASAPI_windows.cpp`](https://github.com/juce-framework/JUCE/blob/master/modules/juce_audio_devices/native/juce_WASAPI_windows.cpp) |
| Plain C/C++ references: Apple's Core Audio taps sample (macOS 26 SDK / Xcode 26), Apple's SCK sample, Microsoft's ApplicationLoopback sample, and AudioCap (Swift). | links above |

---

## 4. How MiniMeters does it

All of these are **(unverified)**: minimeters.app could not be fetched, and they come from search-result excerpts.

- **macOS 13+:** it uses ScreenCaptureKit for "Desktop Audio" and asks for the Screen Recording permission. There is no desktop capture on macOS 12 and earlier, where it recommends BlackHole/Loopback + Audio Hijack or Sound Siphon. ([audio-macos](https://minimeters.app/help/audio-macos/))
- **Windows:** "Default Output Capture", which is WASAPI loopback of the default output. It cannot capture while the DAW uses ASIO; in that case the MiniMetersServer plugin (VST3/AU/CLAP) is the workaround. ([audio-windows](https://minimeters.app/help/audio-windows/))

## 5. Side-by-side (facts only)

| | Min OS | Permission | Driver | Rates/channels | Per-app / exclude-self |
|---|---|---|---|---|---|
| SCK audio | macOS 13.0 | Screen (& System Audio) Recording | no | 8/16/24/48 kHz; 1–2 ch | exclude own process; filter by app |
| Core Audio tap | macOS 14.2 (cpal: 14.6) | System audio recording (`NSAudioCaptureUsageDescription`) | no | device format | include/exclude process lists; can mute |
| Virtual driver | macOS 10.10 (BlackHole) | normal input-device access | **yes** | up to 768 kHz, up to 256 ch | via user routing |
| WASAPI loopback | Vista+ (event-driven: Win10 1703) | none documented | no | endpoint mix format | no (whole endpoint) |
| Process loopback | Win10 build 20348 | none documented | no | requested format | include/exclude process tree |

## Open questions not answered here

- Measured latency of SCK audio compared with a Core Audio tap. No primary source gives a number.
- Whether process loopback works on Windows 10 builds before 20348, such as 19041/2004.
- Why cpal requires 14.6 for taps when Apple documents 14.2.
- The exact current macOS 15/26 re-prompt behaviour for Screen Recording compared with System Audio Recording (support.apple.com was unreachable).
