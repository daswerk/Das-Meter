//! The scene description: everything the shell needs to draw, and nothing more.
//!
//! Values are rounded to what is shown, so a scene only changes when something
//! visible changes. That is what lets the app core decide to sleep.

use dasmeter_analysis::{ChannelLevels, LoudnessReadings};

use crate::layout::{Edge, LayoutMode, Rect, ScreenMode, WindowKey};
use crate::meters::{MeterSettings, MeterView};
use crate::panes::Divider;
use crate::presets::PresetScene;
use crate::settings::AppSettings;
use crate::sources::ListenTo;
use crate::theme::{Colour, Palette, Role};
use crate::themes::ThemeScene;

/// Everything on screen: the windows, the notes shown over them and the colours.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub windows: Vec<WindowScene>,
    /// Brief notes, such as "Output changed: reset".
    pub notes: Vec<Note>,
    /// The Theme in use's colours; a Meter's own overrides go over them.
    pub palette: Palette,
    /// The Theme in use, its styling, and the list to choose from.
    pub theme: ThemeScene,
    /// The Presets, the current one, and whether Revert is offered.
    pub presets: PresetScene,
    /// Displays the layout's windows were placed on that aren't connected:
    /// a quiet note names them and offers Keep here.
    pub missing_displays: Vec<String>,
    pub listen_to: ListenTo,
    /// Bar mode or Window mode.
    pub mode: LayoutMode,
    /// The Send Plugins a Meter menu's Source item lists: every one that isn't gone.
    pub send_plugins: Vec<SendPluginItem>,
    /// The open Meter menu, if any.
    pub menu: Option<MeterMenu>,
    /// Whether the settings panel is open.
    pub settings_open: bool,
    /// The first-launch card to show: the welcome card, a hint or a note.
    pub card: Option<crate::onboarding::Card>,
    /// Show in Dock and Launch at login, for the settings and the menu.
    pub show_in_dock: bool,
    pub launch_at_login: bool,
    pub app: AppSettings,
    /// The highest frame-rate cap the settings panel offers.
    pub max_frame_rate_cap: u32,
}

/// An open Meter menu: whose it is and where it was opened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeterMenu {
    pub meter: usize,
    /// The window it was opened in.
    pub window: WindowKey,
    /// The right-click's point, as fractions of the window.
    pub at: [f32; 2],
}

/// A Send Plugin as a Meter menu's Source item lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct SendPluginItem {
    pub id: u64,
    /// The name, or "<name> (outdated — restart your DAW)".
    pub label: String,
    pub colour: Colour,
    /// False for an outdated Send Plugin: listed, but it can't be picked.
    pub pickable: bool,
}

/// One window and the Meters in it.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowScene {
    pub key: WindowKey,
    pub title: String,
    /// Where the window sits, in logical pixels; `None` until the display is known.
    pub frame: Option<Rect>,
    /// Kept above other windows.
    pub on_top: bool,
    /// Windows only: an AppBar that reserves its strip of the screen.
    pub reserve_space: bool,
    /// Shown over fullscreen apps instead of getting out of their way.
    pub over_fullscreen: bool,
    /// The Bar's screen button; `None` for other windows.
    pub screen: Option<ScreenMode>,
    /// The edge the Bar docks to; `None` for other windows.
    pub edge: Option<Edge>,
    pub meters: Vec<MeterScene>,
    /// Window mode's split dividers, which the user drags; empty elsewhere.
    pub dividers: Vec<Divider>,
}

/// One Meter: where it sits in its window and what it shows.
#[derive(Clone, Debug, PartialEq)]
pub struct MeterScene {
    /// Which Meter this is: its index in the app core, which events name.
    pub meter: usize,
    pub frame: Frame,
    pub state: MeterState,
    /// The small Source label, when the Meter shows it.
    pub source: Option<SourceLabel>,
    /// The Meter's settings, which its menu and the settings panel show.
    pub settings: MeterSettings,
    /// The Send Plugin the Meter shows or would show, for its Source item:
    /// its pick, the one it follows, or the only one there is.
    pub picked: Option<u64>,
    /// Whether the Source label is switched on.
    pub show_source_label: bool,
    /// The Meter's own colours for single roles, over the Theme's.
    pub overrides: Vec<(Role, Colour)>,
}

/// The Source a Meter shows, as its small label says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLabel {
    pub name: String,
    /// The Send Plugin's colour; `None` for System Capture.
    pub colour: Option<Colour>,
}

/// A Send Plugin in a Meter's "Pick a Send Plugin" list.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceItem {
    pub id: u64,
    /// The name, or "<name> (outdated — restart your DAW)".
    pub label: String,
    pub colour: Colour,
    /// False for an outdated Send Plugin: listed, but it can't be picked.
    pub pickable: bool,
    /// Where the item sits, as fractions of the window. A click inside picks it.
    pub frame: Frame,
}

/// A rectangle as fractions of the window: 0 to 1 from the top-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Frame {
    /// Where `point` (window fractions) falls inside this frame, if it does.
    pub fn locate(&self, [x, y]: [f32; 2]) -> Option<[f32; 2]> {
        let inside = [(x - self.x) / self.width, (y - self.y) / self.height];
        inside
            .iter()
            .all(|v| (0.0..=1.0).contains(v))
            .then_some(inside)
    }
}

