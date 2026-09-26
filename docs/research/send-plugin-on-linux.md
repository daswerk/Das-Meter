# Research: the Send Plugin on Linux in Bitwig

Ticket: [#80](https://github.com/daswerk/Das-Meter/issues/80) (part of [#76](https://github.com/daswerk/Das-Meter/issues/76)). Researched 2026-09-26.

This file collects facts and leaves the choices open. Wording follows `CONTEXT.md`: **Send Plugin**, **Meter**.

Each claim carries a source link. Claims that are my own reading of a source are marked **(inference)**. Claims with no primary source I could open are marked **(unverified)**. Nothing here was run on a Linux machine; §6 lists what to test.

---

## Short answer

- **Build:** the crate already builds and tests on Linux in CI. clap-wrapper-rs compiles its VST3 wrapper for Linux and exports `ModuleEntry`/`ModuleExit`. What's missing is a Linux branch in `scripts/bundle-send-plugin.sh`: a single `Das-Meter Send.clap` file, and a `Das-Meter Send.vst3/Contents/x86_64-linux/Das-Meter Send.so` bundle. Install them to `~/.clap` and `~/.vst3`.
- **Window on Wayland:** Bitwig is an X11 client on Linux (through Xwayland on a Wayland session), and CLAP has no embedded Wayland windows at all. baseview's X11 child window goes into the X11 parent that Bitwig passes. This should work. A recent baseview bug made windows stay black in the Flatpak Bitwig under Xwayland, but the revision we pin predates it.
- **Shared memory, `.deb` Bitwig:** Bitwig's plug-in host processes are ordinary processes of the same user, sharing the host `/dev/shm`. The table should just work **(inference)**.
- **Shared memory, Flatpak Bitwig:** **it does not work as shipped.** The Flathub manifest grants `--device=all` but not `--device=shm`. Flatpak then mounts a private tmpfs on `/dev/shm`, so the Send Plugin inside Bitwig and the app outside see two different tables. The user can fix it with `flatpak override --user --device=shm com.bitwig.BitwigStudio`. Even then, the Flatpak has its own PID namespace, which breaks the table's `host_pid` checks (§3.4). "Open Das-Meter" and the window's font also fail inside the Flatpak (§3.5, §2.4).
- **Track name and colour:** Bitwig implements CLAP `track-info` since 5.1 (December 2023). The Send Plugin already reads it through clack. Use the CLAP build in Bitwig. The VST3 build on Linux has a separate problem: its heartbeat only runs while the window is open (§1.4).

---

## 1. Building the CLAP and the VST3 on Linux

### 1.1 What the repo does today

- `crates/send-plugin` is a `cdylib` with the CLAP entry plus `clap_wrapper::export_vst3!()` and `export_auv2!()` (`crates/send-plugin/src/lib.rs`).
- CI builds, lints and tests the whole workspace on `ubuntu-24.04`, and installs `libx11-dev libx11-xcb-dev libxcursor-dev libxcb1-dev libxcb-icccm4-dev libgl1-mesa-dev` "for the Send Plugin's window" (`.github/workflows/ci.yml`). It skips the bundling step on Linux.
- `scripts/bundle-send-plugin.sh` handles macOS and Windows and exits with "only macOS and Windows are supported" otherwise.
- The release workflow's Linux job builds only `dasmeter-app` (`.github/workflows/release.yml`).
- `INSTALL.md` says: "There's no Send Plugin for Linux yet".

### 1.2 clap-wrapper-rs on Linux

- The README says it is "Tested on Linux (Ubuntu 22.04), MacOS (13.7) and Windows (10)". It leaves bundling to you and points to Steinberg's format page. [clap-wrapper 0.3.1 README](https://docs.rs/crate/clap-wrapper/0.3.1/source/README.md)
- `build.rs` compiles the VST3 wrapper for `linux` with `linuxmain.cpp` and clap-wrapper's `detail/os/linux.cpp`, defining `LIN`. The AUv2 wrapper is built only on macOS. [build.rs](https://docs.rs/crate/clap-wrapper/0.3.1/source/build.rs)
- On Unix other than macOS, `export_vst3!()` exports `GetPluginFactory`, `ModuleEntry` and `ModuleExit`. These are the entry points a Linux VST3 host looks for. `export_auv2!()` expands to an empty module off macOS, so the current `lib.rs` compiles unchanged. [src/lib.rs](https://docs.rs/crate/clap-wrapper/0.3.1/source/src/lib.rs)

### 1.3 Bundle layout and install folders

**CLAP.** On Linux a `.clap` is a single shared object renamed, not a bundle. Hosts search `~/.clap` and `/usr/lib/clap`, and also every directory in `CLAP_PATH`. They search recursively for files ending in `.clap`. [clap/entry.h](https://github.com/free-audio/clap/blob/a47f6badb49d948fd009998f28309cdab78979c9/include/clap/entry.h)

**VST3.** A Linux bundle is `MyPlugin.vst3/Contents/x86_64-linux/MyPlugin.so`, optionally with `Contents/Resources/moduleinfo.json`. "The folder (bundle) and the shared library (.so) file must have the same name". The architecture folder is `uname -m` + `-linux`. [VST 3 Plug-in Format](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Locations+Format/Plugin+Format.html)

Search order: `$HOME/.vst3/`, `/usr/lib64/vst3/` (Fedora), `/usr/lib/vst3/`, `/usr/local/lib64/vst3/`, `/usr/local/lib/vst3/`, then `$APPFOLDER/vst3/`. [VST 3 Plug-in Locations](https://steinbergmedia.github.io/vst3_dev_portal/pages/Technical+Documentation/Locations+Format/Plugin+Locations.html)

So the Linux branch of the bundle script would produce (inference, following the two specs above and the Windows branch):

```
target/plugins/Das-Meter Send.clap                                   # = libdasmeter_send.so
target/plugins/Das-Meter Send.vst3/Contents/x86_64-linux/Das-Meter Send.so
```

**Install for the Flatpak Bitwig: use the home folders.** The Flathub manifest sets `CLAP_PATH=/app/extensions/Plugins/clap` and `VST3_PATH=/app/extensions/Plugins/vst3`, and grants `--filesystem=host`. [Flathub manifest](https://github.com/flathub/com.bitwig.BitwigStudio/blob/42d54e0fc78811070e399b22f96e5edd37012c4c/com.bitwig.BitwigStudio.yaml)
- `--filesystem=host` exposes the home folder, but not `/usr`, which is reserved and holds the runtime. [Flatpak sandbox permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html)
- CLAP makes `CLAP_PATH` an addition to the default folders, not a replacement ([entry.h](https://github.com/free-audio/clap/blob/a47f6badb49d948fd009998f28309cdab78979c9/include/clap/entry.h)).
- **(inference)** So `~/.clap` and `~/.vst3` are visible to both the `.deb` and the Flatpak Bitwig. A host-wide `/usr/lib/clap` or `/usr/lib/vst3` install is **not** visible inside the Flatpak. Recommend the home folders, as the Windows and macOS installs do for user scope.

### 1.4 The VST3 on Linux only ticks while its window is open

This affects the VST3 build and not the CLAP build. It matters for any Linux host that loads the VST3.

- The Send Plugin bumps its heartbeat from a CLAP host timer every 250 ms (`plugin.rs` registers `HEARTBEAT_INTERVAL`, and `on_timer` calls `Link::tick`, which calls `writer.heartbeat()`). The app marks a Send Plugin **Gone** after 2 s without a heartbeat (`GONE_AFTER` in `crates/transport/src/lib.rs`).
- In clap-wrapper's VST3 wrapper on Linux, CLAP timers are attached to the host's `Steinberg::Linux::IRunLoop`. The wrapper gets that run loop only from the `IPlugFrame` in `WrappedView::setFrame`, which is when the editor opens. It detaches the timers again when the view is destroyed. [plugview.cpp (vendored)](https://docs.rs/crate/clap-wrapper/0.3.1/source/external/clap-wrapper/src/detail/vst3/plugview.cpp), [wrapasvst3.cpp (vendored)](https://docs.rs/crate/clap-wrapper/0.3.1/source/external/clap-wrapper/src/wrapasvst3.cpp)
- The fallback path runs timers from `onIdle()`. Its own comment says "if we don't have a runloop on linux onIdle isn't called anyway". The Linux helper's `init()` is empty, and nothing calls its `executeDefered()`. [wrapasvst3.cpp](https://docs.rs/crate/clap-wrapper/0.3.1/source/external/clap-wrapper/src/wrapasvst3.cpp), [os/linux.cpp](https://docs.rs/crate/clap-wrapper/0.3.1/source/external/clap-wrapper/src/detail/os/linux.cpp)
- **(inference)** So a VST3 Send Plugin on Linux stops heartbeating whenever its window is closed, and its Meter goes Gone after 2 s. This is the same gap the window already works around for clap-wrapper's AU ("Hosts without a timer … never tick the Link", `window.rs`). But on Linux there is not even a timer while the window is closed.
- Fix options, not chosen here: in Bitwig, ship and recommend the CLAP. For the VST3, bump the heartbeat from a plugin-owned background thread on Linux, or let the audio thread's frame counter count as liveness. The audio thread already bumps `processed`.

## 2. The plug-in window on a Wayland session

### 2.1 CLAP and Wayland

`clap/ext/gui.h` defines `CLAP_WINDOW_API_WAYLAND` with the note "embed is currently not supported, use floating windows". `CLAP_WINDOW_API_X11` uses "physical size" and embeds via XEmbed. [clap/ext/gui.h](https://github.com/free-audio/clap/blob/a47f6badb49d948fd009998f28309cdab78979c9/include/clap/ext/gui.h)

On Linux, any embedded CLAP window is therefore an X11 window.

### 2.2 Bitwig is an X11 application

- The Flathub manifest opens `--socket=x11` and no `wayland` socket ("Needed to talk with X11, Wayland, pulseaudio and pipewire" is the comment above `--share=ipc`, `--socket=pulseaudio`, `--socket=x11`). [Flathub manifest](https://github.com/flathub/com.bitwig.BitwigStudio/blob/42d54e0fc78811070e399b22f96e5edd37012c4c/com.bitwig.BitwigStudio.yaml)
- **(inference)** On a Wayland session the Flatpak Bitwig runs under Xwayland, and gives plug-ins an X11 parent window on the Xwayland server.
- The Bitwig 5.2, 5.1, 6.0 and 6.1 release notes I read mention neither Wayland nor X11. Their Linux requirements are a `.deb` (Ubuntu 24.04+) or the Flatpak, plus "a Vulkan- or GL-compatible video driver". [6.1](https://downloads.bitwig.com/6.1/Release-Notes-6.1.html), [6.0](https://downloads.bitwig.com/6.0/Release-Notes-6.0.html)
- That the `.deb` build is also X11-only is **(unverified)**. Community reports say "no native Wayland support" ([KVR forum](https://www.kvraudio.com/forum/viewtopic.php?t=575305), not a primary source).

### 2.3 baseview on Linux

- At the revision we pin, baseview is X11-only on Linux. `lib.rs` has `#[cfg(target_os = "linux")] mod x11;`, and the Linux dependencies are `x11rb` and `x11` (xlib, xlib_xcb). [Cargo.toml @237d323](https://github.com/RustAudio/baseview/blob/237d323c729f3aa99476ba3efa50129c5e86cad3/Cargo.toml), [src/lib.rs](https://github.com/RustAudio/baseview/blob/237d323c729f3aa99476ba3efa50129c5e86cad3/src/lib.rs)
- `open_parented` accepts only `Xlib` or `Xcb` parent handles, and panics on anything else. It creates a plain child window of the parent id; there is no XEmbed code in the tree. [src/x11/window.rs](https://github.com/RustAudio/baseview/blob/237d323c729f3aa99476ba3efa50129c5e86cad3/src/x11/window.rs)
- It opens its **own** X connection with `XOpenDisplay(NULL)`, that is from `$DISPLAY`, and `assert!`s that it succeeded. [src/x11/xcb_connection.rs](https://github.com/RustAudio/baseview/blob/237d323c729f3aa99476ba3efa50129c5e86cad3/src/x11/xcb_connection.rs)
- It runs its event loop on its own thread (`thread::spawn` in `open_parented`), so it doesn't need the host's CLAP `posix-fd-support` or `timer-support` to draw. [src/x11/window.rs](https://github.com/RustAudio/baseview/blob/237d323c729f3aa99476ba3efa50129c5e86cad3/src/x11/window.rs)
- The Send Plugin offers exactly this. `is_api_supported` accepts only `GuiApiType::default_for_current_platform()`, which clack defines as `X11` on Unix other than macOS, and only embedded (`!is_floating`). (`crates/send-plugin/src/plugin.rs`; [clack-extensions 0.2.0 gui.rs](https://docs.rs/crate/clack-extensions/0.2.0/source/src/gui.rs))
- **(inference)** Bitwig passes an X11 window on the same X server that `$DISPLAY` names: Xwayland on a Wayland session, and the Flatpak sets `DISPLAY` because it has `--socket=x11`. So the window should open. If `DISPLAY` were unset, the `assert!` would panic across the FFI boundary and take Bitwig's plug-in host process down. That can't happen with a host that hands out X11 parents, since it needs `DISPLAY` itself.

### 2.4 Known Bitwig + Xwayland problems

- [baseview#331](https://github.com/RustAudio/baseview/issues/331) (closed): on Arch with the COSMIC desktop and the **Flatpak Bitwig 6**, a clack + baseview plug-in's window "stayed black". The cause was baseview's new X11 visibility tree. Xwayland reported the host's parent window as unmapped, so `on_frame()` never ran.
  - The fix is [baseview PR #340](https://github.com/RustAudio/baseview/pull/340), still **open** at the time of writing. The reporter and a maintainer confirm it fixes the problem.
  - The visibility tree landed in baseview in August 2026 ([commit history of `src/platform/x11/visibility_tree.rs`](https://github.com/RustAudio/baseview/commits/master/src/platform/x11/visibility_tree.rs)). Our pin `237d323` is from 2025-11-29 and has no visibility logic in `src/x11/`.
  - **(inference)** We are not affected today. Upgrading baseview/egui-baseview past the visibility-tree change before #340 lands would bring the black window in.
- **The window's font inside the Flatpak (repo finding).** `window.rs` loads the UI font only from `/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf` (Debian/Ubuntu) or `/usr/share/fonts/TTF/DejaVuSans.ttf` (Arch). If neither exists, `set_parent` fails with "couldn't read the system font" and no window opens.
  - Inside the Flatpak, `/usr` is the `org.freedesktop.Platform` runtime (25.08 per the manifest). That runtime installs DejaVu to `%{datadir}/fonts/dejavu`, so the file is `/usr/share/fonts/dejavu/DejaVuSans.ttf`. [freedesktop-sdk 25.08 dejavu-fonts.bst](https://gitlab.com/freedesktop-sdk/freedesktop-sdk/-/blob/release/25.08/elements/components/dejavu-fonts.bst)
  - **(inference)** The Send Plugin window fails to open in the Flatpak Bitwig until that path is added. Fedora puts DejaVu under yet another folder (`/usr/share/fonts/dejavu-sans-fonts/`, **(unverified)**).
  - Asking fontconfig, or trying a list of known folders, would cover all of these.

## 3. Shared memory between Bitwig's plug-in hosts and the app

### 3.1 What the transport does

- `crates/transport/src/shm.rs` calls `shm_open("/dasmeter.v1", O_RDWR|O_CREAT|O_EXCL, 0600)` or opens the existing segment, then `ftruncate` and `mmap(MAP_SHARED)`.
- Each slot stores `host_pid` = `std::process::id()` (`writer.rs`).
  - The app marks a slot Gone if `kill(host_pid, 0)` fails with anything but `EPERM`, or if the heartbeat is 2 s stale (`reader.rs`, `process.rs`).
  - A Send Plugin that claims a slot frees any slot whose `host_pid` isn't alive by that same test (`reclaim_dead_slot`, and the duplicate-ID check in `claim_in`).
- The ADR is [docs/adr/0003](../adr/0003-send-plugin-shared-memory-table.md).

### 3.2 Linux basics

- "On Linux, shared memory objects are created in a (tmpfs(5)) virtual filesystem, normally mounted under /dev/shm." [shm_overview(7)](https://man7.org/linux/man-pages/man7/shm_overview.7.html) So `shm_open` in a process sees whatever `/dev/shm` is mounted in that process's mount namespace.
- IPC namespaces isolate "System V IPC objects" and "POSIX message queues". POSIX shared memory is not among them. [ipc_namespaces(7)](https://man7.org/linux/man-pages/man7/ipc_namespaces.7.html)
- **(inference)** Flatpak's `--share=ipc` is therefore irrelevant to our table. What matters is which `/dev/shm` is mounted.

### 3.3 Bitwig's separate plug-in host processes

- Bitwig's Plug-in Hosting Modes are "Within Bitwig", "Together", "By Manufacturer", "By Plug-in" and "Individually". All but the first host plug-ins outside the audio engine; "Individually" puts "every plug-in instance by itself". [Bitwig user guide: plug-in handling](https://www.bitwig.com/userguide/latest/vst_plug-in_handling_and_options/)
- The release notes call these "sandboxed" plug-ins, for example "Fixed an audio engine crash in some cases when a sandboxed plug-in crashed". [5.2 release notes](https://downloads.bitwig.com/5.2/Release-Notes-5.2.html) I found no Bitwig source saying this sandbox restricts files or IPC. It reads as crash isolation.
- **(inference)** In the `.deb` install, every plug-in host process runs as the same user in the host's mount namespace, so all of them and the app open the same `/dev/shm/dasmeter.v1`. The 0600 mode is fine, since it's the same UID. The Send Plugin already treats instances as cross-process (one table, per-slot `host_pid`), so hosting modes change nothing.
- In the Flatpak, all Bitwig processes, the plug-in hosts included, run inside the one sandbox and share its `/dev/shm` **(inference)**. So Send Plugins still see each other. The problem is the app outside.

### 3.4 The Flatpak Bitwig does not see the host's `/dev/shm`

- Manifest `finish-args`: `--device=all`, `--share=ipc`, `--socket=x11`, `--socket=pulseaudio`, `--filesystem=xdg-run/pipewire-0`, `--share=network`, `--filesystem=host`, and no `--device=shm`, no `--allow=per-app-dev-shm`. [Flathub manifest](https://github.com/flathub/com.bitwig.BitwigStudio/blob/42d54e0fc78811070e399b22f96e5edd37012c4c/com.bitwig.BitwigStudio.yaml)
- Flatpak docs: `--device=all` is "All devices, including all of the above **except `shm`**", and `--device=shm` is "Shared Memory in `/dev/shm`". `/dev` is also one of the reserved paths that `--filesystem=host` does not expose. [Flatpak sandbox permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html)
- Flatpak source, `flatpak-run.c`: with `DEVICE_ALL`, it `--dev-bind`s host `/dev`, then says "Don't expose the host /dev/shm, just the device nodes, unless explicitly allowed". [flatpak-run.c](https://github.com/flatpak/flatpak/blob/ddcd5c4ebb545a7a1e7225a96bd44256c61ac5cb/common/flatpak-run.c#L413-L455)
  - Without `DEVICE_SHM` and without the per-app feature, it adds `--tmpfs /dev/shm`: "The host, the original sandbox and each subsandbox each have a separate /dev/shm."
  - This dates from 2018: [89e2b66 "Don't expose host /dev/shm with --device=all"](https://github.com/flatpak/flatpak/commit/89e2b6679cd4a59c15b225c0c6c0e1293ae83f7d).
  - `--device=shm` ("giving access to host /dev/shm, as needed for jack") came in 1.6.1: [39903ea](https://github.com/flatpak/flatpak/commit/39903eab40920dab749a795ce2bef8ce54aacc1a), [NEWS](https://github.com/flatpak/flatpak/blob/ddcd5c4ebb545a7a1e7225a96bd44256c61ac5cb/NEWS).
  - `--allow=per-app-dev-shm` (1.11.1) shares `/dev/shm` only between instances of the same app, not with the host: [cb47d83](https://github.com/flatpak/flatpak/commit/cb47d83b7221de7f08a487dd47e69eec57f458e5).
- **Conclusion:** as shipped on Flathub, the Send Plugin in the Flatpak Bitwig creates its own private `dasmeter.v1`, and the app never sees it.
- **The user-side fix:** `flatpak override --user --device=shm com.bitwig.BitwigStudio`. `flatpak override --device` accepts `shm`. [flatpak command reference](https://docs.flatpak.org/en/latest/flatpak-command-reference.html)
  - This is a persistent change to the user's Flatpak configuration, so Das-Meter should document it, not do it silently.
  - Bitwig/Flathub could also add `--device=shm` to the manifest. Flatpak's own reason for it ("needed for jack") suggests other audio tools hit the same problem **(inference)**.

**PID namespace (also Flatpak only).**
- Flatpak always runs apps with `--unshare-pid` unless the parent shares PIDs ([flatpak-run.c](https://github.com/flatpak/flatpak/blob/ddcd5c4ebb545a7a1e7225a96bd44256c61ac5cb/common/flatpak-run.c#L2448-L2449)), and its docs say sandboxed apps have "No access to processes outside the sandbox" ([sandbox permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html)).
- **(inference)** With `--device=shm` in place:
  - The `host_pid` a sandboxed Send Plugin writes is a PID in Bitwig's namespace, usually a small number. The app's `kill(pid, 0)` on the host tests some unrelated process:
    - If that process is a kernel thread or another user's process (`EPERM`), the slot looks alive, and Gone detection falls back to the 2 s heartbeat. That is tolerable.
    - If that PID is unused (`ESRCH`), the app marks a live Send Plugin **Gone** at once. That is a bug.
  - Inside the sandbox, a Send Plugin claiming a slot tests other slots' host PIDs, which it cannot see (`ESRCH`). `reclaim_dead_slot` would take **live** slots belonging to Send Plugins in other DAWs running natively at the same time. The duplicate-ID check would likewise free a live slot that has the same ID.
- The table would need a namespace-aware liveness rule before the Flatpak is supported. Options, not decided here:
  - record the writer's PID-namespace inode (`/proc/self/ns/pid`) next to `host_pid`, and use heartbeat-only liveness across namespaces;
  - or detect the Flatpak (`/.flatpak-info` exists) and write `host_pid = 0` meaning "unknown", which the reader and `reclaim_dead_slot` then treat by heartbeat only. Note that `process_alive(0)` returns false today, so 0 would currently mean "dead".

### 3.5 "Open Das-Meter" from inside the Flatpak

- `window.rs` runs `Command::new("das-meter")` on Linux. Inside the Flatpak that runs a command in the sandbox's `PATH` and the runtime's `/usr`, not the user's `~/.local/bin` **(inference)**.
- To run a host command, a Flatpak app uses `flatpak-spawn --host`, which needs the `org.freedesktop.Flatpak` D-Bus name ([flatpak command reference](https://docs.flatpak.org/en/latest/flatpak-command-reference.html)). The Bitwig manifest only grants `--talk-name=org.freedesktop.Notifications`.
- Expect the button to show an error in the Flatpak. Even in the `.deb` Bitwig, `~/.local/bin` is only on `PATH` if the desktop session puts it there **(unverified)**.
- An absolute path, a `.desktop` file launched through `xdg-open`/the OpenURI portal, or D-Bus activation would be more robust.

## 4. Track name and colour in Bitwig on Linux

- CLAP `track-info` has the ID `clap.track-info/1` and the compat ID `clap.track-info.draft/1`.
  - `clap_track_info` carries `flags`, `name`, `color`, `audio_channel_count` and `audio_port_type`, with flags such as `CLAP_TRACK_INFO_HAS_TRACK_NAME` and `CLAP_TRACK_INFO_HAS_TRACK_COLOR`.
  - The plug-in's `changed()` and the host's `get()` are both `[main-thread]`.
  - [clap/ext/track-info.h](https://github.com/free-audio/clap/blob/a47f6badb49d948fd009998f28309cdab78979c9/include/clap/ext/track-info.h)
- clack-extensions 0.2 asks for both IDs (`IDENTIFIERS: &[CLAP_EXT_TRACK_INFO, CLAP_EXT_TRACK_INFO_COMPAT]`), so an older host that only knows the draft ID still works. [clack-extensions 0.2.0 track_info.rs](https://docs.rs/crate/clack-extensions/0.2.0/source/src/track_info.rs)
- Bitwig: "CLAP plug-ins: Added missing track info implementation", listed under "What's New in Bitwig Studio 5.1 [released 06 December 2023]", together with "CLAP: Updated to version 1.1.9". [Bitwig 5.1 release notes](https://downloads.bitwig.com/5.1.7/Release-Notes-5.1.7.html) The note has no platform qualifier. That Linux gets the same host code is **(inference)**. The Flathub build is 6.1.1.
- The Send Plugin reads the name, the colour (ignored when alpha is 0) and the channel count in `read_track()`, on `init` and on `changed()` (`plugin.rs`). Nothing there is platform-specific, so the CLAP build should get Bitwig's track name and colour on Linux as on macOS **(inference)**.
- For the VST3 build, clap-wrapper maps VST3 `IInfoListener::setChannelContextInfos` (channel name and colour) onto `track-info` ([wrapasvst3.cpp](https://docs.rs/crate/clap-wrapper/0.3.1/source/external/clap-wrapper/src/wrapasvst3.cpp)). Whether Bitwig calls `IInfoListener` is **(unverified)**. Combined with §1.4, the CLAP is the build to recommend in Bitwig.

## 5. Summary of work implied (for the follow-up ticket, not decided here)

1. Add a Linux branch to `scripts/bundle-send-plugin.sh` (`.clap` file, `.vst3/Contents/x86_64-linux/*.so`). Build it in the release job and document installing to `~/.clap` and `~/.vst3`.
2. Make the VST3 heartbeat independent of the window on Linux (§1.4), or document "use the CLAP".
3. Find the window font in more places, at least `/usr/share/fonts/dejavu/` for the Flatpak runtime (§2.4).
4. Flatpak Bitwig:
   - document `flatpak override --user --device=shm com.bitwig.BitwigStudio`;
   - make `host_pid` liveness namespace-aware (§3.4);
   - decide what "Open Das-Meter" does in a sandbox (§3.5).
5. Keep the baseview pin until [baseview#340](https://github.com/RustAudio/baseview/pull/340) lands, or test any upgrade in the Flatpak Bitwig under Xwayland.

## 6. To test on a real machine

- `.deb` Bitwig on GNOME/KDE Wayland: the CLAP loads, the window opens and draws, track name and colour arrive, and a Meter appears in the app in each Plug-in Hosting Mode.
- Flatpak Bitwig, as shipped: confirm the app sees nothing, and that `/dev/shm` inside is a private tmpfs (`flatpak run --command=sh com.bitwig.BitwigStudio -c 'ls /dev/shm'`).
- Flatpak Bitwig after `--device=shm`: the Meter appears. Watch for it going Gone (§3.4 PID issue).
- VST3 in Bitwig and in REAPER on Linux: close the window and check whether the Meter goes Gone after 2 s (§1.4).
- The glibc floor: the CLAP built on Ubuntu 24.04 loads in the Flatpak runtime 25.08 and on the oldest distro we claim. **(unverified)**
