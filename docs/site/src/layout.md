# Layout

Das-Meter has two layouts; switch in the **Window** menu (macOS) or the
settings.

## Bar

A strip of Meters docked to a screen edge. Drag its inner edge to change its
thickness, drag between Meters to share out the space, and drag its ends to
make it shorter. Choose the edge in the settings (**Bar ▸ Edge**).

- **Float on Top** (**Window ▸ Float on Top**) keeps it above other windows.
- **Reserve space** (Windows) makes other windows make room for the Bar.
  macOS has no such thing, so there the Bar floats.

## Pop-outs

Take a single Meter out of the Bar into its own window with **Pop out** in
its right-click menu; **Dock back** (or closing the window) returns it.

## Window mode

All Meters in one ordinary window, split into panes. Drag the dividers to
resize panes; **Split side by side**, **Split stacked** and **Close pane**
are in a Meter's right-click menu.
**Always on top** is in the settings.

## Fullscreen apps

**Show over fullscreen apps** (in the Bar settings) keeps the Bar visible over
other apps' fullscreen windows. On macOS this hides Das-Meter's Dock icon while
it's on; the menu bar icon stays.

## Virtual desktops

On macOS, the Bar stays on its Space, like other windows; with **Show over
fullscreen apps** on, it's on every Space. On Windows, the Bar stays on its desktop;
to have it on every desktop, right-click it in Task View and choose **Show
this window on all desktops**. Das-Meter tells you this once, the first time
you switch to a desktop without it.

## Multiple displays

Each window (the Bar, each Pop-out, the Window) remembers the display it's on.
If that display is disconnected, the window moves to the main display; it goes
back when the display returns, unless you moved it in the meantime. To keep
the new place, use **Window ▸ Keep Windows Here**.
