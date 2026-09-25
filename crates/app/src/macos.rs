//! What winit doesn't cover on macOS: the display's usable area and whether a
//! window joins fullscreen apps' Spaces.

use dasmeter_core::{Display, Rect};
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSScreen, NSView, NSWindow,
    NSWindowCollectionBehavior,
};
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

fn ns_window(window: &Window) -> Option<objc2::rc::Retained<NSWindow>> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: winit's AppKit handle points at the window's content view, which
    // lives as long as `window`; this runs on the main thread.
    let view: &NSView = unsafe { handle.ns_view.cast().as_ref() };
    view.window()
}

/// Lets `window` show over fullscreen apps (on every Space, as a fullscreen
/// app's auxiliary window), or keeps it out of their Spaces. It only works
/// while the app has no Dock icon: see [`set_dock_icon`].
pub fn set_over_fullscreen(window: &Window, over: bool) {
    let Some(ns_window) = ns_window(window) else {
        return;
    };
    ns_window.setCollectionBehavior(if over {
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
    } else {
        NSWindowCollectionBehavior::Managed
    });
}

/// Shows or hides the app's Dock icon. Since macOS 10.14 an app with a Dock
/// icon can't put windows over other apps' fullscreen windows, so the icon
/// goes while a window should show over them (as Electron does for
/// `visibleOnFullScreen`).
pub fn set_dock_icon(shown: bool) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let policy = if shown {
        NSApplicationActivationPolicy::Regular
    } else {
        NSApplicationActivationPolicy::Accessory
    };
    if app.activationPolicy() == policy {
        return;
    }
    app.setActivationPolicy(policy);
    // Changing the policy deactivates the app; stay in front.
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
}

/// Brings `window` in front of other apps' windows without giving it the
/// keyboard, so returning to Das-Meter shows all of its windows.
pub fn order_front(window: &Window) {
    if let Some(ns_window) = ns_window(window) {
        ns_window.orderFront(None);
    }
}
