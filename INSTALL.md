# Installing Das-Meter

Downloads are on the
[Releases page](https://github.com/daswerk/Das-Meter/releases/latest). Each
release has:

| File | For |
|---|---|
| `Das-Meter-<version>.dmg` | macOS 14.6 or newer, Apple silicon and Intel |
| `das-meter-<version>-linux-x86_64.tar.gz` | Linux x86_64 with PipeWire |
| `Das-Meter-<version>.app.tar.gz` and `.minisig` | The app's own updater; you don't need these |

Windows (10 and 11) is coming.

- [macOS](#macos)
- [Linux](#linux)
- [Building from source](#building-from-source)

## macOS

### Install

1. Open `Das-Meter-<version>.dmg`.
2. Drag **Das-Meter** onto **Applications**.
3. Open Das-Meter from Applications.

### Allow it once (Open Anyway)

The first time, macOS says it can't check Das-Meter for malicious software.
That's expected, and you only do this once:

1. Close the message.
2. Open **System Settings ▸ Privacy & Security** and scroll down.
3. Next to "Das-Meter was blocked…", click **Open Anyway**, then confirm with
   your password or Touch ID.

This happens because Das-Meter is signed with its own certificate, not a paid
Apple Developer ID
([more](https://daswerk.github.io/Das-Meter/getting-started.html#open-anyway)).
Updates are signed with the same certificate, so you won't need to do it again.

### Start listening

Das-Meter opens as a Bar at the edge of your screen, with a welcome card.
Press **Start listening**. macOS then asks for **Screen & System Audio
Recording** permission, which Das-Meter needs to read what your Mac plays.
Nothing is recorded or saved.

If you said no, or the Meters stay flat, open **System Settings ▸ Privacy &
Security ▸ Screen & System Audio Recording** and turn on Das-Meter.

### Send Plugin (optional, for DAWs)

The Send Plugin sends single DAW tracks to Das-Meter. It comes inside the app:

1. Choose **Das-Meter ▸ Install Send Plugin…**.
2. Rescan plug-ins in your DAW, or restart it.
3. Add **Das-Meter Send** (CLAP, VST3 or AU) to a track.
4. In Das-Meter, switch **Listen to** to **Send Plugins**.

The plug-ins go into `~/Library/Audio/Plug-Ins/{CLAP,VST3,Components}`, with
no password needed.

### Updates

Das-Meter checks for a new version once a day, or when you choose **Das-Meter
▸ Check for Updates…**. **Das-Meter ▸ Update to Das-Meter *version*…** then
installs it. Before installing, it checks the download's signature. The
installed Send Plugins are updated with the app.

### Uninstall

Choose **Das-Meter ▸ Uninstall Das-Meter…**. It moves the app and the Send
Plugins it installed to the Trash. Your Presets, Themes and settings in
`~/Library/Application Support/Das-Meter` stay, unless you choose to delete
them too.

## Linux

A single x86_64 executable, built on Ubuntu 24.04. It runs on current
distributions: Arch, Fedora, Ubuntu 24.04 and newer, and others like them.

### What it needs

- **PipeWire**, for System Capture. It's the default audio server on Arch,
  Fedora and current Ubuntu.
- **Vulkan or OpenGL** graphics drivers.
- Wayland or X11.

On Arch, these are almost always installed already. If they aren't:

```sh
sudo pacman -S --needed pipewire pipewire-pulse alsa-lib libxkbcommon wayland vulkan-icd-loader
```

You also need the Vulkan driver for your GPU: `vulkan-radeon`, `vulkan-intel`
or `nvidia-utils`. For the Preset file dialogs, you need
`xdg-desktop-portal` and your desktop's portal backend.

### Install and run

```sh
tar -xzf das-meter-<version>-linux-x86_64.tar.gz
./das-meter-<version>-linux-x86_64/das-meter
```

To run it from anywhere, copy it onto your `PATH`:

```sh
install -Dm755 das-meter-<version>-linux-x86_64/das-meter ~/.local/bin/das-meter
```

Das-Meter starts listening to your default output when you press **Start
listening**. Settings, Presets and Themes are kept in `~/.config/das-meter`.

### Differences from macOS

- The Bar is an ordinary always-on-top window. Wayland doesn't let apps dock
  themselves to a screen edge.
- There's no menu bar or tray icon. Right-click a Meter for its settings.
- There's no Send Plugin for Linux yet, and no self-update. To update,
  download the new version.

### Uninstall

```sh
rm ~/.local/bin/das-meter        # or wherever you put it
rm -r ~/.config/das-meter        # only if you also want your settings gone
```

## Building from source

You need a recent stable [Rust](https://rustup.rs) (1.95 or newer).

```sh
git clone https://github.com/daswerk/Das-Meter.git
cd Das-Meter
cargo run --release --bin das-meter
```

**macOS:** you also need the Xcode Command Line Tools
(`xcode-select --install`). System Capture only works from an app bundle, so
build one:

```sh
scripts/macos-bundle.sh dist            # dist/Das-Meter.app, with the Send Plugin inside
scripts/bundle-send-plugin.sh           # the Send Plugin on its own, in target/plugins
```

A self-built app is signed ad hoc, so macOS asks for the capture permission
again after every rebuild.

**Linux (Arch):**

```sh
sudo pacman -S --needed base-devel clang pkgconf pipewire alsa-lib libxkbcommon wayland libx11 libxcursor
```

**Linux (Debian / Ubuntu):**

```sh
sudo apt install build-essential libasound2-dev libpipewire-0.3-dev libclang-dev pkg-config \
  libxkbcommon-dev libwayland-dev libx11-dev libx11-xcb-dev libxcursor-dev libxcb1-dev \
  libxcb-icccm4-dev libgl1-mesa-dev
```

Run the tests with `cargo test --workspace`.
