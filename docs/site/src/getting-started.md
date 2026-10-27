# Getting started

## Download

Get the latest release from
[GitHub Releases](https://github.com/daswerk/Das-Meter/releases/latest):

- **macOS** (14.6 or newer, Apple silicon and Intel): `Das-Meter-<version>.dmg`.
- **Windows** (10 and 11, x64): `Das-Meter-<version>-windows-x64-setup.exe`.
- **Linux** (x86_64, e.g. Arch): `das-meter-<version>-linux-x86_64.tar.gz`.
  Unpack it and run `./das-meter`. System Capture needs PipeWire (the default
  on Arch, Fedora and current Ubuntu). The Bar is an ordinary always-on-top
  window there (Wayland doesn't let apps dock themselves), there's no menu bar
  or tray icon, and no Send Plugin for Linux yet.

## Install on macOS

Open the DMG and drag **Das-Meter** to **Applications**.

## Install on Windows

Run the installer and allow it to make changes (it installs for everyone on
the PC). Untick **CLAP** or **VST3** if your DAW doesn't need that Send Plugin
format. Double-clicking a `.dasmeter-preset` file afterwards imports it.

Until the installer is code-signed, Windows SmartScreen asks once: choose
**More info ▸ Run anyway**.

## Open Anyway

The first time you open Das-Meter, macOS says it can't check the app for
malicious software. That's expected, and you only need to do this once:

1. Open Das-Meter once and close the message.
2. Open **System Settings ▸ Privacy & Security** and scroll down.
3. Next to "Das-Meter was blocked…", click **Open Anyway**, then confirm.

**Why:** Apple only skips this step for apps signed with a paid Apple Developer
ID and sent to Apple for checking ("notarized"). Das-Meter is open source and
signed with its own certificate instead, so you can read exactly what it does.
Once there's a Developer ID, this step goes away.

The Send Plugin comes inside the app, so this is the only step like it:
Das-Meter copies the Send Plugin into your plug-in folders itself (see
[Send Plugins](send-plugins.md#install)).

## First launch

Das-Meter opens as a **Bar** docked to the top of your screen (the
**Producing Top-Bar 1.0** Preset), with a welcome
card next to it:

- **Start listening** starts [System Capture](listen-to.md). On macOS, this is
  when macOS asks for permission. Das-Meter never records or saves audio.
- **Install Send Plugin…** (macOS) puts the Send Plugin in your plug-in folders
  for your DAW.
- **✕** closes the card without listening; each Meter then shows a small
  **Start listening** button.

You can bring the card back with **Help ▸ Show Welcome**.

Right-click any Meter for its settings. The settings panel (**Das-Meter ▸
Settings…**, ⌘,) has everything else.

On macOS, Das-Meter also has an icon in the menu bar, so you can turn off
**Show in Dock**. **Launch at Login** is off until you turn it on.
