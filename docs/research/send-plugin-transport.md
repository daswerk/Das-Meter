# Research: how the Send Plugin gets audio to the app

Ticket: [#4](https://github.com/daswerk/Das-Meter/issues/4). Researched 2026-09-24.

This file collects facts and leaves the choice open. Wording follows `CONTEXT.md`: **Send Plugin**, **Source**, **Meter**.

Each claim carries a source link. Some claims are my own reading of a source or have no primary source I could open; those are marked **(inference)** or **(unverified)**. `support.apple.com`, `learn.microsoft.com`, `minimeters.app`, `bitwig.com` and `rossbencina.com` were blocked from this research environment, so I read Microsoft docs from their GitHub source repos and read Apple docs through the `developer.apple.com/documentation/*.md` endpoints.

---

## 1. Realtime rules for the Send Plugin's audio thread

- CLAP names what the audio thread should avoid: "malloc() and free(), contended locks and mutexes, I/O, waiting, and so forth". The deadline "can be <1ms". [clap/ext/thread-check.h](https://github.com/free-audio/clap/blob/main/include/clap/ext/thread-check.h)
- CLAP also warns that "the audio-thread is symbolic": a host may call `process()` on different OS threads over time, but never on two at once for one instance. [same file](https://github.com/free-audio/clap/blob/main/include/clap/ext/thread-check.h)
- Apple says hosts call an AU's `renderBlock` "from a realtime context". [AUAudioUnit.renderBlock](https://developer.apple.com/documentation/audiotoolbox/auaudiounit/renderblock)
- The C++ standard only *recommends* that lock-free atomics be "address-free". Its note says this is what makes atomics work "by memory that is shared between two processes". So atomics in shared memory are sound when `is_always_lock_free` is true, but the standard does not promise it. [C++ draft, threads.tex [atomics.lockfree]](https://github.com/cplusplus/draft/blob/main/source/threads.tex)

**What this means for each transport (inference):**
- **Shared memory with a lock-free SPSC ring:** the audio thread only does atomic loads and stores plus `memcpy`. There are no syscalls and no waiting, so it fits every rule above.
- **Sockets, pipes, Mach messages, XPC:** each send is a syscall, which counts as "I/O" under CLAP's rule. The usual way to stay realtime-safe is a two-stage design: the audio thread writes into an in-process lock-free ring, and a plugin-owned non-realtime thread drains that ring into the socket or pipe.

## 2. Latency

- I found no primary-source benchmark comparing the options, so this section gives no numbers.
- **Shared memory (inference):** a sample is visible to the reader as soon as it is written. End-to-end latency is set by how often the app reads (its frame or timer rate) and by how much buffered data it keeps.
- **Sockets and pipes (inference):** they add a syscall and a kernel copy per chunk, plus a wake-up of the forwarding thread. For a visualizer that redraws every ~8–16 ms (60–120 Hz), this cost is small compared with the display interval. Syscall cost is not what rules sockets out; the realtime rule in §1 is what matters.
- **MiniMetersServer's ring (arithmetic):** it holds 8192 stereo frames (16384 floats, see §5), which is about 171 ms at 48 kHz and about 85 ms at 96 kHz. Its source has a `FIXME` saying the buffer should be longer for high sample rates. [PluginProcessor.cpp#L224-L228](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/PluginProcessor.cpp#L224-L228)

## 3. Sandboxing and process isolation

### macOS: where plugin code actually runs

- **AUv2 and non-sandbox-safe AUs:** `AudioComponent.h` says a "sandbox-safe AudioComponent can function correctly in even the most severely sandboxed process", which has "curtailed or no access to … communication with other processes". [AudioComponent.h (macOS 11.3 SDK mirror)](https://github.com/phracker/MacOSX-SDKs/blob/master/MacOSX11.3.sdk/System/Library/Frameworks/AudioToolbox.framework/Versions/A/Headers/AudioComponent.h)
- **The `resourceUsage` key:** an AU that is not sandbox-safe declares the resources it needs in its Info.plist `resourceUsage` dictionary. The documented keys are `iokit.user-client`, `mach-lookup.global-name` ("the mach services the AudioComponent needs to connect to … via bootstrap_look_up() or XPC"), `network.client` and `temporary-exception.files.all.read-write`.
  - When a sandboxed host loads such an AU, "the system evaluates the 'resourceUsage' information against the restrictions the process is under".
  - If the host has `com.apple.security.temporary-exception.audio-unit-host`, the user is asked whether to suspend the host's sandbox.
  - [same header](https://github.com/phracker/MacOSX-SDKs/blob/master/MacOSX11.3.sdk/System/Library/Frameworks/AudioToolbox.framework/Versions/A/Headers/AudioComponent.h)
  - POSIX shared memory is **not** among the documented `resourceUsage` keys **(inference from the list above)**.
- **AUv3:** by default an AUv3 "can be loaded into a separate extension service process". In-process loading on macOS is only a request (`kAudioComponentInstantiation_LoadInProcess`), and it needs the AU packaged in a separate bundle. [same header](https://github.com/phracker/MacOSX-SDKs/blob/master/MacOSX11.3.sdk/System/Library/Frameworks/AudioToolbox.framework/Versions/A/Headers/AudioComponent.h)
- **Logic Pro on Apple silicon (unverified):** it hosts AUs out of process, in `AUHostingService`, with `AUHostingCompatibilityService` for x86 plugins under Rosetta. Apple's support note says AU problems "can't cause the app to quit". I could not fetch that page; this comes from the search snippet of [support.apple.com/102082](https://support.apple.com/en-us/102082) and user reports.
  - I found no Apple primary source saying which sandbox profile `AUHostingService` applies. **Open question:** test on a real machine whether `shm_open` with an arbitrary name, a Unix socket in `/tmp`, or a Mach lookup works from inside Logic.
- **Bitwig Studio (unverified):** its plugin-hosting modes include "Within Bitwig", "Together" (the default), "By Manufacturer", "By Plug-in" and "Individually". In every mode except the first, plugins run in separate processes. This comes from the search snippet of the [Bitwig user guide](https://www.bitwig.com/userguide/latest/vst_plug-in_handling_and_options/), which I could not fetch.
  - Consequence **(inference):** several Send Plugin instances cannot count on sharing one process. Any registry of instances has to be cross-process, not a static singleton.

### macOS: App Sandbox naming rules for IPC

- Apple's app-group entitlement doc gives the rules for sandboxed apps doing IPC. Apps in the same group "can communicate … using Mach IPC, XPC, POSIX semaphores and shared memory, and UNIX domain sockets". [com.apple.security.application-groups](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.application-groups)

  | Mechanism | Name rule | Length limit |
  |---|---|---|
  | Mach IPC, XPC | `<group identifier>.<unique name>` | `BOOTSTRAP_MAX_NAME_LEN` |
  | POSIX semaphores | `<group identifier>/<unique name>` | `PSEMNAMLEN` |
  | POSIX shared memory | `<group identifier>/<unique name>` | `PSHMNAMLEN` |
  | UNIX domain sockets | socket file must be in the app group container | limited by `SOCK_MAXADDRLEN` |

- The same doc: "In macOS, use app groups to enable IPC communication between two sandboxed apps, or between a sandboxed app and a nonsandboxed app." [same](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.application-groups)
- **Short names:** `PSHMNAMLEN` and `PSEMNAMLEN` are both **31** in xnu ([posix_shm.h](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/posix_shm.h), [posix_sem.h](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/posix_sem.h)). `shm_open` fails with `ENAMETOOLONG` beyond that ([shm_open.2](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/shm_open.2)). A Team-ID-prefixed group name such as `ABCDE12345.dasmeter/` already uses 20 characters.
- **Entitlements belong to the process (inference):** a Send Plugin is loaded into the host's process, or into `AUHostingService`, and runs with that process's entitlements. The Send Plugin cannot add itself to an app group. The app-group rules therefore only help if the *Das-Meter app* is sandboxed, for example for the Mac App Store, and the plugin side is not.
- **Network sockets:** a sandboxed app needs `com.apple.security.network.client` to open outgoing connections, including "to a server process running … on the same machine", and `…network.server` to listen. [network.client](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.network.client), [network.server](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.network.server)
- **The Mac App Store requires App Sandbox.** [App Sandbox](https://developer.apple.com/documentation/security/app-sandbox)
- **Two macOS shared-memory quirks from xnu:**
  - A shared-memory object "persists until it is unlinked and all other references are gone". A crashed writer therefore leaves the name behind. [shm_open.2](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/shm_open.2)
  - `ftruncate` on an object that has already been sized returns `EINVAL`, so a shared-memory object's size is fixed the first time it is set. [posix_shm.c `pshm_truncate`](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/posix_shm.c)

### Windows

- **Session namespaces:** named file-mapping objects, events, mutexes and semaphores live in a per-session namespace by default. `Global\` puts a name in the global namespace and `Local\` makes the session namespace explicit. [Kernel object namespaces](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/TermServ/kernel-object-namespaces.md)
- **Creating global mappings needs a privilege:** "The creation of a file-mapping object … in the global namespace … from a session other than session zero is a privileged operation" and needs `SeCreateGlobalPrivilege`. The check applies only to creation, not to opening. [same](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/TermServ/kernel-object-namespaces.md)
  - **Inference:** a DAW and the Das-Meter app run by the same user in the same session share a namespace, so an unprefixed or `Local\` name is enough and needs no privilege. `Global\` would fail for a normal user process.
- **Store / AppContainer apps:** "The global namespace is not available for Windows Store apps." [same](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/TermServ/kernel-object-namespaces.md)
- **Opening an existing mapping:** `CreateFileMapping` on an existing name returns that object "with its current size, not the specified size" and sets `ERROR_ALREADY_EXISTS`. [CreateFileMappingA](https://github.com/MicrosoftDocs/sdk-api/blob/docs/sdk-api-src/content/winbase/nf-winbase-createfilemappinga.md)
- **Lifetime:** a mapping is freed once all handles are closed. So on Windows, unlike macOS, a crashed writer's segment goes away when the last process closes its handle. [Creating Named Shared Memory](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/Memory/creating-named-shared-memory.md)
- **Named pipes:** they support many instances under one name, "each instance has its own buffers and handles", so several clients can connect at once. They are reachable remotely when the Server service runs unless `NT AUTHORITY\NETWORK` is denied. [Named Pipes](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/ipc/named-pipes.md)

## 4. Several Send Plugins: identity, names, disappearance

### What each plugin format tells the plugin about its track

- **CLAP, `clap.track-info/1`:** the host's `get()` fills `clap_track_info` with `name`, `color`, audio channel count and port type, plus flags for return track, bus and master. The plugin's `changed()` callback fires when any of these change. Both calls are `[main-thread]`. There is no stable track ID. [clap/ext/track-info.h](https://github.com/free-audio/clap/blob/main/include/clap/ext/track-info.h)
- **VST3, `Vst::IInfoListener::setChannelContextInfos`:** this is on the edit controller, called on the UI thread, and "the host will call setChannelContextInfos for each change occurring to this channel". Keys include:
  - `kChannelUIDKey`: "unique id string used to identify a channel"
  - `kChannelRuntimeIDKey`: "may change when reloading project"
  - `kChannelNameKey`: "name of the channel like displayed in the mixer"
  - `kChannelColorKey`, `kChannelIndexKey`, `kChannelPluginLocationKey`

  Every key is optional. [ivstchannelcontextinfo.h](https://github.com/steinbergmedia/vst3_pluginterfaces/blob/master/vst/ivstchannelcontextinfo.h)
- **AU:** `AUAudioUnit.contextName` gives "information about the host context in which the audio unit is connected … a host could set 'track 3' as the context". It is bridged to the v2 property `kAudioUnitProperty_ContextName`. There is no track ID or colour. [AUAudioUnit.contextName](https://developer.apple.com/documentation/audiotoolbox/auaudiounit/contextname)
- **All three are optional (inference):** hosts may not provide any of them. A Send Plugin needs its own instance ID, and it needs a fallback display name, such as a name the user types in the plugin UI.

### How instances could be told apart (options, no decision)

- **Instance ID:** generated at construction, for example a 128-bit random UUID. MiniMetersServer uses a hash of 64 random bits (see §5).
  - If the ID should survive project reloads so a Meter keeps its Source, the ID has to be saved in plugin state (CLAP `state`, VST3 `getState`, AU `fullState`). MiniMetersServer saves nothing: `getStateInformation` is empty. [PluginProcessor.cpp#L243-L249](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/PluginProcessor.cpp#L243-L249)
  - Duplicating a track would then copy the ID. **(inference)**
- **Announcement:** one well-known registry (a shared-memory table of slots, or a socket or pipe the app listens on) where each instance registers its ID, name, sample rate and channel count, and points to its own ring (its own segment, or a slot index).

### How the app can notice a Send Plugin disappearing

- **Clean removal:** the plugin's destructor or deactivate clears its slot. This is what MiniMetersServer does (§5). A crash or a killed host skips it.
- **Heartbeat / stale counter:** the writer bumps an atomic counter or timestamp every block, and the app treats a slot as gone after N ms without change. This works with pure shared memory. **(inference)** It cannot tell "transport stopped / host not processing" apart from "gone", because many hosts stop calling `process()` when idle. **(inference)**
- **Process watch:** the slot stores the writer's PID.
  - On macOS, the app can use kqueue `EVFILT_PROC` with `NOTE_EXIT`. The man page says "If a process can normally see another process, it can attach an event to it". [kqueue.2](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/kqueue.2)
  - On Windows, the app can open the process and `WaitForSingleObject` on it; "Process" is a waitable object. [WaitForSingleObject](https://github.com/MicrosoftDocs/sdk-api/blob/docs/sdk-api-src/content/synchapi/nf-synchapi-waitforsingleobject.md)
  - This notices a host crash, but not a single instance being removed while the host keeps running. **(inference)**
- **Connection-based:** with a socket or pipe, the app sees EOF or an error when the writer closes it, and when the writer's process dies. **(inference, standard socket semantics)**
- **Windows named mutex:** if the owning thread ends without releasing it, waiters get `WAIT_ABANDONED`. [Mutex Objects](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/Sync/mutex-objects.md) Ownership is per *thread*, and CLAP says the audio thread can change from call to call, so the mutex would have to be owned by a plugin-created thread. **(inference)**

## 5. How MiniMetersServer does it

Source: [direct-audio/MiniMetersServer](https://github.com/direct-audio/MiniMetersServer) at `master` = `6a21e02` (2023-12-24, `project(MiniMetersServer VERSION 1.0.8)`). It is **GPL-3.0** ([LICENSE](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/LICENSE)): read it for ideas and do not copy code into a differently-licensed project. The MiniMeters app (the reader side) is closed source, so only the writer side can be seen. Closed builds shipped after this commit may behave differently. **(unverified)**

- **Framework and formats:** JUCE, with AU, VST3 and Standalone targets, plus CLAP through `clap-juce-extensions`. [CMakeLists.txt#L7-L30](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/CMakeLists.txt#L7-L30)
- **Transport:** one fixed-name shared-memory segment `"MM Server"`, sized `sizeof(MM::IpcHandler)`.
  - macOS / Linux: `shm_open(…, O_CREAT | O_RDWR, 0666)`, `ftruncate`, `mmap(MAP_SHARED)`.
  - Windows: `CreateFileMapping(INVALID_HANDLE_VALUE, …, "MM Server")` + `MapViewOfFile`, with no `Global\`/`Local\` prefix, so it lands in the session namespace (§3).
  - [PluginProcessor.cpp#L62-L109](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/PluginProcessor.cpp#L62-L109), [SharedMemory.h#L55-L57](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/SharedMemory.h#L55-L57)
  - The name has no app-group prefix, so it would not satisfy the macOS sandbox naming rule in §3. **(inference)**
- **Layout:** `IpcHandler { atomic<int> block_size; atomic<int> sample_rate; atomic<int64_t> current_id; CircleBuffer<float, 16384> buffer; }`. The ring has atomic `read_head`, `write_head` and `is_full`, and the writer pushes one float at a time. When the ring is full, the writer drops samples. [SharedMemory.h#L11-L52](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/SharedMemory.h#L11-L52)
  - Both sides write `is_full` (the writer sets it, the reader clears it), so the ring is not a strict single-writer-per-field SPSC design. **(inference from the code)**
- **Audio format:** always interleaved stereo float. Mono input is copied to both channels. The plugin passes audio through unchanged. The sample rate and block size are published per block; there is no channel-count field. [PluginProcessor.cpp#L183-L231](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/PluginProcessor.cpp#L183-L231)
- **Several instances: only one sends at a time.**
  - Each instance generates a 64-bit random "UUID" string and hashes it (`compute_hash`) into `m_uuid_hash`. [PluginProcessor.cpp#L21-L55](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/PluginProcessor.cpp#L21-L55)
  - On construction the instance writes its hash into `current_id` and so becomes "primary", which means the newest instance wins. [#L57-L60, #L108-L117](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/PluginProcessor.cpp#L57-L117)
  - In `processBlock`, any instance whose hash is not `current_id` returns early and shows "Another instance is sending audio to MiniMeters." with a "make primary" button. [PluginProcessor.cpp#L191-L196](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/PluginProcessor.cpp#L191-L196), [PluginEditor.cpp#L15-L65](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/PluginEditor.cpp#L15-L65)
  - The source has no track names, no per-instance channels and no way for the app to pick an instance. The processor header has `// TODO: Have proper handshake between MiniMeters and the plugin.` [PluginProcessor.h#L76](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/PluginProcessor.h#L76)
- **Disappearance:** the destructor sets `current_id = 0` without checking whether this instance was primary. There is no heartbeat and no PID, so a crash leaves a stale `current_id`. [PluginProcessor.cpp#L119-L122](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/PluginProcessor.cpp#L119-L122)
- **Finding or launching the app:**
  - Windows: scans the process list for `MiniMeters.exe` and launches it from `Program Files` with `-d "MiniMeters Plugin"`.
  - macOS: looks the app up by bundle ID through `NSWorkspace`. The running check is stubbed to return `true`.
  - [MiniMetersOpener.cpp](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/MiniMetersOpener.cpp), [MacOsHelpers.mm](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/MacOsHelpers.mm)
- **Earlier design (branch `ipc_experiment`, 2022-11):**
  - An HTTP server built on `cpp-httplib` on `0.0.0.0:8422`, with `/hi` (sample dump as text), `/check`, `/data` (JSON with block size, sample rate and host name) and `/stop`. Only one instance could own the port; `/stop` let a new instance take over.
  - The branch resampled to 44.1 kHz with Speex and already had the shared-memory path; the HTTP code is commented out there.
  - The current editor still prints "An error occurred while trying to access port 8422". [ipc_experiment/PluginProcessor.cpp](https://github.com/direct-audio/MiniMetersServer/blob/ff6eb58ede8033cf8c5a82a15a1e2fb7e57937cf/PluginProcessor.cpp), [PluginEditor.cpp#L67-L73](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/PluginEditor.cpp#L67-L73)
- **Defects visible in the code (inference from reading it):**
  - The `mmap` failure check compares with `nullptr` instead of `MAP_FAILED`. [#L73-L77](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/PluginProcessor.cpp#L73-L77)
  - The interleave loops iterate to `n_samples * 2` where `n_samples` is already `numSamples * 2`, so they read past the channel buffers. [#L202-L222](https://github.com/direct-audio/MiniMetersServer/blob/6a21e023ee769d80982c6b09713474edbb6d108c/PluginProcessor.cpp#L202-L222)

## 6. Open questions for the decision

1. Does `shm_open` with a non-group name, a Unix socket in `/tmp` or `$TMPDIR`, or a Mach `bootstrap_look_up` work from an AU inside Logic Pro's `AUHostingService`, and from GarageBand? This needs a test on a real Mac. I found no primary source.
2. Will the Das-Meter app ever be sandboxed (Mac App Store)? If yes, every name must fit `<TeamID>.<group>/<name>` in 31 characters for shared memory.
3. Should a Meter's Source survive a project reload? If yes, the instance ID has to be saved in plugin state, and duplicated tracks need handling.
4. Should one ring per Send Plugin be written into one shared table, or should each instance have its own segment? This affects the macOS stale-name cleanup, since names persist until `shm_unlink`.
