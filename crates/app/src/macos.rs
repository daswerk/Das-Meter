//! What winit doesn't cover on macOS: the display's usable area and whether a
//! window joins fullscreen apps' Spaces.

use dasmeter_core::{Display, Rect};
use objc2::MainThreadMarker;
use objc2_app_kit::{NSScreen, NSView, NSWindowCollectionBehavior};
use objc2_foundation::NSRect;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// The main display (the one with the menu bar): its whole area and the part
/// outside the menu bar and Dock, in logical pixels from its top-left corner.
pub fn main_display() -> Option<Display> {
    let mtm = MainThreadMarker::new()?;
    // The first screen holds the menu bar; Cocoa measures up from its bottom-left.
    let screen = NSScreen::screens(mtm).firstObject()?;
    let frame = screen.frame();
    let height = frame.size.height;
    let flip = |r: NSRect| Rect {
        x: r.origin.x as f32,
        y: (height - (r.origin.y + r.size.height)) as f32,
        width: r.size.width as f32,
        height: r.size.height as f32,
    };
    Some(Display {
        frame: flip(frame),
        usable: flip(screen.visibleFrame()),
    })
}

/// Lets `window` show over fullscreen apps (on every Space, as a fullscreen
/// app's auxiliary window), or keeps it out of their Spaces.
pub fn set_over_fullscreen(window: &Window, over: bool) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return;
    };
    // SAFETY: winit's AppKit handle points at the window's content view, which
    // lives as long as `window`; this runs on the main thread.
    let view: &NSView = unsafe { handle.ns_view.cast().as_ref() };
    let Some(ns_window) = view.window() else {
        return;
    };
    let behaviour = if over {
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
    } else {
        NSWindowCollectionBehavior::Managed
    };
    ns_window.setCollectionBehavior(behaviour);
}
