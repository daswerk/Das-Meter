//! The displays on Windows and Linux, read through winit: the whole area of
//! each, and on Windows the usable part without the taskbar. (macOS reads
//! its own, with the menu bar and Dock, in `macos.rs`.)

use dasmeter_core::{Display, Rect, Screen};
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;

/// Every display, in logical pixels from the primary display's top-left.
pub fn screens(event_loop: &ActiveEventLoop) -> Vec<Screen> {
    let primary = event_loop.primary_monitor();
    event_loop
        .available_monitors()
        .map(|monitor| {
            let scale = monitor.scale_factor();
            let position = monitor.position();
            let size = monitor.size();
            let frame = [
                position.x,
                position.y,
                size.width as i32,
                size.height as i32,
            ];
            let work = work_area(&monitor).unwrap_or(frame);
            Screen {
                display: logical(frame, work, scale),
                fingerprint: None,
                name: monitor.name().unwrap_or_default(),
                main: primary.as_ref() == Some(&monitor),
            }
        })
        .collect()
}

/// A display's whole and usable area, from physical `[x, y, width, height]`.
/// The display's corner is divided by its own scale, as winit places windows
/// on it; the usable part keeps its offset within the display.
pub fn logical(frame: [i32; 4], work: [i32; 4], scale: f64) -> Display {
    let scale = scale.max(0.1) as f32;
    let [x, y, width, height] = frame.map(|v| v as f32);
    let [wx, wy, ww, wh] = work.map(|v| v as f32);
    let (left, top) = (x / scale, y / scale);
    Display {
        frame: Rect {
            x: left,
            y: top,
            width: width / scale,
            height: height / scale,
        },
        usable: Rect {
            x: left + (wx - x) / scale,
            y: top + (wy - y) / scale,
            width: ww / scale,
            height: wh / scale,
        },
    }
}

/// The display's area without the taskbar, physical `[x, y, width, height]`.
#[cfg(windows)]
fn work_area(monitor: &MonitorHandle) -> Option<[i32; 4]> {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITORINFO};
    use winit::platform::windows::MonitorHandleExtWindows;
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: winit's handle is a live HMONITOR, and `info` is a MONITORINFO
    // with its size set, as GetMonitorInfoW expects.
    let ok = unsafe { GetMonitorInfoW(HMONITOR(monitor.hmonitor() as _), &mut info) };
    if !ok.as_bool() {
        return None;
    }
    let r = info.rcWork;
    Some([r.left, r.top, r.right - r.left, r.bottom - r.top])
}

/// Linux doesn't say through winit where the panels are: the whole display.
#[cfg(not(windows))]
fn work_area(_monitor: &MonitorHandle) -> Option<[i32; 4]> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taskbar_at_the_bottom_is_left_out() {
        // 1920×1080 at 150 %, the taskbar 48 px high.
        let display = logical([0, 0, 1920, 1080], [0, 0, 1920, 1032], 1.5);
        assert_eq!(display.frame.width, 1280.0);
        assert_eq!(display.frame.height, 720.0);
        assert_eq!(display.usable.bottom(), 688.0);
    }

    #[test]
    fn a_second_display_keeps_its_place_and_its_taskbar_on_the_left() {
        let display = logical([1920, 0, 2560, 1440], [1980, 0, 2500, 1440], 1.0);
        assert_eq!(display.frame.x, 1920.0);
        assert_eq!(display.usable.x, 1980.0);
        assert_eq!(display.usable.right(), display.frame.right());
    }
}
