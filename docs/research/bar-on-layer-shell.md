# Can the Bar be a layer-shell surface on KDE Plasma and wlroots?

Research for [issue #78](https://github.com/daswerk/Das-Meter/issues/78) (part of [#76](https://github.com/daswerk/Das-Meter/issues/76)). Researched 2026-09-26.

"Layer-shell" is the Wayland protocol `zwlr_layer_shell_v1`, which panels and docks use. A layer surface is anchored to screen edges, sits on a fixed z-layer, and can reserve an *exclusive zone* so that maximised windows end next to it.

## Short answer

**Yes, on KDE Plasma, Sway and Hyprland. It is not possible on GNOME.** It fits the current stack with no new licence risk, but it is real platform work, not a flag on a winit window:

| Question | Answer |
|---|---|
| Compositor support | KWin, Sway (wlroots) and Hyprland all implement **version 5**, the latest. Mutter (GNOME) has **no** implementation. |
| Which crates | **smithay-client-toolkit (SCTK) 0.19.2**, already in `Cargo.lock` through winit, has a `shell::wlr_layer` module. wgpu draws into the layer's `wl_surface` through `create_surface_unsafe` with raw Wayland handles. SCTK ships an example that does this for an xdg window. winit 0.30 has **no** layer-shell support. Two winit PRs for it have been open since 2023 and 2024. |
| Next to winit's windows | **Yes, one process and one event loop.** Wrap winit's own `wl_display` in a guest `wayland-backend::Backend`, give the Bar its own `EventQueue`, and drain that queue in `about_to_wait`. winit skips pointer and keyboard events for surfaces it does not own. A second connection on a thread also works. |
| Exclusive zone | `set_exclusive_zone(thickness)` with the Bar anchored to one edge plus both perpendicular edges. KWin turns it into a strut, which maximised windows respect. |
| Re-anchor at runtime | **Edge:** yes. `set_anchor`, `set_size` and `set_exclusive_zone`, then commit. The compositor sends a new `configure`. **Output:** no request for it. Destroy the layer surface, then create a new `wl_surface` and layer surface on the target `wl_output`. |
| Pointer | Normal `wl_pointer` events. Presses, drags inside the Bar, and right-clicks all work. Menus that go past the strip need an `xdg_popup` with the layer surface as its parent (`get_popup`). |
| Keyboard and **Shift** | Only with keyboard focus. `none` (the default) means no key or modifier events ever. `on_demand` means the compositor gives the Bar focus on click, which takes focus from the DAW. **Showing a grab cursor while Shift is held over an unfocused Bar is not possible on Wayland.** |
| Licences | SCTK, wayland-client, wayland-backend, wayland-protocols-wlr, calloop and xkbcommon-dl are **MIT**. raw-window-handle is MIT/Apache-2.0/Zlib and wgpu is MIT/Apache-2.0. The protocol XML has an HPND-style permissive licence. All of these fit `deny.toml`. |
| GNOME | No layer-shell, and no standard protocol that replaces it. `ext-layer-shell` is still a draft MR. On GNOME the Bar has to use XWayland ([#77](https://github.com/daswerk/Das-Meter/issues/77)) or be a plain floating window. |

---

## The protocol (what the spec guarantees)

Source: `wlr-layer-shell-unstable-v1.xml` as shipped in `wayland-protocols-wlr` 0.3.12 (the version in `Cargo.lock`), identical to [wlr-protocols](https://gitlab.freedesktop.org/wlroots/wlr-protocols/-/blob/master/unstable/wlr-layer-shell-unstable-v1.xml). Rendered at [wayland.app](https://wayland.app/protocols/wlr-layer-shell-unstable-v1). Interface version 5.

- **Creation:** `get_layer_surface(surface, output, layer, namespace)`. `output` may be NULL, and then "the compositor [decides] which output to use". The client must "perform an initial commit without any buffer attached", wait for `configure`, `ack_configure`, and only then attach a buffer. Creating a layer surface from a `wl_surface` that has had a buffer committed is the `already_constructed` error.
- **Layers:** `background`, `bottom`, `top`, `overlay`, bottom-most first. "Traditional shell surfaces will typically be rendered between the bottom and top layers. Fullscreen shell surfaces are typically rendered at the top layer." For the Bar that means **`top`**: above normal windows, and usually below a fullscreen DAW. `set_layer` (since v2) changes it at runtime.
- **State is double-buffered:** "layer, size, anchor, exclusive zone, margin, interactivity" apply on the next `wl_surface.commit`.
- **Anchor and size:** `set_anchor` takes a bitfield of top/bottom/left/right. `set_size(0, h)` means the compositor picks the width, and "You must set your anchor to opposite edges in the dimensions you omit". A full-width top Bar is `anchor = top|left|right`, `size = (0, thickness)`.
- **Exclusive zone:** "Requests that the compositor avoids occluding an area with other surfaces. The compositor's use of this information is implementation-dependent". A positive zone "is only meaningful if the surface is anchored to one edge or an edge and both perpendicular edges". The spec's own example: "a panel might set its exclusive zone to 10, so that maximized shell surfaces are not shown on top of it." `set_exclusive_edge` (v5) is only needed for corner anchoring.
- **Output is fixed.** No request moves a layer surface to another output. `closed` is sent "when the surface will no longer be shown. The output may have been destroyed…". The client "should destroy the resource … and create a new surface if they so choose."
- **Keyboard interactivity** (`set_keyboard_interactivity`):
  - `none` is the default: "the compositor should never assign it the keyboard focus."
  - `exclusive` takes all keyboard input. It is meant for lock screens.
  - `on_demand` (v4): "allow this surface to be focused and unfocused by the user in an implementation-defined manner", "e.g. click to focus". It is "mainly intended for desktop shell components (e.g. panels)".
- **Pointer:** "Layer surfaces receive pointer, touch, and tablet events normally."
- **Popups:** `get_popup(xdg_popup)` makes a layer surface the parent of an `xdg_popup` that was created with a NULL parent. Popups inherit keyboard interactivity.

## Compositor support

| Compositor | Version | Primary source |
|---|---|---|
| KWin (Plasma) | 5 | `static const int s_version = 5;` in [`src/wayland/layershell_v1.cpp`](https://invent.kde.org/plasma/kwin/-/blob/master/src/wayland/layershell_v1.cpp) (master `6f917dc`) |
| wlroots | 5 | `#define LAYER_SHELL_VERSION 5` in [`types/wlr_layer_shell_v1.c`](https://gitlab.freedesktop.org/wlroots/wlroots/-/blob/master/types/wlr_layer_shell_v1.c) (master `380d6c5`) |
| Sway | 5 | `#define SWAY_LAYER_SHELL_VERSION 5` in [`sway/server.c`](https://github.com/swaywm/sway/blob/master/sway/server.c) (master `1652c54`) |
| Hyprland | 5 | `makeUnique<CLayerShellProtocol>(&zwlr_layer_shell_v1_interface, 5, …)` in [`src/managers/ProtocolManager.cpp`](https://github.com/hyprwm/Hyprland/blob/main/src/managers/ProtocolManager.cpp) (main `b36699f`) |
| Mutter (GNOME) | none | No layer-shell file in [`src/wayland/`](https://gitlab.gnome.org/GNOME/mutter/-/tree/main/src/wayland) or `src/wayland/protocol/` (main `888a7b7`). A request for it ([mutter#1922](https://gitlab.gnome.org/GNOME/mutter/-/work_items/1922), 2021) was closed the next day. |

[wayland.app's support table](https://wayland.app/protocols/wlr-layer-shell-unstable-v1) agrees: KWin 6.7, Hyprland 0.52 and niri report 5, Sway 1.11 reports 4 (the release before the define above changed), and Mutter is "x".

Details that matter for the Bar:
- **KWin:** a layer window `hasStrut()` when `exclusiveZone() > 0`, and `strutRect()` reserves `exclusiveZone` from the anchored edge. That is how maximised windows avoid it. Layer windows are not `isMovable()` or `isMovableAcrossScreens()`, so only the client can move them. When a layer surface's output is removed, KWin closes the window, and the client gets `closed`. See [`src/layershellv1window.cpp`](https://invent.kde.org/plasma/kwin/-/blob/master/src/layershellv1window.cpp).
- **KWin focus:** any non-zero interactivity counts as `acceptsFocus`. When that changes on a `top` or `overlay` surface, KWin calls `activateWindow` right away (`handleAcceptsFocusChanged`). So on Plasma, switching the Bar to `on_demand` takes focus immediately. Keep it `none` unless focus is needed.
- **Sway:** clicking a layer surface whose `keyboard_interactive` is set calls `seat_set_focus_layer` **before** the button is forwarded to the client. With focus-follows-mouse, hovering does the same. A press on a layer surface starts `seatop_begin_down_on_surface`, which keeps the pointer on that surface until release (an implicit grab). See [`sway/input/seatop_default.c`](https://github.com/swaywm/sway/blob/master/sway/input/seatop_default.c).
- **Sway sandbox filter:** Sway lists `layer_shell` among the privileged globals it hides from clients in a `security-context` sandbox (Flatpak). The AUR package and the AppImage are not sandboxed, so this does not affect them.
- **No standard replacement is coming soon.** `ext-layer-shell` is still [wayland-protocols MR 28](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/merge_requests/28), marked "Draft" and open. `staging/` has no layer protocol.

---

## Crates: drawing wgpu into a layer surface

### What the repo already has (`Cargo.lock`)

`winit 0.30.13` with the `wayland` feature, enabled on Linux through `egui-winit`'s `wayland` feature in `crates/app/Cargo.toml`, already pulls in:
- `smithay-client-toolkit 0.19.2` (MIT). winit uses it with `default-features = false, features = ["calloop"]`.
- `wayland-client 0.31.15` and `wayland-backend 0.3.17` (both MIT; `client_system`, which links libwayland-client).
- `wayland-protocols 0.32.12` and `wayland-protocols-wlr 0.3.12` (both MIT). The layer-shell bindings come from the second.
- `calloop 0.13.0`, `calloop-wayland-source 0.3.0` and `xkbcommon-dl 0.4.2` (all MIT).
- `raw-window-handle 0.6.2` (MIT OR Apache-2.0 OR Zlib) and `wgpu 30.0.1` (MIT OR Apache-2.0).

Depending on `smithay-client-toolkit = "0.19"` directly adds **no new crates** to the tree.

### SCTK's layer-shell API (0.19.2 source, `src/shell/wlr_layer/mod.rs`)

- `LayerShell::bind(&globals, &qh)` binds `zwlr_layer_shell_v1` at versions **1..=4**. It returns `Err` when the global is missing, and that is the runtime test for "this desktop has layer-shell". Upstream master (0.21.1 is the latest on crates.io) still binds `1..=4`, so SCTK has no `set_exclusive_edge`. The Bar does not need it (it anchors to an edge, not a corner).
- `create_layer_surface(&qh, wl_surface, Layer::Top, Some("das-meter"), Some(&wl_output))` returns a `LayerSurface` with `set_size`, `set_anchor`, `set_exclusive_zone`, `set_margin`, `set_keyboard_interactivity`, `set_layer` and `get_popup(&XdgPopup)`.
- `LayerShellHandler` has `configure(…, LayerSurfaceConfigure { new_size, .. }, serial)` and `closed(…)`. SCTK acks the configure itself.
- Input comes from `seat::pointer` (`PointerEvent { surface, position, kind: Enter/Leave/Motion/Press/Release/Axis }`, and `cursor_shape.rs` for `wp_cursor_shape_v1`) and `seat::keyboard`. The keyboard module is behind SCTK's `xkbcommon` feature. That feature pulls in the `xkbcommon` crate (MIT, links the MIT-licensed libxkbcommon at build time). winit instead loads it at run time through `xkbcommon-dl`, so the Bar could decode modifiers with xkbcommon-dl too.
- `OutputState` binds `zxdg_output_v1` and gives each output a `logical_position` and `logical_size`. The Bar needs these to decide which edge of which output a drag ends on.

### Handing the surface to wgpu

SCTK's own [`examples/wgpu.rs`](https://github.com/Smithay/client-toolkit/blob/master/examples/wgpu.rs) (0.19.2) builds the handles by hand and calls wgpu:

```rust
let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
    NonNull::new(conn.backend().display_ptr() as *mut _).unwrap()));
let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
    NonNull::new(window.wl_surface().id().as_ptr() as *mut _).unwrap()));
let surface = unsafe { instance.create_surface_unsafe(
    wgpu::SurfaceTargetUnsafe::RawHandle { raw_display_handle, raw_window_handle }) };
```

The example uses an xdg window, but the same code works for a layer surface: wgpu only sees the `wl_surface`. In wgpu 30, `SurfaceTargetUnsafe::RawHandle`'s safety rule says the handles "must remain valid until after the returned" surface is dropped. So drop the `wgpu::Surface` before destroying the `LayerSurface`, for example when moving to another output. If the `Instance` was created with a display handle, the surface's display handle "must be identical". Das-Meter builds one `Instance` per window (`Gpu::for_window` in `crates/app/src/gpu.rs`), so that is no problem. The existing Meter renderers, glyphon and egui-wgpu paint into the `wgpu::Surface` as they do now.

**egui input is manual.** `egui-winit` turns winit `WindowEvent`s into `egui::RawInput`. For the layer surface, Das-Meter has to build `RawInput` from SCTK pointer and keyboard events itself (pointer position ÷ scale, buttons, scroll, modifiers). The scale also comes from Wayland (`wl_surface.preferred_buffer_scale` or `wp_fractional_scale_v1` with `wp_viewporter`), not from winit.

### Other crates considered

| Crate | Licence | Why not (as the base) |
|---|---|---|
| winit PRs [#2832](https://github.com/rust-windowing/winit/pull/2832), [#4044](https://github.com/rust-windowing/winit/pull/4044) (issue [#2582](https://github.com/rust-windowing/winit/issues/2582)) | Apache-2.0 | Both still **open**. #4044's author says they are waiting on the maintainers (last activity September 2026). Using it means shipping a winit fork. A maintainer wrote in #2582 that with layer-shell "keyboard input will work strangely and mouse input as well". |
| `layershellev` / `iced_layershell` 0.19.1 ([waycrate/exwlshelleventloop](https://github.com/waycrate/exwlshelleventloop)) | MIT | "winit like binding for layershell", but it **replaces** the event loop. It cannot share winit's `run_app`, and it is built around iced. |
| `wayapp` 0.3.2 ([Ciantic/wayapp](https://github.com/Ciantic/wayapp)) | MIT (repo); crates.io says "non-standard" | egui + wgpu + SCTK with no winit. Useful as reference code for building egui `RawInput` from SCTK events. It is small and young, so not a dependency. |
| `gtk4-layer-shell` 0.8.1 | MIT | GTK. Not our stack. |

---

## Living next to winit's windows (Pop-outs, Window mode, settings, menus)

winit 0.30.13 keeps drawing the other windows as xdg toplevels. The Bar is a separate SCTK object graph on the **same** Wayland connection:

1. **Share winit's `wl_display`.** `ActiveEventLoop` implements `HasDisplayHandle`, which gives the `wl_display*`. `wayland_backend::client::Backend::from_foreign_display(ptr)` (unsafe, `client_system`) wraps it in "guest" mode, which "will not close the connection on drop". Its documented purpose: "a library that is expected to plug itself into an existing Wayland connection". Then `Connection::from_backend`, `registry_queue_init` gives our own `EventQueue<BarState>`, and we bind `LayerShell`, `CompositorState`, `OutputState`, `SeatState` and `XdgShell` for popups on it.
2. **Dispatch.** winit reads the socket through `calloop-wayland-source`. That calls libwayland's `wl_display_read_events` (`wayland-backend` `sys/client_impl`), which puts each event into its object's own queue. So the Bar's events arrive whenever winit wakes for the socket. Drain them with `bar_queue.dispatch_pending(&mut bar_state)` and flush in `ApplicationHandler::about_to_wait`. calloop-wayland-source flushes before sleeping too. Wake-ups for the Bar's own redraws use the existing `EventLoopProxy` user event.
3. **winit ignores the Bar's input.** winit binds its own `wl_seat`/`wl_pointer`/`wl_keyboard`. Each bound object gets the events, so both winit and the Bar see enter and motion on the Bar. winit's `pointer_frame` looks up the surface's window and does `None => continue` for unknown surfaces (`platform_impl/linux/wayland/seat/pointer/mod.rs`). The keyboard handler does the same (`None => return`).
4. **Alternative:** a second `Connection::connect_to_env()` on its own thread, blocking in `blocking_dispatch`, sending messages to the winit thread through the proxy. This is simpler to reason about, but it doubles the globals and needs locking around shared Bar state. Surfaces on different connections cannot parent each other. That is fine here, because the Bar's popups live on the Bar's connection either way.

**What changes around the Bar on Wayland (both KDE and wlroots):**
- The Meter **menu** and the first-launch **Card** are separate winit windows placed with `with_position`/`set_outer_position` (`shell.rs`). winit documents both as "**Wayland:** Unsupported", and `WindowLevel` as "Wayland: Unsupported" too. Next to a layer-shell Bar they have to become **`xdg_popup`s with the layer surface as parent** (`XdgShell` popup plus `LayerSurface::get_popup`), positioned with an `xdg_positioner` relative to the Bar. Otherwise the Card could be a second small layer surface anchored to the same edge with a margin.
- This does not affect Pop-outs and Window mode (ordinary toplevels). They just cannot be placed by the app on Wayland at all (a separate question, see [#83](https://github.com/daswerk/Das-Meter/issues/83)).

---

## Exclusive zone and re-anchoring (Shift-drag)

- **Dock:** for edge `E` with thickness `t`, set `anchor = E | both perpendicular edges`, `size = t` across and `0` along the edge, and `exclusive_zone = t` (margin included, per spec). Commit, wait for `configure`, and use its size.
- **Another edge, same output:** send `set_anchor`, `set_size` and `set_exclusive_zone` together and commit once. The state is double-buffered, so the switch is atomic. Resize the swapchain on the next `configure`.
- **Another output:** create a new `wl_surface` and layer surface with that `wl_output`, then drop the old `wgpu::Surface` and `LayerSurface`. The new `wl_surface` is needed because the old one already has committed buffers (the `already_constructed` rule). Do the same on `closed` (output unplugged): pick another output and recreate.
- **Dragging:** the compositor will not move a layer surface. KWin's `isMovable()` is `false`, and there is no `xdg_toplevel.move` for layer surfaces. The Bar tracks the drag itself:
  - On press, Sway (and, by convention, other compositors) holds an implicit grab. Motion keeps coming to the Bar in **surface-local** coordinates, even outside it. Add the Bar's position on its output (known from its anchor and the output's `logical_position`/`logical_size` from xdg-output) to get a layout position. Pick the nearest edge of the output under it, and re-anchor on release.
  - The snap **preview** can be a temporary `overlay` layer surface on the target output, anchored to that edge with `exclusive_zone = -1` and no input region. Wayland never gives a global pointer position outside such surfaces, so this is the only way to show one.
  - The spec does not guarantee that motion keeps coming during the grab on every compositor. A drag across outputs needs a test on KWin and Hyprland. The robust fallback is a wl_data_device drag. `start_drag` requires "an active implicit grab", and drop-target events arrive on the preview surfaces of each output.

## Pointer and keyboard input

- **Clicks, drags inside the Bar, scroll, right-click:** all normal `wl_pointer` events on the layer surface. The Meter's right-click menu works once it is an `xdg_popup` (above).
- **Cursor:** set it on `wl_pointer.enter` with `wp_cursor_shape_v1` (SCTK `cursor_shape.rs`) or a themed cursor. This is per surface, so it is independent of winit.
- **Shift (the modifier):** `wl_pointer` button events carry no modifiers. The Bar only learns them from `wl_keyboard.modifiers`, which it only gets while it has **keyboard focus**. The core spec says: "The compositor must send the wl_keyboard.modifiers event after [wl_keyboard.enter]" ([wayland.xml](https://gitlab.freedesktop.org/wayland/wayland/-/blob/main/protocol/wayland.xml), `wl_keyboard.enter`, as shipped in wayland-client 0.31.15). The options:
  1. **`none`** (default, recommended for normal use): the Bar never takes focus from Bitwig. It never sees Shift, so Shift-drag and the "grab cursor while Shift is held" hint cannot work.
  2. **`on_demand`**: the click gives the Bar focus. On Sway, focus is set before the press is forwarded, so `enter` + `modifiers` arrive first and Shift+press can be detected. KWin sends focus the moment the flag is set (above). **Cost:** every click on the Bar takes keyboard focus from the DAW, so its shortcuts stop working until the user clicks back. The "Shift held while hovering" hint still cannot work before the first click.
  3. **A Wayland-specific gesture** that needs no modifier: a drag on a grip or end-cap, a long-press, or "Move Bar to…" in the right-click menu. This keeps `none`.
  This conflict is a decision for [#83](https://github.com/daswerk/Das-Meter/issues/83), not something code can remove. Unfocused Pop-outs (winit xdg windows) have the same limit on Wayland, because winit also gets modifiers only for the focused window.
- **Keyboard in menus:** an `xdg_popup` inherits the Bar's interactivity. Use a popup grab (`xdg_popup.grab` with the press serial) to get keys while the menu is open, or leave menus mouse-only.

## GNOME

Mutter implements neither `zwlr_layer_shell_v1` nor any other protocol for docking a client surface, and a native Wayland client there cannot set a position or reserve space (winit: `set_outer_position` "Wayland: Unsupported"). The Linux map's fallback stays: XWayland ([#77](https://github.com/daswerk/Das-Meter/issues/77)) if it works, otherwise a floating window the user places. The layer-shell path should be chosen at run time: **if `LayerShell::bind` succeeds**, the Bar is a layer surface; if not, it falls back to the GNOME path. That also covers other desktops without layer-shell.

## Recommendation

1. Build the Linux Bar as an SCTK layer surface in a `linux/layer_bar` module, sharing winit's `wl_display` in guest mode and draining its queue in `about_to_wait`. Add `smithay-client-toolkit = "0.19"` (same version as winit, no new crates). Detect support with `LayerShell::bind`.
2. `Layer::Top`, anchored to the edge plus both perpendicular edges, `exclusive_zone = thickness`, `KeyboardInteractivity::None`. Recreate on output change and on `closed`.
3. Move the Meter menu and the Card to `xdg_popup`s parented with `get_popup`. Feed egui `RawInput` from SCTK events, and handle scale through fractional-scale.
4. Resolve Shift-drag on Wayland in #83: without keyboard focus the Bar never sees Shift, so either accept focus stealing (`on_demand`) or use a modifier-free gesture.
5. Before committing to this, **prototype** a layer Bar next to one winit window on KWin, Sway and Hyprland: guest-display dispatch, exclusive zone, re-anchoring, a drag across two outputs, and a popup.

## Sources checked

- Protocol: `wlr-layer-shell-unstable-v1.xml` from `wayland-protocols-wlr` 0.3.12 (= wlr-protocols master). `wayland.xml` from `wayland-client` 0.31.15 (`wl_keyboard.enter`, `wl_pointer`, `wl_data_device.start_drag`). [wayland.app layer-shell page](https://wayland.app/protocols/wlr-layer-shell-unstable-v1) for the support table. wayland-protocols `staging/` listing and [MR 28](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/merge_requests/28).
- Compositor source: KWin master `6f917dc` (`src/wayland/layershell_v1.cpp`, `src/layershellv1window.cpp`). wlroots master `380d6c5`. Sway master `1652c54` (`sway/server.c`, `sway/input/seatop_default.c`). Hyprland main `b36699f` (`src/managers/ProtocolManager.cpp`). Mutter main `888a7b7` (`src/wayland/` tree), [mutter#1922](https://gitlab.gnome.org/GNOME/mutter/-/work_items/1922).
- Crate source (versions from `Cargo.lock`): smithay-client-toolkit 0.19.2 (`src/shell/wlr_layer/mod.rs`, `src/seat/`, `src/output.rs`, `examples/wgpu.rs`, `Cargo.toml`), and upstream master `e97622a` for the bound version. winit 0.30.13 (`Cargo.toml`, `src/window.rs`, `src/event_loop.rs`, `platform_impl/linux/wayland/`). wayland-backend 0.3.17 (`src/sys/mod.rs`, `src/sys/client_impl/mod.rs`). wayland-client 0.31.15. calloop-wayland-source 0.3.0. wgpu 30.0.1 (`src/api/surface.rs`, `src/api/instance.rs`).
- Latest versions and licences from the crates.io API (2026-09-26): smithay-client-toolkit 0.21.1 MIT, winit 0.30.13 / 0.31.0-beta.3 Apache-2.0, layershellev 0.19.1 MIT, xkbcommon 0.9.0 MIT, gtk4-layer-shell 0.8.1 MIT, wayapp 0.3.2.
- winit issue [#2582](https://github.com/rust-windowing/winit/issues/2582), PRs [#2832](https://github.com/rust-windowing/winit/pull/2832) and [#4044](https://github.com/rust-windowing/winit/pull/4044). SCTK issue [#349](https://github.com/Smithay/client-toolkit/issues/349) (request for an egui layer-shell example, open).
- **Not tested on hardware.** Everything above comes from specs and source. The behaviour marked "test" (guest-display dispatch, motion during a cross-output drag, KWin's focus timing) needs the prototype in step 5.
