# Research: a tray icon with a menu on Linux

Ticket: [#79](https://github.com/daswerk/Das-Meter/issues/79) (part of [#76](https://github.com/daswerk/Das-Meter/issues/76)). Researched 2026-09-26.

This note looks at three questions. Which crate should draw Das-Meter's tray icon and menu on Linux? Where does that icon show? Can the app tell whether it shows, so the welcome card can say where the menu is?

**How sources were checked.** Crate facts come from the published crate source (tray-icon 0.25.1, muda 0.20.0, ksni 0.3.6 and libappindicator-sys 0.9.0, as downloaded from crates.io) and from `cargo tree --target x86_64-unknown-linux-gnu`. Desktop facts come from each project's own source: KDE `plasma-workspace` and `kstatusnotifieritem`, `ubuntu/gnome-shell-extension-appindicator`, `swaywm/sway`, `Alexays/Waybar` and `hyprwm/Hyprland`, all at their default branch on 2026-09-26. Distribution facts come from archlinux.org, packages.ubuntu.com and Ubuntu's `gnome-shell` source on Launchpad. freedesktop.org's wiki pages for the StatusNotifierItem spec refused the request (HTTP 418), so the D-Bus interface is cited from KDE's interface XML instead. Nothing was run on a Linux desktop. The runtime behaviour below comes from reading the source, not from testing.

---

## Answer in short

- **Keep tray-icon, and switch its Linux backend to `ksni`.** Use `tray-icon = { version = "0.25", default-features = false, features = ["ksni"] }`. tray-icon 0.25 has a built-in StatusNotifierItem backend that runs on ksni. With it there is no GTK, no libappindicator, no libxdo and no GTK main loop. The same `muda` menu code that macOS uses also drives the Linux menu.
- **Every crate in that tree may be used.** It has 88 crates. All are MIT or Apache-2.0 (single or dual), except ksni itself, which is `Unlicense`. `Unlicense` is already on the allow list in `deny.toml`.
- **Where the icon shows:** KDE Plasma out of the box. Ubuntu's GNOME session out of the box, because Ubuntu installs and enables the AppIndicator extension. Stock GNOME on Arch shows nothing, because the extension is not in the `gnome` group. Sway shows the icon in swaybar, but swaybar has no menu support. On Hyprland it shows only if the user runs a bar with a tray, such as Waybar.
- **Detection:** ask the session D-Bus whether the name `org.kde.StatusNotifierWatcher` has an owner. The `IsStatusNotifierHostRegistered` property is useless for this, because KDE and the GNOME extension both hard-code it to `true`.

---

## 1. The two ways to draw the icon

### 1a. tray-icon's default Linux backend (libappindicator + GTK 3)

| Fact | Source |
|---|---|
| tray-icon's default features are `muda-libxdo` and `libappindicator`. `libappindicator` also turns on `muda-gtk3`. | [tray-icon 0.25.1 Cargo.toml](https://docs.rs/crate/tray-icon/0.25.1/source/Cargo.toml.orig) |
| "The default Linux backend uses GTK 3, `libxdo`, and `libappindicator` or `libayatana-appindicator`." It builds against `gtk3 xdotool libappindicator-gtk3` on Arch and `libgtk-3-dev libxdo-dev libappindicator3-dev` on Debian and Ubuntu. | [tray-icon README](https://docs.rs/crate/tray-icon/0.25.1/source/README.md) |
| "On Windows and the Linux/BSD AppIndicator backend, an event loop must be running on the thread." That means a GTK main loop, which winit does not run. The GTK objects are not `Send`, so the tray and its menu would have to be built and live on a separate thread that calls `gtk::init()` and `gtk::main()`. | [tray-icon lib.rs docs](https://docs.rs/tray-icon/0.25.1/tray_icon/) |
| libappindicator-sys loads `libayatana-appindicator3.so.1` or `libappindicator3.so.1` with `dlopen` the first time it is used. If neither is installed, it **panics** with "Failed to load ayatana-appindicator3 or appindicator3 dynamic library". | [libappindicator-sys 0.9.0 src/lib.rs](https://docs.rs/crate/libappindicator-sys/0.9.0/source/src/lib.rs) |
| The Rust crates are MIT or Apache-2.0: `gtk` 0.18 and the `*-sys` crates are MIT, and `libappindicator(-sys)` is `Apache-2.0 OR MIT`. The C libraries they link or load are LGPL. For example, libayatana-appindicator ships LGPL-2.1 and LGPL-3 texts. | `cargo tree` (below); [libayatana-appindicator COPYING](https://github.com/AyatanaIndicators/libayatana-appindicator) |
| When no watcher is running, libayatana-appindicator falls back to a legacy `GtkStatusIcon` (XEmbed). XEmbed does not exist on Wayland. | [app-indicator.c `fallback()`](https://github.com/AyatanaIndicators/libayatana-appindicator/blob/master/src/app-indicator.c) |
| RustSec had marked the gtk-rs GTK 3 crates as unmaintained ([RUSTSEC-2024-0415](https://github.com/rustsec/advisory-db/blob/main/crates/gtk/RUSTSEC-2024-0415.md) and its siblings). That advisory was **withdrawn on 2026-08-14** because the repository became active again. | same |
| Size: 85 crates on Linux, of which 11 are `-sys` crates (glib, gobject, gio, gdk, gdk-pixbuf, gtk, atk, pango, cairo, libxdo, libappindicator). They need the GTK 3 dev headers through pkg-config at build time, and the GTK 3 libraries at run time. | `cargo tree` of `tray-icon = "0.25"` (default features) |

### 1b. A pure-Rust StatusNotifierItem: ksni, used directly or through tray-icon

| Fact | Source |
|---|---|
| tray-icon 0.25 has a `ksni` feature: "Uses the StatusNotifierItem D-Bus backend on Linux and BSD." It pulls in `ksni` 0.3.6 with `async-io` and `blocking`, and turns on `muda/snapshot`. If both backends are enabled, ksni wins and the build prints a warning. | [tray-icon README](https://docs.rs/crate/tray-icon/0.25.1/source/README.md), [Cargo.toml](https://docs.rs/crate/tray-icon/0.25.1/source/Cargo.toml.orig), [build.rs](https://docs.rs/crate/tray-icon/0.25.1/source/build.rs) |
| "The `ksni` backend does not require these system libraries unless a muda GTK backend is also enabled." With GTK off, muda uses a no-op Linux backend (`platform_impl/noop`). | [tray-icon README](https://docs.rs/crate/tray-icon/0.25.1/source/README.md), [muda platform_impl/mod.rs](https://docs.rs/crate/muda/0.20.0/source/src/platform_impl/mod.rs) |
| Threading: "The KSNI backend runs its D-Bus service on a worker thread and does not require a GTK event loop." ksni's blocking `spawn()` registers with the watcher, then runs its service loop on a `std::thread`. tray-icon adds a `tray-icon-menu-watcher` thread that pushes muda menu changes to that service. | [tray-icon lib.rs docs](https://docs.rs/tray-icon/0.25.1/tray_icon/), [ksni blocking.rs](https://docs.rs/crate/ksni/0.3.6/source/src/blocking.rs), [tray-icon ksni/mod.rs](https://docs.rs/crate/tray-icon/0.25.1/source/src/platform_impl/ksni/mod.rs) |
| Menus: the backend turns muda's `MenuItem`, `CheckMenuItem`, `Submenu`, `IconMenuItem` and separators into ksni DBusMenu items. **Other predefined items, including `PredefinedMenuItem::quit`, become a greyed-out label** (`enabled: false`, with "TODO: support some predefined items like Quit or About"). On Linux, Das-Meter's tray menu needs a plain "Quit Das-Meter" `MenuItem`. | [tray-icon ksni/menu.rs](https://docs.rs/crate/tray-icon/0.25.1/source/src/platform_impl/ksni/menu.rs) |
| A left click reaches the app as `TrayIconEvent::Click { button: Left }` (SNI `Activate`). `SecondaryActivate` becomes `Middle`. | [tray-icon ksni/mod.rs](https://docs.rs/crate/tray-icon/0.25.1/source/src/platform_impl/ksni/mod.rs) |
| The icon is sent as ARGB pixmaps. `with_icon_as_template` is macOS-only, so the icon does not recolour for light or dark panels. | same |
| **Startup without a watcher:** tray-icon calls ksni's `spawn()` without `assume_sni_available`. If no `org.kde.StatusNotifierWatcher` exists, `TrayIconBuilder::build()` returns `Err(Error::Ksni(Watcher(ServiceUnknown…)))`. It does not retry, so an icon that failed at launch will not appear if the user enables a tray later. | [tray-icon ksni/mod.rs](https://docs.rs/crate/tray-icon/0.25.1/source/src/platform_impl/ksni/mod.rs), [tray-icon error.rs](https://docs.rs/crate/tray-icon/0.25.1/source/src/error.rs), [ksni service.rs](https://docs.rs/crate/ksni/0.3.6/source/src/service.rs) |
| Using ksni **directly** instead gives `assume_sni_available(true)` plus the `Tray::watcher_online` and `watcher_offline` callbacks. ksni then keeps the item alive and registers it again when a watcher appears. The cost is a second menu model (ksni's own `MenuItem` tree) next to muda's. | [ksni lib.rs](https://docs.rs/crate/ksni/0.3.6/source/src/lib.rs), [ksni service.rs](https://docs.rs/crate/ksni/0.3.6/source/src/service.rs) |
| ksni's licence is `Unlicense`, MSRV 1.80. Its only D-Bus dependency is `zbus` 5 (pure Rust, MIT). It uses Tokio by default, but tray-icon selects `async-io` instead, so no Tokio is pulled in. | [ksni Cargo.toml](https://docs.rs/crate/ksni/0.3.6/source/Cargo.toml.orig) |
| Size: `tray-icon` with `default-features = false, features = ["ksni"]` is 88 crates on Linux. None is a C `-sys` crate (only `libc` and `linux-raw-sys`). Licences: all `MIT`, `Apache-2.0` or dual (a few also offer Zlib, 0BSD or LLVM-exception), one `Unicode-3.0` in combination, and `Unlicense` for ksni. Everything is on the `deny.toml` allow list. | `cargo tree --target x86_64-unknown-linux-gnu -e normal -f "{p} {l}"` |

### Comparison

| | tray-icon + libappindicator (default) | tray-icon + `ksni` feature | ksni directly |
|---|---|---|---|
| System libraries (build / run) | GTK 3, libxdo headers; GTK 3 + (ayatana-)appindicator .so at run time, panic if missing | none | none |
| Threads beside winit | own GTK thread with `gtk::main()` | ksni worker + menu watcher (spawned by the crate) | ksni worker |
| Menu code shared with macOS | yes (muda) | yes (muda; Quit must be a plain item) | no (separate ksni menu tree) |
| Watcher appears after launch | ayatana handles it | no (build fails once, app must retry) | yes (`assume_sni_available`) |
| Licences | MIT/Apache crates; LGPL C libraries | MIT/Apache + Unlicense | MIT/Apache + Unlicense |

**Recommendation:** use tray-icon with the `ksni` feature. It keeps one menu code path across macOS and Linux, and it has no C dependencies to install or bundle. It also fits the existing `MenuEvent::set_event_handler(... wake())` pattern, because that closure is already `Send + Sync` and ksni calls it from its worker thread. One gap: the icon does not appear if a watcher starts after launch. Cover it by watching `NameOwnerChanged` for `org.kde.StatusNotifierWatcher` (section 3) and calling `TrayIconBuilder::build()` again when an owner appears. If that turns out clumsy, switch to ksni directly.

---

## 2. Where the icon shows

The protocol: an item registers with the D-Bus service `org.kde.StatusNotifierWatcher` at `/StatusNotifierWatcher`, using `RegisterStatusNotifierItem`. A host (the panel) reads the items from that service. ([KDE interface XML](https://github.com/KDE/kstatusnotifieritem/blob/master/src/org.kde.StatusNotifierWatcher.xml); ksni's proxy uses the same name and path: [dbus_interface.rs](https://docs.rs/crate/ksni/0.3.6/source/src/dbus_interface.rs).) Whoever owns that name decides whether the icon is seen.

| Desktop | Watcher / host | Icon | Menu | Source |
|---|---|---|---|---|
| **KDE Plasma** | The `StatusNotifierWatcher` KDED module in plasma-workspace, always loaded. The Plasma system tray is the host. | yes | yes | [plasma-workspace statusnotifierwatcher.cpp](https://github.com/KDE/plasma-workspace/blob/master/statusnotifierwatcher/statusnotifierwatcher.cpp) |
| **GNOME (upstream)** | GNOME Shell has none. The extension "AppIndicator and KStatusNotifierItem Support" (`appindicatorsupport@rgcjonas.gmail.com`, GNOME Shell 45–51, latest release v66 from 2026-09-22) owns `org.kde.StatusNotifierWatcher` while it is enabled. | only with the extension | yes | [extension repo](https://github.com/ubuntu/gnome-shell-extension-appindicator), [metadata.json](https://github.com/ubuntu/gnome-shell-extension-appindicator/blob/master/metadata.json), [statusNotifierWatcher.js](https://github.com/ubuntu/gnome-shell-extension-appindicator/blob/master/statusNotifierWatcher.js) |
| **Ubuntu (GNOME session)** | `ubuntu-desktop-minimal` **depends on** `gnome-shell-extension-appindicator` (noble 24.04 and questing). Ubuntu's session mode enables `ubuntu-appindicators@ubuntu.com` by default. | yes, out of the box | yes | [packages.ubuntu.com noble/ubuntu-desktop-minimal](https://packages.ubuntu.com/noble/ubuntu-desktop-minimal), [questing](https://packages.ubuntu.com/questing/ubuntu-desktop-minimal), [ubuntu gnome-shell `ubuntu.json`](https://git.launchpad.net/ubuntu/+source/gnome-shell/tree/debian/ubuntu-session-mods/ubuntu.json?h=ubuntu/noble) |
| **Arch (GNOME)** | `gnome-shell-extension-appindicator` (version 65, `extra`) is in **no group**. It is not in `gnome` or `gnome-extra`, and `gnome-shell` does not depend on it. Only four packages list it, all as optional dependencies. Even after installing it, the user must enable it in the Extensions app, then log out and back in on Wayland. | no, by default | — | [archlinux.org package](https://archlinux.org/packages/extra/any/gnome-shell-extension-appindicator/), [`gnome` group](https://archlinux.org/groups/x86_64/gnome/), [`gnome-extra` group](https://archlinux.org/groups/x86_64/gnome-extra/), [extension README](https://github.com/ubuntu/gnome-shell-extension-appindicator) |
| **Sway** | swaybar has its own watcher and host (build option `tray`, `auto` by default). It shows on all outputs unless `tray_output` limits it. **swaybar draws no menus**: `item.c` has `// TODO menu` and only sends `ContextMenu`/`Activate`/`SecondaryActivate` to the item. ksni answers `ContextMenu` with "Not supported". | yes | **no**: left click (`Activate`) is all the user gets | [sway swaybar/tray/item.c](https://github.com/swaywm/sway/blob/master/swaybar/tray/item.c), [watcher.c](https://github.com/swaywm/sway/blob/master/swaybar/tray/watcher.c), [sway-bar(5)](https://github.com/swaywm/sway/blob/master/sway/sway-bar.5.scd), [meson_options.txt](https://github.com/swaywm/sway/blob/master/meson_options.txt), [ksni dbus_interface.rs](https://docs.rs/crate/ksni/0.3.6/source/src/dbus_interface.rs) |
| **Sway / Hyprland with Waybar** | Waybar's `tray` module owns `org.kde.StatusNotifierWatcher` (it can replace another owner) and draws DBusMenu menus through libdbusmenu-gtk. `tray` is in Waybar's shipped default `modules-right`. | yes | yes | [Waybar sni/watcher.cpp](https://github.com/Alexays/Waybar/blob/master/src/modules/sni/watcher.cpp), [sni/item.cpp](https://github.com/Alexays/Waybar/blob/master/src/modules/sni/item.cpp), [resources/config.jsonc](https://github.com/Alexays/Waybar/blob/master/resources/config.jsonc) |
| **Hyprland** | No bar of its own. The example config only suggests `waybar` in a comment ("Autostart necessary processes (like notifications daemons, status bars, etc.)"). | only with a bar | depends on the bar | [Hyprland example/hyprland.lua](https://github.com/hyprwm/Hyprland/blob/main/example/hyprland.lua) |

Consequences for Das-Meter:
- A left click on the icon must do something useful on its own ("Show Das-Meter"), because on swaybar the click is the only thing that works.
- The app must stay reachable when there is no tray at all (stock Arch GNOME, a bare Hyprland). The tray is a convenience on Linux, not the only way back to the app as it is on macOS.

---

## 3. Can the app tell whether a tray is there?

Yes. Ask the session bus whether `org.kde.StatusNotifierWatcher` has an owner (`org.freedesktop.DBus.NameHasOwner`), and listen to `NameOwnerChanged` for that name. ksni itself uses the same signal to notice a watcher coming and going ([service.rs](https://docs.rs/crate/ksni/0.3.6/source/src/service.rs)). `zbus` is already in the tree through ksni, so this adds no new crates.

| Fact | Source |
|---|---|
| The watcher interface has `IsStatusNotifierHostRegistered` (b), `RegisteredStatusNotifierItems` (as) and `ProtocolVersion` (i), plus the signals `StatusNotifierHostRegistered` and `StatusNotifierHostUnregistered`. | [KDE org.kde.StatusNotifierWatcher.xml](https://github.com/KDE/kstatusnotifieritem/blob/master/src/org.kde.StatusNotifierWatcher.xml) |
| **`IsStatusNotifierHostRegistered` tells nothing.** KDE's watcher returns `true` unconditionally and ignores `RegisterStatusNotifierHost`. The GNOME extension also returns `true`, and answers `RegisterStatusNotifierHost` with `NOT_SUPPORTED`. ksni's source says the same, so its `Error::WontShow` "cannot occur in practice". | [plasma-workspace statusnotifierwatcher.cpp L92–100](https://github.com/KDE/plasma-workspace/blob/master/statusnotifierwatcher/statusnotifierwatcher.cpp), [extension statusNotifierWatcher.js](https://github.com/ubuntu/gnome-shell-extension-appindicator/blob/master/statusNotifierWatcher.js), [ksni service.rs](https://docs.rs/crate/ksni/0.3.6/source/src/service.rs) |
| Name ownership does tell. The GNOME extension calls `own_name(WATCHER_BUS_NAME)` only when it is enabled. Waybar and swaybar own the name only while they run. On stock GNOME without the extension, nobody owns it, and a call to it fails with `ServiceUnknown`. That is exactly the error tray-icon's `build()` returns. | [extension statusNotifierWatcher.js](https://github.com/ubuntu/gnome-shell-extension-appindicator/blob/master/statusNotifierWatcher.js), [Waybar watcher.cpp](https://github.com/Alexays/Waybar/blob/master/src/modules/sni/watcher.cpp), [sway watcher.c](https://github.com/swaywm/sway/blob/master/swaybar/tray/watcher.c), [ksni lib.rs `Error::Watcher`](https://docs.rs/crate/ksni/0.3.6/source/src/lib.rs) |
| Limits: an owner means some host is running. It does not say that the host draws menus (swaybar does not) or where on screen it sits. `XDG_CURRENT_DESKTOP` (for example `KDE`, `GNOME`, `ubuntu:GNOME`, `sway`, `Hyprland`) is the usual way to choose the wording. | inferred from the sources above |

A possible welcome-card rule (a proposal, not a decision):

1. The watcher is owned and `XDG_CURRENT_DESKTOP` contains `KDE`: "Das-Meter's menu is in the system tray, bottom right."
2. The watcher is owned and it is GNOME: "Das-Meter's menu is in the top bar, top right."
3. The watcher is owned and it is sway: "Click Das-Meter's icon in swaybar to bring it back." swaybar shows no menu, so say where the menu lives inside the app.
4. The watcher is owned otherwise: "Das-Meter's menu is in your panel's tray."
5. No watcher and GNOME: "GNOME has no tray icons. Install and enable the 'AppIndicator and KStatusNotifierItem Support' extension, or use the menu inside the app."
6. No watcher otherwise: point at the in-app menu only. Keep watching `NameOwnerChanged`, and build the tray icon (and update the card) if a watcher appears later.

---

## Open points (not checked here)

- None of this was run on a real Linux desktop. Before shipping, do a hand check on KDE Plasma 6, Ubuntu 24.04 GNOME, Arch GNOME with and without the extension, sway with swaybar, and Hyprland with Waybar.
- Whether the GNOME extension and Plasma render the ARGB pixmap sharply at HiDPI, or whether a themed `icon_name` would look better, is untested. tray-icon's ksni backend only sends pixmaps.
- Flatpak or Snap packaging would need `--talk-name=org.kde.StatusNotifierWatcher` or the matching Snap plug. ksni already avoids `GetAll` property caching for Snap strict confinement ([service.rs](https://docs.rs/crate/ksni/0.3.6/source/src/service.rs)).
