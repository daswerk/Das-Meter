# Can the Bar reserve screen space on macOS and Windows?

Research for [issue #13](https://github.com/daswerk/Das-Meter/issues/13). Researched 2026-09-24.

"Reserve screen space" here means: when the Bar is docked to a screen edge, maximised windows (for example the DAW) shrink so they end next to the Bar instead of going under it.

## Short answer

| | Windows | macOS |
|---|---|---|
| Reserve space the way the system does it | **Yes.** Register the Bar as an *appbar* with `SHAppBarMessage`. The system takes the Bar's strip out of the monitor's work area, and maximised windows respect it. | **No.** No public API changes `NSScreen.visibleFrame`. Only the Dock and menu bar change it. |
| Next best option | (not needed) | **Overlay only:** the Bar floats on top and covers part of other windows. As an opt-in, the Bar can **move and resize other apps' windows through the Accessibility API**. That needs the user to grant the Accessibility permission, and it rules out the App Sandbox and so the Mac App Store. |
| Always on top | `HWND_TOPMOST` (`WS_EX_TOPMOST`), plus `WS_EX_TOOLWINDOW` so it stays out of the taskbar and Alt+Tab | `NSWindow.Level.floating` (what winit and JUCE use), or `.statusBar` to sit above other floating panels |
| Show on every Space or desktop | No public API to pin a window to all virtual desktops. The user can do it from Task View. | Yes: `NSWindow.CollectionBehavior.canJoinAllSpaces` |
| DAW goes fullscreen | The appbar docs say the Bar **must** drop to the bottom of the z-order (`ABN_FULLSCREENAPP`) | By default the Bar is not on the DAW's fullscreen Space. `.canJoinAllApplications` (macOS 13+) or `.fullScreenAuxiliary` lets it join. |

MiniMeters, the closest comparable app, has the same limits. Its docs say its "Stick" mode (a toolbar that makes other windows move out of the way) works only on Windows and Linux: "macOS does not have a system like Windows and Linux have for allowing MiniMeters to behave like a Toolbar on the desktop". For macOS it tells users to write Hammerspoon scripts that resize windows themselves (see [MiniMeters](#what-other-apps-do)).

---

## Windows: the AppBar API

### How it works

Source: [Using Application Desktop Toolbars](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/shell/application-desktop-toolbars.md) (the source of learn.microsoft.com/windows/win32/shell/application-desktop-toolbars) and [`SHAppBarMessage`](https://github.com/MicrosoftDocs/sdk-api/blob/docs/sdk-api-src/content/shellapi/nf-shellapi-shappbarmessage.md).

- An appbar "is anchored to an edge of the screen … The system prevents other applications from using the desktop area used by an appbar." This is the behaviour the Bar wants.
- **Register:** send `ABM_NEW` with the window's `HWND` and an app-defined callback message ID. The system sends notifications to the window procedure through that message, with the notification code in `wParam`. Before the window is destroyed, send `ABM_REMOVE`: "An application should always send ABM_REMOVE before destroying an appbar."
- **Position (two steps):** send `ABM_QUERYPOS` with a proposed edge (`uEdge`) and rectangle (`rc`). The system adjusts it around the taskbar and other appbars, "purely by rectangle subtraction; it makes no effort to preserve the rectangle's initial size", so the app should set its own thickness again. Then send `ABM_SETPOS`, which "may adjust the bounding rectangle" again, and move the window to the returned `rc` with `MoveWindow`. `ABM_SETPOS` "causes the system to send the ABN_POSCHANGED notification message to all appbars" ([ABM_SETPOS](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/shell/abm-setpos.md)).
- **Keeping it in place:** redo QUERYPOS and SETPOS on `ABN_POSCHANGED`, which comes whenever the taskbar changes or another appbar on the same edge is added, resized or removed ([ABN_POSCHANGED](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/shell/abn-poschanged.md)). Forward `WM_ACTIVATE` as `ABM_ACTIVATE` and `WM_WINDOWPOSCHANGED` as `ABM_WINDOWPOSCHANGED`. The docs say the second one "must" be sent.
- If the Bar shares an edge with the taskbar, "the system ensures that the taskbar is always on the outermost edge."
- **The result is the work area.** `MONITORINFO.rcWork` is the monitor's work area ([MONITORINFO](https://github.com/MicrosoftDocs/sdk-api/blob/docs/sdk-api-src/content/winuser/ns-winuser-monitorinfo.md)), and `SPI_SETWORKAREA` defines the work area as "the portion of the screen not obscured by the system taskbar or by application desktop toolbars" ([SystemParametersInfo](https://github.com/MicrosoftDocs/sdk-api/blob/docs/sdk-api-src/content/winuser/nf-winuser-systemparametersinfoa.md)). Maximised windows fill the work area, so they end next to the Bar. For the space left free to the app, the `ABM_GETTASKBARPOS` docs point to `GetMonitorInfo`, not the taskbar rectangle.
- **Auto-hide appbars** (`ABM_SETAUTOHIDEBAR`/`…EX`) do *not* reserve space. The system allows only one per edge (`…EX`: "only one autohide appbar for each edge of each monitor"). This is not what the Bar needs, but it could be an option later.
- `SPI_SETWORKAREA` sets the work area directly, for "the monitor that contains the specified rectangle". This is a global setting, and the system does not keep it in step with other appbars. Do not use it instead of the AppBar API.

### Multiple monitors

- **Documented:** `ABM_SETAUTOHIDEBAREX` / `ABM_GETAUTOHIDEBAREX` exist "for use in multiple monitor situations", and they pick the monitor from `rc`. Work areas are tracked per monitor (`MONITORINFO.rcWork`, `SPI_SETWORKAREA` above).
- **Not documented:** the appbar overview never mentions multiple monitors. Its sample builds the rectangle from `GetSystemMetrics(SM_CXSCREEN/SM_CYSCREEN)`, which is the *primary* monitor only. For a Bar on another monitor, fill `rc` from `GetMonitorInfo(MonitorFromWindow(…)).rcMonitor` in virtual-screen coordinates, which can be negative. Many appbar tools use non-primary monitors, but no Microsoft page states the exact behaviour. **Test it.**
- `ABN_FULLSCREENAPP` carries only a BOOL (`lParam` = opening or closing) and does not say which monitor ([ABN_FULLSCREENAPP](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/shell/abn-fullscreenapp.md)). With several monitors, the Bar cannot tell from the message alone whether the fullscreen app is on its monitor. It would have to check the foreground window's monitor itself. How Windows decides an app is "fullscreen" is not documented either.

### DPI scaling

- Declare the process **Per-Monitor v2** DPI-aware ([High DPI desktop app development](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/hidpi/high-dpi-desktop-application-development-on-windows.md), [DPI_AWARENESS_CONTEXT](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/hidpi/dpi-awareness-context.md)). A process that is not DPI-aware, or only system-aware, can get *virtualized* coordinates back from system APIs. Microsoft says which APIs do this "is not currently sufficiently documented". An appbar `rc` in the wrong coordinate space would reserve the wrong amount. winit is per-monitor aware by default (`EventLoopBuilderExtWindows::with_dpi_aware`, default `true` in winit 0.30.13). JUCE sets per-monitor awareness itself as well (`HiddenMessageWindow::setDPIAwareness` in `juce_Windowing_windows.cpp`).
- The appbar docs do not say which coordinate space `rc` uses. The safe reading, not verified, is physical pixels for a Per-Monitor-v2 process. The Bar's thickness in physical pixels has to be recalculated, followed by QUERYPOS and SETPOS again, on `WM_DPICHANGED` or when the Bar moves to another monitor.

### Fullscreen and maximised apps

- **Maximised** DAW: this is the case the AppBar API is for. The DAW is limited to the work area.
- **Fullscreen** app: the docs say "If it is opening, the appbar **must** drop to the bottom of the z-order. The appbar should restore its z-order position when the last full-screen application has closed." Their sample calls `SetWindowPos(…, HWND_BOTTOM, …, SWP_NOACTIVATE)` and puts the window back to `HWND_TOPMOST` afterwards.
- "Exclusive fullscreen" is mostly a thing of games today. Microsoft recommends flip-model borderless windows over fullscreen exclusive mode, and says that when "other desktop contents come on top", the compositor handles it ([Use DXGI flip model](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/direct3ddxgi/for-best-performance--use-dxgi-flip-model.md)). DAWs that go "fullscreen" usually use a borderless window the size of the monitor. That also triggers `ABN_FULLSCREENAPP` (not verified: the heuristic is not documented).

### From Rust (winit / egui, `windows` crate)

- `windows` 0.62.2 (latest stable) has `Windows::Win32::UI::Shell::SHAppBarMessage(dwmessage: u32, pdata: *mut APPBARDATA) -> usize`, the `ABM_*` and `ABN_*` constants and `SetWindowSubclass`, behind the `Win32_UI_Shell` feature (checked in the crate source). `windows-sys` has the same functions.
- **winit** has no appbar support. Get the `HWND` through `raw-window-handle`. eframe's `Frame` implements `HasWindowHandle`. You need to **receive the callback message** in the window procedure. winit 0.30's `EventLoopBuilderExtWindows::with_msg_hook` runs "before dispatching a win32 message", inside the `GetMessage` loop, so it sees *posted* messages only. The docs do not say whether the shell posts or sends the appbar callback. The reliable way is to **subclass the winit window** with `SetWindowSubclass` ([docs](https://github.com/MicrosoftDocs/sdk-api/blob/docs/sdk-api-src/content/commctrl/nf-commctrl-setwindowsubclass.md)). The subclass handles the callback message, `WM_ACTIVATE` and `WM_WINDOWPOSCHANGED`, and passes everything else on with `DefSubclassProc`.
- winit `WindowLevel::AlwaysOnTop` on Windows sets `WS_EX_TOPMOST` / `HWND_TOPMOST` (winit-win32 `window_state.rs`). `set_skip_taskbar(true)` hides the taskbar button. egui 0.36.2 (which depends on winit 0.30.13) passes these through as `ViewportBuilder::with_window_level(WindowLevel::AlwaysOnTop)` and `with_taskbar(false)`.

### From JUCE

- JUCE has no appbar support. `ComponentPeer::getNativeHandle()` returns the `HWND`. JUCE's own window procedure (`peerWindowProc`) is internal, so the callback would come through `SetWindowSubclass` here too. `Component::setAlwaysOnTop(true)` calls `SetWindowPos(…, HWND_TOPMOST)` (`juce_Windowing_windows.cpp`). JUCE uses `WS_EX_TOOLWINDOW` for windows that don't appear on the taskbar.

### Always on top and virtual desktops (Windows)

- `HWND_TOPMOST`: "Places the window above all non-topmost windows. The window maintains its topmost position even when it is deactivated." To drop it: `HWND_NOTOPMOST`, or `HWND_BOTTOM`, which also removes topmost status ([SetWindowPos](https://github.com/MicrosoftDocs/sdk-api/blob/docs/sdk-api-src/content/winuser/nf-winuser-setwindowpos.md)). Other topmost windows compete in the same group. The Bar cannot be guaranteed to stay above all of them.
- Useful extended styles ([Extended Window Styles](https://github.com/MicrosoftDocs/win32/blob/docs/desktop-src/winmsg/extended-window-styles.md)): `WS_EX_TOPMOST`. `WS_EX_TOOLWINDOW` keeps the window out of the taskbar and the Alt+Tab list. `WS_EX_NOACTIVATE` means clicking the Bar does not take focus from the DAW (the window "does not become the foreground window when the user clicks it"). This matters for a meter that sits next to a DAW that has keyboard focus.
- **Virtual desktops:** the public `IVirtualDesktopManager` only has `IsWindowOnCurrentVirtualDesktop`, `GetWindowDesktopId` and `MoveWindowToDesktop` ([docs](https://github.com/MicrosoftDocs/sdk-api/blob/docs/sdk-api-src/content/shobjidl_core/nn-shobjidl_core-ivirtualdesktopmanager.md); vtable checked in `windows` 0.62.2). **There is no public API to pin a window to all desktops.** The user can do it from Task View ("Show this window on all desktops"). Third-party tools that do it themselves are reported to use undocumented shell COM interfaces whose IDs change between Windows builds. This is not verified here, and not recommended. Whether a topmost tool window or a registered appbar follows the user across desktops by itself is **not verified**. Test it.

---

## macOS

### Can an app change `visibleFrame`? No.

- `NSScreen.visibleFrame` is read-only (`var visibleFrame: NSRect { get }`). It "does not include the area currently occupied by the dock and menu bar". Only those two system elements are named ([visibleFrame](https://developer.apple.com/documentation/appkit/nsscreen/visibleframe)).
- The `NSScreen` class docs: "The NSScreen class is only for getting information about the available displays. If you need additional information or want to change the attributes relating to a display, you must use Quartz Services." ([NSScreen](https://developer.apple.com/documentation/appkit/nsscreen)). Quartz Display Services handles display modes and configuration. It has no inset or reserved area.
- `NSApplication.PresentationOptions` can only hide or auto-hide the Dock and menu bar for *your own* app while it is active ([PresentationOptions](https://developer.apple.com/documentation/appkit/nsapplication/presentationoptions-swift.struct)). That can make `visibleFrame` larger, never smaller.
- Conclusion: **no public API makes other apps' zoom or maximise avoid the Bar.** No private API for it was found either. Tools like SketchyBar use private SkyLight calls (`SLSSetWindowLevel`, `SLSSetWindowTags` in `src/window.c`) for their *own* window, and still rely on a window manager to keep other windows away (below).

### What other apps do

- **MiniMeters**: overlay only on macOS. Its help pages say "Stick" mode (a toolbar that makes other windows move out of the way) is available on Windows and on Linux (Wayland `wlr-layer-shell`, X11 `_NET_WM_WINDOW_TYPE_DOCK`) only, because "macOS does not have a system like Windows and Linux have … which is how 'Stick' mode works". It suggests Hammerspoon scripts that resize windows below MiniMeters ([Toolbar help](https://minimeters.app/help/toolbar/), [macOS window management tips](https://minimeters.app/help/macos-window-management/)). *These pages were blocked from this environment. The quotes come from search-engine extracts of those pages, not a direct read.*
- **Übersicht** (source checked, `Uebersicht/UBWindow.m` @ `859a7d7`): no reservation at all. A borderless window at `kCGDesktopWindowLevel` or `kCGNormalWindowLevel-1`, which is *behind* normal windows, with `Stationary | CanJoinAllSpaces | IgnoresCycle`, and it ignores mouse events.
- **Window managers (yabai, Hammerspoon, and BetterTouchTool's window snapping)**: they move and resize other apps' windows through the **Accessibility API**. yabai's `external_bar <main|all>:<top>:<bottom>` setting exists so that windows it tiles leave space for a bar like SketchyBar (`doc/yabai.asciidoc`). Its README says it "must be given permission to utilize the Accessibility API". BetterTouchTool is closed source, so its method is inferred from its Accessibility permission requirement (not verified from source).

### The Accessibility option: resizing other apps' windows

- API: `AXUIElementCreateApplication(pid)` → the app's windows → `AXUIElementSetAttributeValue(window, kAXPositionAttribute / kAXSizeAttribute, AXValue)` ([AXUIElementSetAttributeValue](https://developer.apple.com/documentation/applicationservices/1460434-axuielementsetattributevalue), [kAXSizeAttribute](https://developer.apple.com/documentation/applicationservices/kaxsizeattribute), [kAXPositionAttribute](https://developer.apple.com/documentation/applicationservices/kaxpositionattribute)). Note that `kAXPositionAttribute` uses top-left-origin coordinates relative to the screen with the menu bar. AppKit uses a bottom-left origin.
- **Permission:** the user must turn the app on in System Settings › Privacy & Security › Accessibility. `AXIsProcessTrustedWithOptions` with the prompt option checks this and shows the prompt ([docs](https://developer.apple.com/documentation/applicationservices/1459186-axisprocesstrustedwithoptions)).
- **No sandbox, no Mac App Store:** Apple's [Protecting user data with App Sandbox](https://developer.apple.com/documentation/security/protecting-user-data-with-app-sandbox) lists "Use of accessibility APIs in assistive apps" as forbidden in the sandbox. Apple DTS: "It's not possible to use the Accessibility APIs from a sandboxed app." ([forum thread 805556](https://developer.apple.com/forums/thread/805556), Oct 2025). The Bar has to be distributed outside the Mac App Store (Developer ID plus notarization) if it uses this.
- **Limits** (from how the design works, not tested): this does not reserve anything, it only reacts. Das-Meter would have to watch window moves and resizes, and "zoom" or "fill" commands, in every app (AXObserver notifications) and push windows back. New windows and the green Zoom button still use the full `visibleFrame`. Some DAW windows limit their own size or fight back. It should be an **opt-in** ("Keep windows clear of the Bar"), not the default.

### Window level

- Levels, lowest to highest: … `normal`, `floating`, `tornOffMenu`/`submenu`, `modalPanel`, `mainMenu`, `statusBar`, `popUpMenu`, `screenSaver`. "Even the bottom window in a level will obscure the top window of the next level down." ([NSWindow.Level](https://developer.apple.com/documentation/appkit/nswindow/level-swift.struct)). `.mainMenu` is "Reserved for the application's main menu".
- winit maps `WindowLevel::AlwaysOnTop` to `kCGFloatingWindowLevel` (winit-appkit `window_delegate.rs`, same in 0.30.13). JUCE's `setAlwaysOnTop` uses `NSFloatingWindowLevel` (or `NSPopUpMenuWindowLevel` for temporary windows) in `juce_NSViewComponentPeer_mac.mm`.
- **Recommendation:** `.floating` is enough to stay above normal DAW windows. DAW plugin editors and mixers are often floating panels too. To stay above those, `.statusBar` is the usual next step. It is below menus and pop-ups, so menus still open on top of the Bar. With winit or egui this needs a direct `NSWindow.setLevel` call through `objc2-app-kit` on the raw `NSView`'s window.
- Make the Bar an `NSPanel` with `.nonactivatingPanel` so clicking it does not take focus from the DAW. winit master (0.31 beta) has `WindowAttributesMacOS::with_panel`. 0.30.x does not. Consider `NSApplication.ActivationPolicy.accessory` (LSUIElement: no Dock icon).

### Every Space, and fullscreen Spaces

- `NSWindow.CollectionBehavior.canJoinAllSpaces`: "The window can appear in all spaces. The menu bar behaves this way." Add `.stationary` (Mission Control leaves it in place) and `.ignoresCycle` (Übersicht uses all three) ([CollectionBehavior](https://developer.apple.com/documentation/appkit/nswindow/collectionbehavior-swift.struct)).
- **Fullscreen Spaces:** a native fullscreen app gets its own Space. For the Bar to appear there:
  - `.canJoinAllApplications` (macOS 13+): "can join the windows of other apps in full screen spaces when eligible. Use this collection behavior for floating windows and system overlays." It cannot be combined with `.primary` or `.auxiliary`.
  - `.fullScreenAuxiliary` (10.7+): "The window displays on the same space as the full screen window". winit master has `with_fullscreen_auxiliary`. JUCE sets it for non-resizable windows.
  - Whether `.fullScreenAuxiliary` plus `.canJoinAllSpaces` alone is enough to overlay *another app's* fullscreen Space, compared with also needing a non-activating panel, is **not verified from Apple docs**. It has to be tested on the macOS versions we support.
- winit and egui 0.30.x do not expose `canJoinAllSpaces`. Set it with `objc2-app-kit` on the window. With JUCE, get the `NSView*` from `getNativeHandle()`, then call `[[view window] setCollectionBehavior:…]`.

---

## What the Bar should do when the DAW goes fullscreen

- **Windows (required by the API):** on `ABN_FULLSCREENAPP` with `lParam = TRUE`, drop to `HWND_BOTTOM`. Go back to `HWND_TOPMOST` when it comes with `FALSE`. With several monitors, check whether the foreground window is on the Bar's monitor before dropping, since the message does not say which monitor (see above).
- **macOS (a product choice, since the system imposes nothing):** the default Space behaviour already hides the Bar on the DAW's fullscreen Space. An optional "Show over fullscreen apps" setting turns on `.canJoinAllApplications` or `.fullScreenAuxiliary`. There the Bar would cover part of the DAW, because nothing can shrink a fullscreen window.
- On both: a producer who puts the DAW in fullscreen has chosen that, so **getting out of the way by default** matches the Windows rule and the macOS default. Covering the DAW should be the opt-in.

## Recommendation

1. **Windows:** implement real reservation with the AppBar API in a platform module (subclassed HWND, Per-Monitor-v2, re-query on `ABN_POSCHANGED` / DPI change / monitor change, `ABM_REMOVE` on exit and on crash where possible). Topmost, `WS_EX_TOOLWINDOW`, `WS_EX_NOACTIVATE`.
2. **macOS:** the default is overlay only. Floating (or status-bar) level, a non-activating panel, `canJoinAllSpaces | stationary | ignoresCycle`. Document that windows are not pushed aside. Optionally add an Accessibility-based "keep windows clear" feature later, only for the non-sandboxed build.
3. Keep the "reserves space" promise per platform in the UI. MiniMeters does the same.

## Sources checked

- Microsoft docs, read from the `MicrosoftDocs/win32` and `MicrosoftDocs/sdk-api` GitHub repos (the source of learn.microsoft.com, which this environment could not reach): application-desktop-toolbars, SHAppBarMessage, ABM_NEW/QUERYPOS/SETPOS/GETSTATE/SETSTATE/SETAUTOHIDEBAREX, ABN_POSCHANGED/FULLSCREENAPP, SetWindowPos, Extended Window Styles, SystemParametersInfo, MONITORINFO, IVirtualDesktopManager, SetWindowSubclass, high-DPI docs, DXGI flip model.
- Apple developer documentation, read through its JSON data endpoint: NSScreen, visibleFrame, frame, NSWindow.Level, level, CollectionBehavior (+ members and availability), PresentationOptions, nonactivatingPanel, ActivationPolicy.accessory, AX* functions and attributes, Protecting user data with App Sandbox. Apple Developer Forums threads 805556 and 707680 (DTS replies).
- Source code: winit (master `809b3bc`, 0.31.0-beta.3, and tag v0.30.13), egui 0.36.2 (`ab7d768`), JUCE (`7278278`), `windows` crate 0.62.2, Übersicht (`859a7d7`), yabai (`dd84572`), SketchyBar (`5f358ec`).
- MiniMeters help pages: only through search-engine extracts (the site could not be reached).
