//! Which display each window belongs on, recognised by the monitor itself
//! (its EDID fingerprint), and where windows go while their display is missing.

use serde::{Deserialize, Serialize};

use crate::layout::{Display, Rect};

/// A monitor's EDID identity. A serial of 0 means the monitor reports none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Fingerprint {
    pub vendor: u32,
    pub model: u32,
    pub serial: u32,
}

/// A display as the shell reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    pub display: Display,
    /// `None` when the OS doesn't say which monitor it is.
    pub fingerprint: Option<Fingerprint>,
    /// For show only, such as "DELL U2720Q".
    pub name: String,
    /// The display with the menu bar (or the Windows primary display).
    pub main: bool,
}

/// A window's saved display: the monitor, its name for the note, and where
/// it sat relative to the main display (the tie-break for two of a model).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DisplayRef {
    pub fingerprint: Fingerprint,
    pub name: String,
    /// The display's top-left corner relative to the main display's, in logical px.
    pub position: [f32; 2],
}

impl DisplayRef {
    /// A reference to `screen`, or `None` if the OS didn't identify it.
    pub fn of(screen: &Screen, main: &Screen) -> Option<DisplayRef> {
        Some(DisplayRef {
            fingerprint: screen.fingerprint?,
            name: screen.name.clone(),
            position: [
                screen.display.frame.x - main.display.frame.x,
                screen.display.frame.y - main.display.frame.y,
            ],
        })
    }

    /// The same reference without the monitor's serial number, for sharing.
    pub fn without_serial(&self) -> DisplayRef {
        DisplayRef {
            fingerprint: Fingerprint {
                serial: 0,
                ..self.fingerprint
            },
            ..self.clone()
        }
    }
}

/// The screen `reference` means: the same monitor (vendor, model and serial);
/// else the one of its vendor and model, the closest to where it sat if there
/// are several; else none.
pub fn find(reference: &DisplayRef, screens: &[Screen]) -> Option<usize> {
    let wanted = reference.fingerprint;
    if wanted.serial != 0
        && let Some(i) = screens.iter().position(|s| s.fingerprint == Some(wanted))
    {
        return Some(i);
    }
    let main = screens.iter().find(|s| s.main)?;
    let distance = |s: &Screen| {
        let dx = s.display.frame.x - main.display.frame.x - reference.position[0];
        let dy = s.display.frame.y - main.display.frame.y - reference.position[1];
        dx * dx + dy * dy
    };
    screens
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            s.fingerprint
                .is_some_and(|f| f.vendor == wanted.vendor && f.model == wanted.model)
        })
        .min_by(|(_, a), (_, b)| distance(a).total_cmp(&distance(b)))
        .map(|(i, _)| i)
}

/// The screen a window's frame lies on: the one holding its centre, else the
/// nearest.
pub fn screen_at(frame: Rect, screens: &[Screen]) -> Option<usize> {
    let (cx, cy) = (frame.x + frame.width / 2.0, frame.y + frame.height / 2.0);
    let gap = |s: &Screen| {
        let f = s.display.frame;
        let dx = (f.x - cx).max(cx - f.right()).max(0.0);
        let dy = (f.y - cy).max(cy - f.bottom()).max(0.0);
        dx * dx + dy * dy
    };
    screens
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| gap(a).total_cmp(&gap(b)))
        .map(|(i, _)| i)
}

/// `frame` (relative to a display's top-left) placed on `display`, inside
/// its usable area.
pub fn place(frame: Rect, display: &Display) -> Rect {
    Rect {
        x: display.frame.x + frame.x,
        y: display.frame.y + frame.y,
        ..frame
    }
    .clamped_to(display.usable)
}

/// An on-screen `frame` relative to `display`'s top-left.
pub fn relative(frame: Rect, display: &Display) -> Rect {
    Rect {
        x: frame.x - display.frame.x,
        y: frame.y - display.frame.y,
        ..frame
    }
}