/// What a Meter shows.
#[derive(Clone, Debug, PartialEq)]
// Nearly every Meter is Live, and a scene is built at most once a frame: boxing
// the big variant would cost an allocation per Meter per frame for nothing.
#[allow(clippy::large_enum_variant)]
pub enum MeterState {
    /// System Capture hasn't delivered a sample rate yet.
    Starting,
    /// System Capture couldn't start; the reason is shown in place of the Meter.
    Unavailable(String),
    Live(MeterView),
    /// On Send Plugins: the picked Send Plugin is gone. The Meter dims and
    /// shows "Waiting for <name>" until it is back.
    WaitingFor(String),
    /// On Send Plugins: several Send Plugins and no pick. The list is shown in
    /// place of the Meter.
    PickSendPlugin(Vec<SourceItem>),
    /// On Send Plugins, and there are none: "No Send Plugins yet".
    NoSendPlugins,
    /// On System Capture before Start listening: the Meter offers a small
    /// Start listening button and captures nothing.
    NotListening,
}

impl MeterState {
    /// What "Pick a Send Plugin" says above its list.
    pub const PICK_TITLE: &str = "Pick a Send Plugin";
    /// The button a Meter shows before Start listening.
    pub const START_LISTENING: &str = "Start listening";
    /// What a Meter says with no Send Plugins, and the hint under it.
    pub const NO_SEND_PLUGINS: &str = "No Send Plugins yet";
    pub const NO_SEND_PLUGINS_HINT: &str = "Add Das-Meter Send to a track";

    /// "Waiting for <name>".
    pub fn waiting_text(name: &str) -> String {
        format!("Waiting for {name}")
    }
}

/// A brief note shown over the Meters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Note {
    /// The output device or its rate changed, so the readings started over.
    OutputChanged,
    /// A file to import wasn't a Das-Meter Preset; nothing was changed.
    ImportFailed,
    /// The installed Send Plugin was replaced by this version's.
    SendPluginUpdated,
    /// A newer Das-Meter is out.
    UpdateAvailable,
    /// Updating failed; the app is unchanged.
    UpdateFailed,
}

impl Note {
    pub fn text(self) -> &'static str {
        match self {
            Note::OutputChanged => "Output changed: reset",
            Note::ImportFailed => "That file isn't a Das-Meter Preset: nothing was imported",
            Note::SendPluginUpdated => "Send Plugin updated: restart your DAW to use it",
            Note::UpdateAvailable => "A new Das-Meter is out: Das-Meter ▸ Update",
            Note::UpdateFailed => "The update didn't work: Das-Meter is unchanged",
        }
    }
}

/// Loudness readings as shown: in 0.1 dB steps, with anything below the floor as silence.
#[derive(Clone, Debug, PartialEq)]
pub struct LoudnessDisplay {
    pub sample_rate: u32,
    /// Momentary, short-term and integrated LUFS. [`Level::Silent`] below the −70 LUFS gate.
    pub momentary: Level,
    pub short_term: Level,
    pub integrated: Level,
    /// Loudness range in LU.
    pub range: Level,
    /// Peak to loudness ratio (true-peak maximum − integrated) and peak to
    /// short-term loudness ratio (last 3 s), in LU.
    pub plr: Level,
    pub psr: Level,
    /// The loudness graph: the LUFS bar's reading every 100 ms, oldest first
    /// (empty when the graph is off).
    pub history: Vec<Level>,
    /// Highest true peak since the last reset, in dBTP.
    pub true_peak_max: Level,
    /// False when the true peak is only a sample peak (192 kHz and above).
    pub true_peak_oversampled: bool,
    pub left: ChannelDisplay,
    pub right: ChannelDisplay,
}

/// One channel's levels as shown, in dBFS.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChannelDisplay {
    pub rms: Level,
    pub peak: Level,
    pub peak_hold: Level,
}

/// A level in tenths of a dB, or silence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    Silent,
    Tenths(i32),
}

impl Level {
    /// Rounds `db` to 0.1 dB; anything below `floor` (or not a number) is silence.
    pub fn from_db(db: f64, floor: f64) -> Level {
        if db.is_nan() || db < floor {
            Level::Silent
        } else {
            Level::Tenths((db * 10.0).round() as i32)
        }
    }

    /// The level in dB, or `None` for silence.
    pub fn db(self) -> Option<f64> {
        match self {
            Level::Silent => None,
            Level::Tenths(tenths) => Some(f64::from(tenths) / 10.0),
        }
    }
}

/// LUFS below the absolute gate (ITU-R BS.1770) show as silence.
pub const LUFS_FLOOR: f64 = -70.0;
/// PLR and PSR below this (in LU) can't happen with real audio: shown as none.
const RATIO_FLOOR: f64 = -30.0;
/// Sample levels below this show as silence.
pub const LEVEL_FLOOR_DB: f64 = -90.0;

impl LoudnessDisplay {
    pub(crate) fn new(sample_rate: u32, readings: &LoudnessReadings) -> LoudnessDisplay {
        let lufs = |value| Level::from_db(value, LUFS_FLOOR);
        LoudnessDisplay {
            sample_rate,
            momentary: lufs(readings.momentary),
            short_term: lufs(readings.short_term),
            integrated: lufs(readings.integrated),
            range: Level::from_db(readings.range, 0.0),
            plr: Level::from_db(readings.plr, RATIO_FLOOR),
            psr: Level::from_db(readings.psr, RATIO_FLOOR),
            history: Vec::new(),
            true_peak_max: Level::from_db(readings.true_peak_max, LEVEL_FLOOR_DB),
            true_peak_oversampled: readings.true_peak_oversampled,
            left: ChannelDisplay::new(&readings.left),
            right: ChannelDisplay::new(&readings.right),
        }
    }
}

impl ChannelDisplay {
    fn new(levels: &ChannelLevels) -> ChannelDisplay {
        let level = |value| Level::from_db(value, LEVEL_FLOOR_DB);
        ChannelDisplay {
            rms: level(levels.rms),
            peak: level(levels.peak),
            peak_hold: level(levels.peak_hold),
        }
    }
}
