//! Bar mode: the Bar docked to a screen edge, its Meters' shares, Pop-outs,
//! the screen button and "Show over fullscreen apps".
//!
//! Sizes and positions are logical pixels with the origin at the top-left of
//! the main display, as winit gives them. The layout isn't saved yet; the
//! Presets ticket saves it.

/// The operating system, for what the screen button offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
}

impl Platform {
    /// The platform this build runs on (Windows, or macOS for everything else).
    pub fn current() -> Platform {
        if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::MacOs
        }
    }
}

/// A rectangle in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// This rectangle moved (and shrunk if it must) to lie inside `bounds`.
    pub fn clamped_to(self, bounds: Rect) -> Rect {
        let width = self.width.min(bounds.width);
        let height = self.height.min(bounds.height);
        Rect {
            x: self.x.clamp(bounds.x, bounds.right() - width),
            y: self.y.clamp(bounds.y, bounds.bottom() - height),
            width,
            height,
        }
    }
}

/// A display: its whole area, and the part windows may use (without the menu
/// bar, Dock or taskbar).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Display {
    pub frame: Rect,
    pub usable: Rect,
}

/// The screen edge the Bar docks to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Edge {
    Top,
    #[default]
    Bottom,
    Left,
    Right,
}

impl Edge {
    pub const ALL: [Edge; 4] = [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right];

    /// Whether the Bar lies along this edge horizontally, Meters side by side.
    pub fn horizontal(self) -> bool {
        matches!(self, Edge::Top | Edge::Bottom)
    }
}

/// How the Bar sits with other windows: the Bar's screen button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenMode {
    /// Windows only: an AppBar, so maximised windows make room.
    ReserveSpace,
    /// Floats over other windows.
    FloatOnTop,
    /// An ordinary window.
    NormalWindow,
}

impl ScreenMode {
    /// The Bar's default on this platform.
    pub fn default_for(platform: Platform) -> ScreenMode {
        match platform {
            Platform::Windows => ScreenMode::ReserveSpace,
            Platform::MacOs => ScreenMode::FloatOnTop,
        }
    }

    /// What one press of the screen button switches to. macOS can't reserve
    /// screen space, so it only toggles Float on top and Normal window.
    pub fn next(self, platform: Platform) -> ScreenMode {
        match (self, platform) {
            (ScreenMode::ReserveSpace, _) => ScreenMode::FloatOnTop,
            (ScreenMode::FloatOnTop, _) => ScreenMode::NormalWindow,
            (ScreenMode::NormalWindow, Platform::Windows) => ScreenMode::ReserveSpace,
            (ScreenMode::NormalWindow, Platform::MacOs) => ScreenMode::FloatOnTop,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ScreenMode::ReserveSpace => "Reserve space",
            ScreenMode::FloatOnTop => "Float on top",
            ScreenMode::NormalWindow => "Normal window",
        }
    }
}

/// What the screen button's hover says on macOS.
pub const NO_RESERVE_SPACE_ON_MACOS: &str =
    "macOS doesn't let apps reserve screen space, so the Bar floats on top instead.";

/// Which window something happens in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WindowKey {
    Bar,
    /// The Pop-out showing this Meter.
    PopOut(usize),
}

/// The Bar's default thickness.
pub const DEFAULT_THICKNESS: f32 = 180.0;
/// The thinnest Bar.
pub const MIN_THICKNESS: f32 = 60.0;
/// The smallest share a Meter in the Bar keeps.
pub const MIN_SHARE: f32 = 0.05;
/// A new Pop-out's size.
pub const POP_OUT_SIZE: (f32, f32) = (480.0, 320.0);
/// The smallest Pop-out.
pub const MIN_POP_OUT: (f32, f32) = (160.0, 120.0);

/// A Meter in its own window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopOut {
    pub meter: usize,
    pub frame: Rect,
    pub on_top: bool,
}

/// Bar mode's layout.
#[derive(Clone, Debug, PartialEq)]
pub struct BarLayout {
    pub edge: Edge,
    /// Logical pixels across the Bar, before the cap to a third of the display.
    pub thickness: f32,
    pub screen: ScreenMode,
    pub show_over_fullscreen: bool,
    /// The Meters in the Bar, in order, each with its share of the Bar's length.
    /// Shares add up to 1.
    pub meters: Vec<(usize, f32)>,
    pub pop_outs: Vec<PopOut>,
}

