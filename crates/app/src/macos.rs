//! What winit doesn't cover on macOS: the display's usable area and whether a
//! window joins fullscreen apps' Spaces.

use dasmeter_core::{Display, Fingerprint, Rect, Screen};
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSScreen, NSView, NSWindow,
    NSWindowCollectionBehavior,
};
use objc2_foundation::NSRect;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

// CoreGraphics' display queries: which monitor each display is (its EDID
// vendor, model and serial) and where it sits, in the same top-left-origin
// logical coordinates winit uses.
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGGetActiveDisplayList(max: u32, displays: *mut u32, count: *mut u32) -> i32;
    fn CGDisplayBounds(display: u32) -> NSRect;
    fn CGDisplayVendorNumber(display: u32) -> u32;
    fn CGDisplayModelNumber(display: u32) -> u32;
    fn CGDisplaySerialNumber(display: u32) -> u32;
    fn CGDisplayIsMain(display: u32) -> u32;
}

/// A CoreGraphics display's bounds and fingerprint.
struct CgDisplay {
    bounds: Rect,
    fingerprint: Fingerprint,
    main: bool,
}

fn cg_displays() -> Vec<CgDisplay> {
    let mut ids = [0u32; 16];
    let mut count = 0u32;
    // SAFETY: the buffer holds `ids.len()` entries and `count` is written
    // with how many were filled; the functions below only read a display ID.
    unsafe {
        if CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) != 0 {
            return Vec::new();
        }
        ids[..count as usize]
            .iter()
            .map(|&id| {
                let b = CGDisplayBounds(id);
                CgDisplay {
                    bounds: Rect {
                        x: b.origin.x as f32,
                        y: b.origin.y as f32,
                        width: b.size.width as f32,
                        height: b.size.height as f32,
                    },
                    fingerprint: Fingerprint {
                        vendor: CGDisplayVendorNumber(id),
                        model: CGDisplayModelNumber(id),
                        serial: CGDisplaySerialNumber(id),
                    },
                    main: CGDisplayIsMain(id) != 0,
                }
            })
            .collect()
    }
}

/// Every display: its whole and usable area (without the menu bar and Dock)
/// in logical pixels from the main display's top-left, which monitor it is,
/// its name, and whether it's the main one.
pub fn screens() -> Vec<Screen> {
    let Some(mtm) = MainThreadMarker::new() else {
        return Vec::new();
    };
    let ns_screens = NSScreen::screens(mtm);
    // Cocoa measures up from the bottom-left of the first screen.
    let Some(first) = ns_screens.firstObject() else {
        return Vec::new();
    };
    let height = first.frame().size.height;
    let flip = |r: NSRect| Rect {
        x: r.origin.x as f32,
        y: (height - (r.origin.y + r.size.height)) as f32,
        width: r.size.width as f32,
        height: r.size.height as f32,
    };
    let cg = cg_displays();
    ns_screens
        .iter()
        .map(|screen| {
            let frame = flip(screen.frame());
            // The same display in CoreGraphics: the one with the same bounds.
            let same = cg.iter().find(|d| {
                (d.bounds.x - frame.x).abs() < 1.0
                    && (d.bounds.y - frame.y).abs() < 1.0
                    && (d.bounds.width - frame.width).abs() < 1.0
            });
            Screen {
                display: Display {
                    frame,
                    usable: flip(screen.visibleFrame()),
                },
                fingerprint: same
                    .map(|d| d.fingerprint)
                    .filter(|f| f.vendor != 0 || f.model != 0),
                name: screen.localizedName().to_string(),
                main: same.map_or(frame.x == 0.0 && frame.y == 0.0, |d| d.main),
            }
        })
        .collect()
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
