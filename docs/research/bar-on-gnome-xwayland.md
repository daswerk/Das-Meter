# Research: the Bar on GNOME through XWayland

Ticket: [#77](https://github.com/daswerk/Das-Meter/issues/77) (part of the Linux map, [#76](https://github.com/daswerk/Das-Meter/issues/76)). Researched 2026-09-26.

The question: on GNOME 47 or newer under Wayland, can the Bar run as an X11 client through XWayland and still behave like a Bar? That means sitting at an exact screen edge on a chosen display, staying above other windows, reserving its strip, getting multi-display coordinates right, staying sharp at 125 % and 150 %, choosing the X11 backend only on GNOME, and being movable with Shift-drag.

**How sources were checked.** Mutter facts come from its source: `main` at commit [`888a7b7`](https://gitlab.gnome.org/GNOME/mutter/-/tree/888a7b7dac0c58612007c46bbad75de9f9437f48) (version 51.0, 2026-09-17), checked against the tags `47.0`, `48.0`, `49.0` and `50.0`. Line links point at `888a7b7` unless a tag is named. winit facts come from the winit 0.30.13 crate source, which is the version in `Cargo.lock`. gnome-settings-daemon facts come from its `main` (51.0). EWMH quotes come from the [freedesktop spec](https://specifications.freedesktop.org/wm/latest/ar01s05.html). Nothing here was run on a GNOME desktop. Behaviour marked **(untested)** is what the source says should happen.

## Short answer

**Yes: through XWayland, the Bar can be a real docked Bar on GNOME.** Make it an X11 `_NET_WM_WINDOW_TYPE_DOCK` window and set `_NET_WM_STRUT_PARTIAL`. Mutter handles X11 windows the same way whether they come from Xorg or XWayland: `MetaWindowXwayland` is a subclass of `MetaWindowX11`, and the strut code has no Wayland check.

| Question | Answer |
|---|---|
| Exact edge on a chosen display | **Yes.** Docks skip Mutter's placement code and its "keep it on screen" constraints. Client moves (`XConfigureWindow`, which winit's `set_outer_position` sends) are honoured by default. |
| Kept above other windows | **Yes.** A DOCK window sits in `META_LAYER_DOCK`, which is the same level as `_NET_WM_STATE_ABOVE`. It drops to the bottom layer while its monitor shows a fullscreen window, which is what the Bar wants. |
| Reserve the strip (`_NET_WM_STRUT_PARTIAL`) | **Yes, on outer edges.** Mutter reads the property for XWayland clients and takes the strut out of the work area, so maximised windows leave room. **The limit:** EWMH struts are measured from the edge of the whole X screen, so a Bar on an *inner* edge (between two displays) cannot reserve space. That is a limit of the protocol, not a Mutter bug. |
| Multi-display coordinates | **Consistent.** XWayland's root window uses GNOME's logical layout times one integer scale, and struts, moves and RandR monitor rectangles all use that same space. |
| Fractional scaling (125 %, 150 %) | **GNOME 50 and newer: sharp.** The X11 client renders at 2× (the rounded-up highest scale) and Mutter scales it down. **GNOME 47–49:** fractional scaling is an experimental flag that is off by default. With it on, X11 windows are upscaled and blurry unless the user also turns on `xwayland-native-scaling`. |
| winit X11 backend on GNOME only | **Yes.** winit 0.30.13 has `EventLoopBuilderExtX11::with_x11()`, chosen at run time before the event loop is built. The whole process then uses X11, so Pop-outs do too. |
| Shift-drag moves | **App-driven moves work; `drag_window()` does not work on a DOCK.** Mutter ignores `_NET_WM_MOVERESIZE` (what `drag_window` sends) for docks. Moving the window with `set_outer_position` from the app's own drag handling works (untested). |

---

## 1. Placing the Bar at an exact edge

- **Docks skip placement.** Mutter's placement code treats docks as windows where the app knows best how to place them, "no placement algorithm ever" ([place.c L871–L886](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/core/place.c#L871-886)). A NORMAL window with a program- or user-specified position is not placed either ([place.c L937–L948](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/core/place.c#L937-948)). winit writes `USPosition` into `WM_NORMAL_HINTS` whenever `WindowAttributes::with_position` is set (winit 0.30.13 `src/platform_impl/linux/x11/window.rs` L442–L446).
- **Docks skip the on-screen constraints.** "We only apply the various onscreen requirements to normal windows" ([constraints.c L676–L679](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/core/constraints.c#L676-679)). The single-monitor constraint also skips docks because "we don't want docks to be shoved 'onscreen' by their own strut" ([constraints.c L1857–L1864](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/core/constraints.c#L1857-1864)). So a DOCK Bar keeps the exact rectangle it asks for, inside its own strut.
- **Moves after mapping are honoured.** On an X11 `ConfigureRequest`, Mutter allows the position change unless the `disable-workarounds` preference is on (it is off by default) or a mouse grab is in progress ([window-x11.c L2783–L2805](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/x11/window-x11.c#L2783-2805)). winit's X11 `set_outer_position` is a plain `ConfigureWindow` with x and y (winit `x11/window.rs` L1208–L1239).
- **No frame.** Docks get frame type `META_FRAME_TYPE_LAST`, which means no decorations ([window.c L7324 on](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/core/window.c#L7324)). They are also sticky on every workspace (`always_sticky`, [window.c L6111–L6114](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/core/window.c#L6111-6114)) and kept out of the taskbar and pager ([window.c L6005 on](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/core/window.c#L6005)).
- **From winit:** `WindowAttributesExtX11::with_x11_window_type(vec![WindowType::Dock])` sets `_NET_WM_WINDOW_TYPE` (winit `src/platform/x11.rs`). The type is set before mapping, which is when Mutter reads it.
- **GNOME Shell's top bar is a strut too.** Shell registers it with `meta_workspace_set_builtin_struts` ([workspace.c L1076 on](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/core/workspace.c#L1076)), and Shell's chrome is drawn above every window. A top-edge Bar at y = 0 would sit under the panel. Place the Bar inside the work area Mutter publishes *before* the Bar sets its own strut: `_NET_WORKAREA`, or per monitor `_GTK_WORKAREAS_D<n>` ([meta-x11-display.c L994, L1049](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/x11/meta-x11-display.c#L994)). Or subtract the gap between the monitor rectangle and that work area.

## 2. Staying on top and handling fullscreen

- Layer rules ([window.c L6539–L6555](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/core/window.c#L6539-6555)): `wm_state_above` puts a window in `META_LAYER_TOP` (but not while it is maximised). A DOCK goes in `META_LAYER_DOCK`, or in `META_LAYER_BOTTOM` while "`window->monitor->in_fullscreen`". `META_LAYER_TOP` and `META_LAYER_DOCK` are both 4 ("Same as DOCK; see EWMH", [meta-enums.h L269–L271](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/meta/meta-enums.h#L269-271)).
- So a DOCK Bar stays above normal windows and gets out of the way of a fullscreen DAW on its own monitor only. This is the behaviour Windows' `ABN_FULLSCREENAPP` asks for (see [bar-screen-space.md](bar-screen-space.md)), without any extra code.
- winit's `set_window_level(AlwaysOnTop)` toggles `_NET_WM_STATE_ABOVE` (winit `x11/window.rs` L1074). It is harmless on a dock but not needed.
- **Focus:** Mutter does not focus a dock on click: "Don't focus panels--they must explicitly request focus" ([window.c L7827](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/core/window.c#L7827)). Docks also never take focus when they are mapped ([window.c L2160 on](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/core/window.c#L2160)). The Bar's clicks and right-click menus still arrive, because X delivers pointer events without focus. Anything in the Bar that needs the keyboard, such as a text field, would need an explicit focus request (`_NET_ACTIVE_WINDOW`, or winit `focus_window`) **(untested)**.

## 3. Reserving the strip: `_NET_WM_STRUT_PARTIAL`

**Mutter honours it for XWayland clients.**

- The property is watched for every X11 window, and a change reloads the struts ([window-props.c L1728–L1729](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/x11/window-props.c#L1728-1729)). `meta_window_x11_update_struts` reads the 12 cardinals and turns each non-zero side into a strut rectangle ([window-x11.c L1545–L1637](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/x11/window-x11.c#L1545-1637)). If `_NET_WM_STRUT_PARTIAL` is missing, it falls back to `_NET_WM_STRUT`.
- This is the X11 window class, and XWayland windows are `G_DEFINE_TYPE (MetaWindowXwayland, meta_window_xwayland, META_TYPE_WINDOW_X11)` ([meta-window-xwayland.c L62](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/wayland/meta-window-xwayland.c#L62)). No code path checks whether the session is Wayland.
- The strut coordinates go through `meta_window_protocol_to_stage_point`, which converts XWayland's scaled coordinates to GNOME's logical ones (begin/end rounded to shrink, thickness rounded to grow). So struts stay right under scaling (§5).
- Each workspace collects every window's struts, plus Shell's built-in ones, and computes the work area per logical monitor and for the whole screen ([workspace.c L880–L918](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/core/workspace.c#L880-918)). Maximising uses that work area. A DOCK is on every workspace, so its strut applies everywhere.
- If the struts leave less than 100 px, Mutter logs "struts occupy an unusually large percentage of the screen" and ignores part of them ([workspace.c L932–L967](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/core/workspace.c#L932-967)). This does not matter for a Bar.

**The multi-display limit.** EWMH: "Struts MUST be specified in root window coordinates, that is, they are *not* relative to the edges of any view port or Xinerama monitor". Its example: a bottom panel on the right-hand, shorter monitor "should set a bottom strut of 306 … Note that the strut is relative to the screen edge, and not the edge of the xinerama monitor" ([EWMH §5.10](https://specifications.freedesktop.org/wm/latest/ar01s05.html)). Mutter builds each strut from the edge of the whole display ([window-x11.c L1600–L1620](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/x11/window-x11.c#L1600-1620)). So:

- **Works:** any edge of a display that lies on the outside of the combined layout. Examples: the bottom of either display when two are side by side with aligned bottoms, the left edge of the left display, the right edge of the right display. `*_start`/`*_end` limit the strut to the Bar's own display.
- **Also works, with a quirk:** the bottom of the shorter display when bottoms are not aligned. The strut thickness must count from the bottom of the whole screen (the "306" in the spec example).
- **Does not work:** an edge shared with another display, such as the right edge of the left display. To reach it from the screen edge, the strut would have to cover the whole neighbouring display in that range. For an inner edge, the Bar can only be an overlay that stays on top but reserves nothing, like the macOS fallback. The freedesktop wm-spec list discussed a fix (`_NET_WM_STRUT_AREA`, [wm-spec-list, Jan 2020](https://mail.gnome.org/archives/wm-spec-list/2020-January/msg00000.html)), but it is not in the spec.

**From Rust:** winit has no strut API. Set the property with `x11rb` (MIT OR Apache-2.0, already in `Cargo.lock` as 0.13.2 through winit): `change_property32(Replace, window, _NET_WM_STRUT_PARTIAL, CARDINAL, &[l, r, t, b, l_start, l_end, r_start, r_end, t_start, t_end, b_start, b_end])`. Get the X window ID from winit's `raw-window-handle` (`RawWindowHandle::Xlib`/`Xcb`). Setting a property from a second connection is ordinary X11. Update it whenever the Bar's edge, display, thickness or length changes, and clear it (all zeros, or delete the property) before hiding the Bar.

## 4. Multi-display coordinates

- Mutter tells XWayland (through `xdg_output`) each logical monitor's position and size multiplied by the XWayland "effective scale" ([meta-wayland-outputs.c L206–L222, L711–L722](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/wayland/meta-wayland-outputs.c#L206-222)). Window positions go the other way with the same factor ([meta-window-xwayland.c L410–L500](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/wayland/meta-window-xwayland.c#L410-500)). So the X root space is **GNOME's logical layout × one integer**. RandR monitor rectangles, window positions and struts all agree, and a display's edge is an exact integer in X coordinates.
- Mutter also mirrors the primary display into XWayland's RandR state (`meta_xwayland_set_primary_output`, [meta-xwayland.c L1273 on](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/wayland/meta-xwayland.c#L1273)).
- winit on X11 lists monitors through RandR. Their `position()`/`size()` are in that X space (physical in winit's terms). Placing the Bar in physical X coordinates (`PhysicalPosition`) avoids a round trip through winit's scale factor.
- The effective scale is one number for all X clients (§5), not one per monitor. With mixed scales (a 100 % display next to a 150 % one), every X11 window renders at the same scale. It stays sharp, but the 100 % display does extra GPU downscaling.
- Related Mutter bug: [#4841](https://gitlab.gnome.org/GNOME/mutter/-/work_items/4841), a 1 px seam between the top bar and maximised *Wayland* windows at fractional scale. It is filed for Wayland clients. Whether XWayland windows show the same seam is **(untested)**.

## 5. Fractional scaling (125 %, 150 %)

This depends on the GNOME version.

| Mutter | Fractional scales available | XWayland "effective scale" | What an X11 window looks like at 125/150 % |
|---|---|---|---|
| 47, 48, 49 | Only when the user adds `scale-monitor-framebuffer` to `org.gnome.mutter experimental-features`. Without it the layout is *physical* and only integer scales exist (`META_MONITOR_SCALES_CONSTRAINT_NO_FRAC`, [47.0 meta-monitor-manager-native.c L453–L454](https://gitlab.gnome.org/GNOME/mutter/-/blob/47.0/src/backends/native/meta-monitor-manager-native.c#L453-454)) | `ceil(highest monitor scale)` only if **both** `scale-monitor-framebuffer` and `xwayland-native-scaling` are enabled, otherwise 1 ([47.0 meta-xwayland.c L1338–L1361](https://gitlab.gnome.org/GNOME/mutter/-/blob/47.0/src/wayland/meta-xwayland.c#L1338-1361)) | With native scaling: rendered at 2× and scaled down, sharp. Without it: rendered at 1× and **upscaled, blurry** |
| 50 and newer (incl. `main`/51) | Always. The layout is always logical ([50.0 meta-monitor-manager-native.c L485](https://gitlab.gnome.org/GNOME/mutter/-/blob/50.0/src/backends/native/meta-monitor-manager-native.c#L485)). NEWS 50.beta: "Make VRR and fractional scaling non-experimental (!4863, !4877)". The `scale-monitor-framebuffer` flag is gone from the schema | Always `ceil(highest monitor scale)`, or the new `org.gnome.mutter.wayland xwayland-scaling-factor` key if set ([meta-xwayland.c L1368–L1394](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/wayland/meta-xwayland.c#L1368-1394)) | **Sharp**: renders at 2× and is downscaled by 1.6 (125 %) or 1.33 (150 %) |

The native-scaling feature arrived in 47.rc: "Let scaling-aware Xwayland clients scale themselves (!3567)" (Mutter NEWS).

**How winit learns the scale.** Mutter publishes the X11 UI scaling factor (the effective scale) as the D-Bus property `org.gnome.Mutter.X11.UiScalingFactor` ([meta-xwayland.c L1397–L1414](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/wayland/meta-xwayland.c#L1397-1414); [meta-x11-display.c L198–L211](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/x11/meta-x11-display.c#L198-211)). gnome-settings-daemon reads it and publishes XSETTINGS `Xft/DPI` = 96 × text-scaling-factor × window-scale × 1024, and `Xft.dpi` in `RESOURCE_MANAGER` ([gsd-xsettings-manager.c L597–L630, L666–L668, L742, L798](https://gitlab.gnome.org/GNOME/gnome-settings-daemon/-/blob/main/plugins/xsettings/gsd-xsettings-manager.c#L597-630)). winit 0.30.13's X11 scale factor is `Xft/DPI` from XSETTINGS, then `Xft.dpi`, then a RandR guess, divided by 96. It is the same for every monitor, and `WINIT_X11_SCALE_FACTOR` overrides it (winit `x11/util/randr.rs` L41–L53, L104–L150). So on GNOME 50 at 150 %, the Bar sees a scale factor of 2.0 (times the accessibility text scale, if the user set one), and egui renders at 2×.

**What to expect:**
- GNOME 50+: text and meter edges are sharp. A 1 physical-px line at scale 2 lands on 0.75 device px at 150 %. Hairlines may look softer than on a native Wayland window, but not blurry.
- GNOME 47–49 with fractional scaling turned on by hand: blurry unless the user also enables `xwayland-native-scaling`. Worth a line in the Linux docs.
- The accessibility "Large Text" setting also raises winit's X11 scale factor, because it goes into `Xft/DPI`. The Bar would grow with it. Probably fine, but worth knowing.

## 6. Choosing the X11 backend only on GNOME

- winit 0.30.13 picks the backend when `EventLoop` is built: a forced backend first, then Wayland if `WAYLAND_DISPLAY`/`WAYLAND_SOCKET` is set, then X11 if `DISPLAY` is set (`src/platform_impl/linux/mod.rs` L735–L775). There is **no environment variable** to force it in 0.30 (`WINIT_UNIX_BACKEND` no longer exists in the source).
- Force it with `winit::platform::x11::EventLoopBuilderExtX11::with_x11(&mut builder)` (`src/platform/x11.rs` L114–L131). `EventLoopExtX11::is_x11()` reports what was chosen. This is a run-time decision, so `shell.rs`'s `EventLoop::<()>::with_user_event().build()` becomes:

  ```rust
  let mut builder = EventLoop::<()>::with_user_event();
  #[cfg(target_os = "linux")]
  if linux::wants_x11() { // GNOME on Wayland, with DISPLAY set
      use winit::platform::x11::EventLoopBuilderExtX11;
      builder.with_x11();
  }
  let event_loop = builder.build()?;
  ```

  A good `wants_x11()`: `XDG_SESSION_TYPE == "wayland"`, `XDG_CURRENT_DESKTOP` contains `GNOME` (a colon-separated list, for example `ubuntu:GNOME`), and `DISPLAY` is set and not empty. Add a user override (a setting or environment variable) so people can go back to native Wayland.
- One backend per process: Pop-outs and dialogs also become X11. With `rfd`'s portal backend, file dialogs still come from the portal.
- The crate already builds both backends: `winit = "0.30.13"` with default features includes `x11` and `wayland`, and `egui-winit` enables both on Linux (`crates/app/Cargo.toml`). No new dependencies are needed apart from `x11rb` for the strut, which is already in the tree and MIT/Apache-2.0.
- XWayland must be available. Mutter starts it on demand unless it was run with `--no-x11` ([meta-context-main.c L347–L353](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/core/meta-context-main.c#L347-353)). If `DISPLAY` is unset, fall back to native Wayland and the floating Bar.
- Native Wayland for comparison: winit's Wayland `set_outer_position` is a no-op ("Not possible on Wayland") and `set_window_level` is empty (`src/platform_impl/linux/wayland/window/mod.rs` L273, L430). That is why XWayland is the only way to get a docked Bar on GNOME without a Shell extension.

## 7. Shift-drag moves

Two ways to move the Bar, and on GNOME/XWayland only one of them works for a dock:

1. **WM-driven (`winit::Window::drag_window`)** sends `_NET_WM_MOVERESIZE` (winit `x11/window.rs` L1661–L1720). Mutter starts the move only `if window->has_move_func` ([window-x11.c L3519–L3557](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/x11/window-x11.c#L3519-3557)). For frameless window types, which include DOCK, `has_move_func = FALSE`: "this keeps panels and things from using NET_WM_MOVERESIZE" ([window.c L6116–L6131](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/core/window.c#L6116-6131)). **`drag_window` does nothing on a DOCK Bar.** It would work on a NORMAL Pop-out. While Mutter runs such a drag, it also ignores the app's own `ConfigureRequest` positions (`in_grab_op`), so the app could not snap during it.
2. **App-driven (`set_outer_position` from the app's own pointer handling):** this works for docks (§1). While the left button is held, X11's implicit pointer grab keeps sending motion events to the window after the pointer leaves it. Under XWayland this also needs Wayland's implicit grab, which Mutter provides for the pressed surface **(untested)**. Positions come relative to the window, so add the Bar's known X position, or ask the server with `x11rb` `query_pointer` on the root window for screen coordinates.

For the planned design (Shift held, drag, the Bar snaps to the nearest edge or display with a preview), the app-driven path fits better on every platform. The Bar picks the target edge itself, shows the preview, and on release moves the window with `set_outer_position` and resets the strut. A Bar that follows the pointer while it moves needs no WM help. Clear the strut while dragging, so maximised windows do not reflow on every step.

## 8. What CI can test without a desktop

These are proposals and have not been tried:

- **Unit-testable now:** the `wants_x11()` decision (from environment values), the maths from Bar edge, display and thickness to the 12 strut cardinals (including the "count from the whole-screen edge" rule, and "inner edge ⇒ no strut"), and the snap-to-edge maths.
- **Headless Mutter in an Arch container:** Mutter runs as a headless compositor with virtual monitors: `mutter --wayland --headless --virtual-monitor 1920x1080 --virtual-monitor 1920x1080` (options in [meta-context-main.c L340–L375](https://gitlab.gnome.org/GNOME/mutter/-/blob/888a7b7dac0c58612007c46bbad75de9f9437f48/src/core/meta-context-main.c#L340-375)). `gdctl` (Mutter ≥ 48, `tools/gdctl`) can set logical-monitor scales to 1.25 or 1.5 through D-Bus. Then start the app with the XWayland `DISPLAY` and check with `xprop -root _NET_WORKAREA _GTK_WORKAREAS_D0`, `xprop -id <bar> _NET_WM_STRUT_PARTIAL _NET_WM_WINDOW_TYPE`, `xwininfo` for position, and `xrandr --listmonitors` for the scaled monitor space. That covers placement, strut and scale end to end, without GNOME Shell (so without Shell's top-bar strut) and without looking at pixels.
- Sharpness still needs the owner's eyes on real GNOME at 125 % and 150 %.

## Checklist for the owner's manual test (Arch + GNOME)

1. `gnome-shell --version` (so we know which row of §5 applies) and `gsettings get org.gnome.mutter experimental-features`.
2. Bar at the bottom of the main display: maximise Bitwig and check it ends at the Bar.
3. Bar on the top edge: check it sits below the Shell top bar and not under it.
4. Two displays: Bar on an outer edge (reserves space) and on the inner edge (overlay only, as expected).
5. Bitwig fullscreen on the Bar's display: the Bar should go behind it. On the other display it should stay.
6. 125 % and 150 %: screenshot the Bar next to a native GTK4 app.
7. Shift-drag the Bar to another edge and another display.
