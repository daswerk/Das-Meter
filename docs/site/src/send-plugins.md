# Send Plugins

A **Send Plugin** is a small plug-in you put on a DAW track. It forwards that
track's audio to Das-Meter and shows no Meters itself. Use it when System
Capture can't hear your DAW (ASIO), or to meter single tracks.

It comes as **CLAP**, **VST3** and **AU** (macOS). In your DAW it's called
**Das-Meter Send**.

## Install

**macOS:** Das-Meter carries the Send Plugin inside the app. Choose
**Das-Meter ▸ Install Send Plugin…** (or the button on the welcome card). It
copies the plug-ins into your own plug-in folders:

- `~/Library/Audio/Plug-Ins/CLAP`
- `~/Library/Audio/Plug-Ins/VST3`
- `~/Library/Audio/Plug-Ins/Components` (AU)

No password is needed. Then rescan plug-ins in your DAW (or restart it) and
add **Das-Meter Send** to a track. When Das-Meter updates, it updates the
installed plug-ins too, and tells you to restart your DAW.

**Windows:** the installer puts the Send Plugin in the standard folders.

## Naming

Each Send Plugin has a **name** and a **colour**, and keeps both when you
reload the project:

- the name you type in its window, else the DAW's track name, else an animal
  name such as "Otter";
- the DAW's track colour, else a random one.

Duplicating a track gives the copy a new Send Plugin.

## Listening

Switch **Listen to** to **Send Plugins**. With one Send Plugin, every Meter
shows it. With several, each Meter shows a list to pick from; right-click a
Meter and use **Source** to change it, or **Use for all Meters**. Turn on
**Show Source label** to see which one a Meter shows.

The first time a Send Plugin sends while you listen to System Capture,
Das-Meter offers to switch.

## Waiting for

If a Meter's Send Plugin goes away (its DAW closed, or the track was removed),
the Meter dims and says **Waiting for *name***. It picks the Send Plugin up
again when it's back.