impl BarLayout {
    /// All `count` Meters in the Bar with equal shares, at the bottom edge.
    pub fn new(count: usize, platform: Platform) -> BarLayout {
        let share = 1.0 / count.max(1) as f32;
        BarLayout {
            edge: Edge::Bottom,
            thickness: DEFAULT_THICKNESS,
            screen: ScreenMode::default_for(platform),
            show_over_fullscreen: false,
            meters: (0..count).map(|meter| (meter, share)).collect(),
            pop_outs: Vec::new(),
        }
    }

    /// The thickness shown on `display`: at most a third of the display across the Bar.
    pub fn thickness_on(&self, display: &Display) -> f32 {
        let across = if self.edge.horizontal() {
            display.frame.height
        } else {
            display.frame.width
        };
        self.thickness
            .min(across / 3.0)
            .max(MIN_THICKNESS.min(across / 3.0))
    }

    /// Where the Bar sits on `display`: along its edge, inside the usable area.
    pub fn frame_on(&self, display: &Display) -> Rect {
        let u = display.usable;
        let t = self.thickness_on(display);
        match self.edge {
            Edge::Top => Rect { height: t, ..u },
            Edge::Bottom => Rect {
                y: u.bottom() - t,
                height: t,
                ..u
            },
            Edge::Left => Rect { width: t, ..u },
            Edge::Right => Rect {
                x: u.right() - t,
                width: t,
                ..u
            },
        }
    }

    /// Scales the shares to add up to 1.
    fn normalise(&mut self) {
        let total: f32 = self.meters.iter().map(|(_, share)| share).sum();
        if total > 0.0 {
            for (_, share) in &mut self.meters {
                *share /= total;
            }
        }
    }

    /// Moves the divider after the Bar's `divider`-th Meter to `at` (0–1 along
    /// the Bar), keeping every Meter at least [`MIN_SHARE`].
    pub fn move_divider(&mut self, divider: usize, at: f32) -> bool {
        if divider + 1 >= self.meters.len() || at.is_nan() {
            return false;
        }
        let start: f32 = self.meters[..divider].iter().map(|(_, s)| s).sum();
        let pair = self.meters[divider].1 + self.meters[divider + 1].1;
        let first = (at - start).clamp(MIN_SHARE, (pair - MIN_SHARE).max(MIN_SHARE));
        if (first - self.meters[divider].1).abs() < f32::EPSILON {
            return false;
        }
        self.meters[divider].1 = first;
        self.meters[divider + 1].1 = pair - first;
        true
    }

    /// Takes `meter` out of the Bar into a Pop-out at `frame`. The Bar keeps at
    /// least one Meter.
    pub fn pop_out(&mut self, meter: usize, frame: Rect) -> bool {
        let Some(index) = self.meters.iter().position(|(m, _)| *m == meter) else {
            return false;
        };
        if self.meters.len() == 1 {
            return false;
        }
        self.meters.remove(index);
        self.normalise();
        self.pop_outs.push(PopOut {
            meter,
            frame,
            on_top: false,
        });
        true
    }

    /// Puts a Pop-out's Meter back into the Bar, in its original order.
    pub fn dock_back(&mut self, meter: usize) -> bool {
        let Some(index) = self.pop_outs.iter().position(|p| p.meter == meter) else {
            return false;
        };
        self.pop_outs.remove(index);
        let share = 1.0 / (self.meters.len() + 1) as f32;
        for (_, s) in &mut self.meters {
            *s *= 1.0 - share;
        }
        let at = self
            .meters
            .iter()
            .position(|(m, _)| *m > meter)
            .unwrap_or(self.meters.len());
        self.meters.insert(at, (meter, share));
        true
    }

    pub fn pop_out_mut(&mut self, meter: usize) -> Option<&mut PopOut> {
        self.pop_outs.iter_mut().find(|p| p.meter == meter)
    }
}
