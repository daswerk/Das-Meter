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
/// app's auxiliary window), or keeps it out of their Spaces.
pub fn set_over_fullscreen(window: &Window, over: bool) {
    let (Some(mtm), Some(ns_window)) = (MainThreadMarker::new(), ns_window(window)) else {
        return;
    };
    if !over {
        ns_window.setCollectionBehavior(NSWindowCollectionBehavior::Managed);
        return;
    }
    // macOS only lets an ordinary (Dock) app's window into another app's
    // fullscreen Space if the behaviour is set while the app is an accessory
    // app, so the app turns into one for a moment (as Electron does for
    // `visibleOnFullScreen`). The Dock icon stays.
    let app = NSApplication::sharedApplication(mtm);
    let regular = app.activationPolicy() == NSApplicationActivationPolicy::Regular;
    if regular {
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    }
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    if regular {
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }
}

/// Brings `window` in front of other apps' windows without giving it the
/// keyboard, so returning to Das-Meter shows all of its windows.
pub fn order_front(window: &Window) {
    if let Some(ns_window) = ns_window(window) {
        ns_window.orderFront(None);
    }
}
