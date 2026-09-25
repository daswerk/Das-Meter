//! The headless app core: all app behaviour, driven by events.
//!
//! It takes events in (audio, Send Plugins, displays, user actions) and puts a
//! scene description and persistence writes out, so it can be tested without
//! windows, a GPU or audio devices. This crate must not depend on platform, GPU
//! or audio-device code (checked by `scripts/check-headless-deps.sh`).
//!
//! Time comes in with every call as the time since the app started, so tests
//! drive the core with a fake clock.

pub mod displays;
pub mod layout;
pub mod meters;
pub mod panes;
pub mod presets;
pub mod scene;
pub mod settings;
pub mod sharing;
pub mod sources;
pub mod theme;
pub mod themes;

use std::time::Duration;

pub use displays::{DisplayRef, Fingerprint, Screen};
pub use layout::{
    BarEnd, BarLayout, Display, Edge, LayoutMode, Platform, PopOut, Rect, ScreenMode, WindowKey,
};
pub use meters::{
    CursorReadout, LoudnessMeterSettings, LufsBar, MeterKind, MeterSettings, MeterView,
    SpectrumMeterSettings, StereoDrawing, StereometerMeterSettings, WaveformColouring,
    WaveformMeterSettings,
};
pub use panes::{Direction, Divider, Node, SplitId, WindowLayout};
pub use presets::{
    BuiltIn, MeterPreset, PresetData, PresetFile, PresetInfo, PresetOp, PresetScene, RoleColour,
    StoredSettings, ThemeRef,
};
pub use scene::{
    ChannelDisplay, Frame, Level, LoudnessDisplay, MeterMenu, MeterScene, MeterState, Note, Scene,
    SendPluginItem, SourceItem, SourceLabel, WindowScene,
};
pub use settings::AppSettings;
pub use sources::{ListenTo, Pick, SendPlugin, SendPluginState};
pub use theme::{Colour, LineWeight, Palette, Role, Styling, Theme};
pub use themes::{Appearance, FileWrite, ThemeFile, ThemeInfo, ThemeScene};

use meters::Meter;
use presets::Presets;
use sources::Resolved;

/// How long a note such as "Output changed: reset" stays on screen.
pub const NOTE_DURATION: Duration = Duration::from_secs(3);

/// The default frame-rate cap: 60 fps.
pub const DEFAULT_FRAME_INTERVAL: Duration =
    Duration::from_nanos(1_000_000_000 / settings::DEFAULT_FRAME_RATE_CAP as u64);

/// Something that happened, fed into the core by the shell.
#[derive(Clone, Copy, Debug)]
pub enum Event<'a> {
    /// System Capture started delivering audio at this sample rate. When it was
    /// already running, the output device or its rate changed.
    CaptureStarted {
        sample_rate: u32,
    },
    /// System Capture couldn't start (or stopped); the reason is shown.
    CaptureFailed(&'a str),
    /// Interleaved stereo frames from the active Source, at its sample rate.
    Audio(&'a [f32]),
    /// Whether any of the app's windows can be seen (not minimised or covered).
    Visible(bool),
    /// The pointer moved to this point of a window (fractions, 0–1 from the
    /// top-left), or left it.
    Pointer(Option<(WindowKey, [f32; 2])>),
    /// A Meter's settings changed. `meter` is its index in the window.
    SetMeter {
        meter: usize,
        settings: MeterSettings,
    },
    /// The user switched Listen to. The shell pauses System Capture while it
    /// is Send Plugins.
    SetListenTo(ListenTo),
    /// The Send Plugins the transport lists now, gone ones included. The shell
    /// sends this every few hundred milliseconds while listening to Send Plugins.
    SendPlugins(&'a [SendPlugin]),
    /// Interleaved stereo frames from one Send Plugin, at its sample rate.
    SendPluginAudio {
        id: u64,
        frames: &'a [f32],
    },
    /// The user picked a Send Plugin for a Meter (its Source item, or its list).
    PickSendPlugin {
        meter: usize,
        id: u64,
    },
    /// "Use for all Meters": every Meter takes the Send Plugin this one shows.
    UseForAllMeters {
        meter: usize,
    },
    /// The Source label on a Meter was switched on or off.
    ShowSourceLabel {
        meter: usize,
        shown: bool,
    },
    /// A click at this point of the window (fractions, 0–1 from the top-left).
    /// It closes an open menu; otherwise it picks from a "Pick a Send Plugin"
    /// list, or resets a Loudness Meter.
    Click {
        window: WindowKey,
        at: [f32; 2],
    },
    /// A right-click at this point: opens the menu of the Meter under it.
    OpenMenu {
        window: WindowKey,
        at: [f32; 2],
    },
    /// The Meter menu was closed without a click on the Meters.
    CloseMenu,
    /// The settings panel was opened or closed.
    ShowSettings(bool),
    /// Reset a Loudness Meter's integrated LUFS, LRA and maxima.
    ResetLoudness {
        meter: usize,
    },
    /// The app settings changed.
    SetApp(AppSettings),
    /// The display the window is on refreshes this many times a second: the
    /// highest frame-rate cap.
    DisplayRefreshRate(u32),
    /// The display the Bar is on: its whole and usable area.
    Display(Display),
    /// Dock the Bar to this edge.
    SetEdge(Edge),
    /// The Bar's thickness was dragged to this many logical pixels.
    SetBarThickness(f32),
    /// The divider after the Bar's `divider`-th Meter was dragged to `at`
    /// (0–1 along the Bar).
    MoveDivider {
        divider: usize,
        at: f32,
    },
    /// One end of the Bar was dragged to `at` logical px along its edge.
    MoveBarEnd {
        end: BarEnd,
        at: f32,
    },
    /// Take a Meter out of the Bar into its own window.
    PopOut {
        meter: usize,
    },
    /// Put a Pop-out's Meter back into the Bar (also when its window is closed).
    DockBack {
        meter: usize,
    },
    /// The user moved or resized a Pop-out.
    PopOutMoved {
        meter: usize,
        frame: Rect,
    },
    /// A Pop-out's Always on top was switched.
    SetPopOutOnTop {
        meter: usize,
        on_top: bool,
    },
    /// The Bar's screen button was pressed.
    CycleScreenMode,
    /// "Show over fullscreen apps" was switched.
    ShowOverFullscreen(bool),
    /// Switch between Bar mode and Window mode. Each keeps its own layout.
    SetMode(LayoutMode),
    /// Split the pane showing this Meter; the new pane gets a copy of it.
    SplitPane {
        meter: usize,
        direction: Direction,
    },
    /// Close the pane showing this Meter; its sibling takes the space.
    ClosePane {
        meter: usize,
    },
    /// A split's divider was dragged to `at` (0–1 across the window, along the
    /// split's direction).
    MoveSplit {
        split: SplitId,
        at: f32,
    },
    /// Show another kind of Meter in this Meter's pane, on default settings.
    AssignMeter {
        meter: usize,
        kind: MeterKind,
    },
    /// Float on Top for whichever window the mode shows: the Bar's screen
    /// button, or Window mode's Always on top.
    ToggleOnTop,
    /// The user moved or resized Window mode's window.
    WindowMoved(Rect),
    /// The Theme files in the themes folder, read at launch and whenever the
    /// folder changes.
    ThemeFiles(&'a [ThemeFile]),
    /// The OS switched between light and dark.
    SystemAppearance(Appearance),
    /// Use these Themes (indexes into the scene's list): the same one twice,
    /// or a light/dark pair that follows the system.
    ChooseTheme {
        light: usize,
        dark: usize,
    },
    /// Copy a Theme into an editable one and use it.
    DuplicateTheme {
        theme: usize,
    },
    /// Edit one colour role of a Theme from the themes folder.
    SetThemeColour {
        theme: usize,
        role: Role,
        colour: Colour,
    },
    /// Edit a Theme's styling.
    SetThemeStyling {
        theme: usize,
        styling: Styling,
    },
    /// Override one colour role on one Meter, or (`None`) go back to the Theme's.
    SetOverride {
        meter: usize,
        role: Role,
        colour: Option<Colour>,
    },
    /// The presets folder's files and `settings.toml` (`None` if there is
    /// none: first launch), read at launch. Opens the last used Preset.
    PresetFiles {
        files: &'a [PresetFile],
        settings: Option<&'a str>,
    },
    /// Switch to the Preset at this index in the list.
    SwitchPreset {
        index: usize,
    },
    /// Cmd/Ctrl+1–9: the n-th Preset in list order.
    PresetShortcut {
        number: usize,
    },
    /// Back to the current Preset as it was opened.
    RevertPreset,
    /// The current state as a new Preset, which becomes current.
    SavePresetAsNew,
    DuplicatePreset {
        index: usize,
    },
    RenamePreset {
        index: usize,
        name: &'a str,
    },
    /// Move a Preset to the trash (the last readable one stays).
    DeletePreset {
        index: usize,
    },
    /// Move a Preset in the list.
    MovePreset {
        from: usize,
        to: usize,
    },
    /// A built-in Preset's copy back to this version's built-in.
    ResetPreset {
        index: usize,
    },
    /// A `.dasmeter-preset` file's contents to import: from the menu, a drop
    /// or the OS opening one. The app switches to it; a bad file changes nothing.
    ImportPreset(&'a str),
    /// The displays, whenever they change: their areas, which monitor each
    /// is, and which is the main one.
    Screens(&'a [Screen]),
    /// Keep here: every window that's away from its missing display adopts
    /// where it is now.
    KeepHere,
    /// Show a quiet note from the shell (an update, the Send Plugin refreshed).
    ShowNote(Note),
    /// The update check ran at this time (seconds since the Unix epoch). It's
    /// kept in `settings.toml`, so a relaunch doesn't check again that day.
    UpdateChecked {
        at: u64,
    },
}

/// What the shell should do next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Draw [`AppCore::scene`] now.
    Draw,
    /// Draw nothing. Ask again at `until` (time since start), or on the next
    /// event if `None`: the scene has settled.
    Sleep { until: Option<Duration> },
}

/// System Capture's state.
enum Capture {
    Starting,
    Failed(String),
    Live,
}

/// A Meter with its Source.
struct MeterSlot {
    meter: Meter,
    /// The Send Plugin the user picked. Kept while on System Capture.
    pick: Option<Pick>,
    show_source_label: bool,
    /// This Meter's own colours for single roles, over the Theme's.
    overrides: Vec<(Role, Colour)>,
    /// On Send Plugins: the ID and sample rate of the Send Plugin whose audio
    /// the analyser has, and when it was last fed (audio or idle silence).
    showing: Option<Showing>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Showing {
    id: u64,
    sample_rate: u32,
    fed_at: Duration,
}

/// Most silence fed at once for an idle Send Plugin, in seconds: enough for
/// every Meter to fall quiet, however long the gap since the last feed.
const MAX_IDLE_FEED_SECONDS: f64 = 1.0;

/// The app core. Owns Listen to, the Send Plugin picks and the Meters with their analysers.
pub struct AppCore {
    listen_to: ListenTo,
    capture: Capture,
    /// The Send Plugins last listed by the transport.
    send_plugins: Vec<SendPlugin>,
    meters: Vec<MeterSlot>,
    pointer: Option<(WindowKey, [f32; 2])>,
    platform: Platform,
    mode: LayoutMode,
    layout: BarLayout,
    window: WindowLayout,
    /// The main display, for the Bar's default place.
    display: Option<Display>,
    /// Every display, as the shell last reported them.
    screens: Vec<Screen>,
    note_until: Option<Duration>,
    note: Note,
    /// When the current stretch of silence began, if it's silent now.
    silent_since: Option<Duration>,
    /// The Meters have settled in silence: silent blocks are skipped.
    quiet: bool,
    visible: bool,
    app: AppSettings,
    /// The display's refresh rate, once the shell says it.
    refresh_rate: Option<u32>,
    menu: Option<MeterMenu>,
    settings_open: bool,
    themes: themes::Themes,
    presets: Presets,
    /// System Capture's rate, to start Meters a Preset brings.
    capture_rate: Option<u32>,
    /// Something a Preset saves changed; it's saved at `save_after`.
    touched: bool,
    save_after: Option<Duration>,
    /// Something happened since the last scene was built.
    changed: bool,
    /// The scene last handed to the shell, and when.
    drawn: Option<Scene>,
    drawn_at: Option<Duration>,
}

impl Default for AppCore {
    fn default() -> Self {
        AppCore::new()
    }
}

impl AppCore {
    /// The default layout: one window with a row of Waveform, Spectrum,
    /// Stereometer and Loudness Meter, all on default settings.
    pub fn new() -> AppCore {
        AppCore::with_meters(vec![
            MeterSettings::Waveform(WaveformMeterSettings::default()),
            MeterSettings::Spectrum(SpectrumMeterSettings::default()),
            MeterSettings::Stereometer(StereometerMeterSettings::default()),
            MeterSettings::Loudness(LoudnessMeterSettings::default()),
        ])
    }

    /// One window with these Meters in a row, left to right.
    pub fn with_meters(meters: Vec<MeterSettings>) -> AppCore {
        let platform = Platform::current();
        AppCore {
            platform,
            mode: LayoutMode::Bar,
            window: if meters.len() == 4 {
                WindowLayout::mixing()
            } else {
                WindowLayout {
                    frame: None,
                    display: None,
                    on_top: false,
                    tree: Node::Pane(0),
                }
            },
            layout: BarLayout::new(meters.len(), platform),
            display: None,
            screens: Vec::new(),
            listen_to: ListenTo::SystemCapture,
            capture: Capture::Starting,
            send_plugins: Vec::new(),
            meters: meters
                .into_iter()
                .map(|settings| MeterSlot {
                    meter: Meter::new(settings),
                    pick: None,
                    show_source_label: false,
                    overrides: Vec::new(),
                    showing: None,
                })
                .collect(),
            pointer: None,
            note_until: None,
            note: Note::OutputChanged,
            silent_since: None,
            quiet: false,
            visible: true,
            app: AppSettings::default(),
            refresh_rate: None,
            menu: None,
            settings_open: false,
            themes: themes::Themes::new(),
            presets: Presets::default(),
            capture_rate: None,
            touched: false,
            save_after: None,
            changed: true,
            drawn: None,
            drawn_at: None,
        }
    }

    /// The same core as if it ran on `platform`, for what the screen button offers.
    pub fn on_platform(mut self, platform: Platform) -> AppCore {
        self.platform = platform;
        self.layout.screen = ScreenMode::default_for(platform);
        self
    }

    /// Theme files to write into the themes folder since the last call.
    pub fn take_writes(&mut self) -> Vec<FileWrite> {
        self.themes.take_writes()
    }

    fn main_screen(&self) -> Option<&Screen> {
        self.screens
            .iter()
            .find(|s| s.main)
            .or(self.screens.first())
    }

    fn main_display(&self) -> Option<Display> {
        self.main_screen().map(|s| s.display)
    }

    /// The screen a saved display means, if it's connected.
    fn found(&self, reference: Option<&DisplayRef>) -> Option<&Screen> {
        displays::find(reference?, &self.screens).map(|i| &self.screens[i])
    }

    /// Whether a window's saved display is missing (`None` means main: never missing).
    fn missing(&self, reference: Option<&DisplayRef>) -> bool {
        reference.is_some() && self.found(reference).is_none()
    }

    /// The display the Bar docks on: its own if connected, else the main one.
    fn bar_display(&self) -> Option<Display> {
        self.found(self.layout.display.as_ref())
            .map(|s| s.display)
            .or_else(|| self.main_display())
    }

    /// An on-screen frame as a window saves it: relative to the display it's
    /// on, and that display (`None` for the main one or an unidentified one).
    fn anchor(&self, frame: Rect) -> (Rect, Option<DisplayRef>) {
        let Some(i) = displays::screen_at(frame, &self.screens) else {
            return (frame, None);
        };
        let screen = &self.screens[i];
        let reference = self
            .main_screen()
            .filter(|main| !screen.main && !std::ptr::eq(*main, screen))
            .and_then(|main| DisplayRef::of(screen, main));
        (displays::relative(frame, &screen.display), reference)
    }

    /// Where a Pop-out shows: on its display, or at the same offset on the
    /// Bar's while it's missing.
    fn pop_out_on_screen(&self, pop_out: &PopOut) -> Rect {
        let display = self
            .found(pop_out.display.as_ref())
            .map(|s| s.display)
            .or_else(|| self.bar_display());
        match display {
            Some(display) => displays::place(pop_out.frame, &display),
            None => pop_out.frame,
        }
    }

    /// Where the Window shows: on its display, or the main one while it's missing.
    fn window_on_screen(&self) -> Option<Rect> {
        let frame = self.window.frame?;
        let display = self
            .found(self.window.display.as_ref())
            .map(|s| s.display)
            .or_else(|| self.main_display());
        Some(match display {
            Some(display) => displays::place(frame, &display),
            None => frame,
        })
    }

    /// The names of the saved displays the current layout misses.
    fn missing_displays(&self) -> Vec<String> {
        let mut refs: Vec<&DisplayRef> = Vec::new();
        match self.mode {
            LayoutMode::Bar => {
                refs.extend(self.layout.display.as_ref());
                refs.extend(
                    self.layout
                        .pop_outs
                        .iter()
                        .filter_map(|p| p.display.as_ref()),
                );
            }
            LayoutMode::Window => refs.extend(self.window.display.as_ref()),
        }
        let mut names: Vec<String> = Vec::new();
        for reference in refs {
            if self.found(Some(reference)).is_none() && !names.contains(&reference.name) {
                names.push(reference.name.clone());
            }
        }
        names
    }

    /// Keep here: windows away from their missing display adopt where they are.
    fn keep_here(&mut self) -> bool {
        let mut changed = false;
        if self.missing(self.layout.display.as_ref()) {
            self.layout.display = None;
            changed = true;
        }
        let moved: Vec<(usize, Rect)> = self
            .layout
            .pop_outs
            .iter()
            .filter(|p| self.missing(p.display.as_ref()))
            .map(|p| (p.meter, self.pop_out_on_screen(p)))
            .collect();
        for (meter, on_screen) in moved {
            let (frame, display) = self.anchor(on_screen);
            if let Some(pop_out) = self.layout.pop_out_mut(meter) {
                pop_out.frame = frame;
                pop_out.display = display;
                changed = true;
            }
        }
        if self.missing(self.window.display.as_ref()) {
            if let Some(on_screen) = self.window_on_screen() {
                let (frame, display) = self.anchor(on_screen);
                self.window.frame = Some(frame);
                self.window.display = display;
            } else {
                self.window.display = None;
            }
            changed = true;
        }
        changed
    }

    /// The display the Bar is on, once the shell has said.
    pub fn display(&self) -> Option<Display> {
        self.display
    }

    /// Whether any window can be seen.
    pub fn visible(&self) -> bool {
        self.visible
    }

    /// Whether the Meters have settled in silence: silence can't change the
    /// screen any more, so only an audible block needs to wake the app. A
    /// [`Decision::Sleep`] without a time alone doesn't mean this: between
    /// frames, silence still moves peaks and holds on.
    pub fn settled(&self) -> bool {
        self.quiet
    }

    pub fn mode(&self) -> LayoutMode {
        self.mode
    }

    pub fn window_layout(&self) -> &WindowLayout {
        &self.window
    }

    pub fn layout(&self) -> &BarLayout {
        &self.layout
    }

    /// Where a new Pop-out for `meter` goes: next to its place in the Bar, inside the display.
    fn pop_out_frame(&self, meter: usize) -> Rect {
        let (width, height) = layout::POP_OUT_SIZE;
        let Some(display) = self.bar_display() else {
            return Rect {
                x: 100.0,
                y: 100.0,
                width,
                height,
            };
        };
        let bar = self.layout.frame_on(&display);
        let start: f32 = self
            .layout
            .meters
            .iter()
            .take_while(|(m, _)| *m != meter)
            .map(|(_, share)| share)
            .sum();
        // Room for the Pop-out's title bar, which sits above its frame.
        let gap = 40.0;
        let frame = match self.layout.edge {
            Edge::Bottom => Rect {
                x: bar.x + start * bar.width,
                y: bar.y - height - gap,
                width,
                height,
            },
            Edge::Top => Rect {
                x: bar.x + start * bar.width,
                y: bar.bottom() + gap,
                width,
                height,
            },
            Edge::Left => Rect {
                x: bar.right() + gap,
                y: bar.y + start * bar.height,
                width,
                height,
            },
            Edge::Right => Rect {
                x: bar.x - width - gap,
                y: bar.y + start * bar.height,
                width,
                height,
            },
        };
        frame.clamped_to(display.usable)
    }

    /// The app settings, with the frame-rate cap kept within the display's refresh rate.
    pub fn app_settings(&self) -> AppSettings {
        self.app
    }

    /// When the update check last ran (seconds since the Unix epoch; 0 for never).
    pub fn last_update_check(&self) -> u64 {
        self.presets.settings.last_update_check
    }

    /// The highest frame-rate cap the settings offer: the display's refresh
    /// rate, and never below the default.
    pub fn max_frame_rate_cap(&self) -> u32 {
        self.refresh_rate
            .unwrap_or(settings::DEFAULT_FRAME_RATE_CAP)
            .max(settings::DEFAULT_FRAME_RATE_CAP)
    }

    fn clamp_frame_rate_cap(&mut self) {
        self.app.frame_rate_cap = self
            .app
            .frame_rate_cap
            .clamp(settings::MIN_FRAME_RATE_CAP, self.max_frame_rate_cap());
    }

    /// The settings of the Meter at `meter`, if there is one.
    pub fn meter_settings(&self, meter: usize) -> Option<MeterSettings> {
        self.meters.get(meter).map(|slot| slot.meter.settings())
    }

    pub fn listen_to(&self) -> ListenTo {
        self.listen_to
    }

    /// The Send Plugin a Meter picked, if it did.
    pub fn pick(&self, meter: usize) -> Option<&Pick> {
        self.meters.get(meter)?.pick.as_ref()
    }

    /// The Send Plugins to listen to: the ones the Meters show. The shell sets
    /// their listened-to flags and clears the others'. Empty on System Capture.
    pub fn listened(&self) -> Vec<u64> {
        let mut ids: Vec<u64> = self
            .meters
            .iter()
            .filter_map(|slot| slot.showing.map(|showing| showing.id))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    pub fn handle(&mut self, event: Event, now: Duration) {
        use Event::*;
        match event {
            PresetFiles { files, settings } => {
                let open = self.presets.load(files, settings);
                let stored = &self.presets.settings;
                self.app.frame_rate_cap = stored.frame_rate_cap;
                self.app.check_for_updates = stored.check_for_updates;
                self.clamp_frame_rate_cap();
                if let Some(data) = open {
                    self.apply(data, now);
                }
                self.changed = true;
                return;
            }
            SwitchPreset { .. }
            | PresetShortcut { .. }
            | RevertPreset
            | SavePresetAsNew
            | DuplicatePreset { .. }
            | RenamePreset { .. }
            | DeletePreset { .. }
            | MovePreset { .. }
            | ResetPreset { .. }
            | ImportPreset(_) => {
                self.handle_preset(event, now);
                self.changed = true;
                return;
            }
            // What doesn't change what a Preset saves.
            Audio(_)
            | SendPluginAudio { .. }
            | Pointer(_)
            | Visible(_)
            | CaptureStarted { .. }
            | CaptureFailed(_)
            | DisplayRefreshRate(_)
            | Display(_)
            | ThemeFiles(_)
            | SystemAppearance(_)
            | OpenMenu { .. }
            | CloseMenu
            | ShowSettings(_)
            | ShowNote(_)
            | UpdateChecked { .. }
            | ResetLoudness { .. } => {}
            // Auto-save a short pause after the last change (with a Preset open).
            _ if self.presets.current_index().is_some() => {
                self.save_after = Some(now + AUTOSAVE_DELAY);
                self.touched = true;
            }
            _ => {}
        }
        if let CaptureStarted { sample_rate } = event {
            self.capture_rate = Some(sample_rate);
        }
        if let SetApp(app) = event {
            let stored = &mut self.presets.settings;
            stored.frame_rate_cap = app.frame_rate_cap;
            stored.check_for_updates = app.check_for_updates;
            self.presets.save_settings();
        }
        self.handle_event(event, now);
    }

    fn handle_preset(&mut self, event: Event, now: Duration) {
        // Whatever is pending goes into the current Preset first.
        self.save_now();
        let open = match event {
            Event::SwitchPreset { index } => self.presets.switch(index),
            Event::PresetShortcut { number } => self.presets.shortcut(number),
            Event::RevertPreset => self.presets.opened().cloned(),
            Event::SavePresetAsNew => {
                let data = self.current_data();
                self.presets.save_as_new(data)
            }
            Event::DuplicatePreset { index } => {
                self.presets.duplicate(index);
                None
            }
            Event::RenamePreset { index, name } => {
                self.presets.rename(index, name);
                None
            }
            Event::DeletePreset { index } => self.presets.delete(index),
            Event::MovePreset { from, to } => {
                self.presets.reorder(from, to);
                None
            }
            Event::ResetPreset { index } => self.presets.reset(index),
            Event::ImportPreset(text) => self.import(text, now),
            _ => None,
        };
        if let Some(data) = open {
            self.apply(data, now);
            // Reverting saves the opened state back.
            if matches!(event, Event::RevertPreset) {
                self.touched = true;
                self.save_after = Some(now);
            }
        }
    }

    /// The current Preset as a `.dasmeter-preset` file: its name (for the
    /// file) and contents, with the custom Themes it uses.
    pub fn export(&self) -> Option<(String, String)> {
        let name = self.presets.current_name()?;
        let data = PresetData {
            name: name.clone(),
            ..self.current_data()
        };
        let mut themes: Vec<Theme> = Vec::new();
        for theme_name in [&data.theme.light, &data.theme.dark] {
            if let Some((theme, false)) = self.themes.get(theme_name)
                && !themes.iter().any(|t| t.name == theme.name)
            {
                themes.push(theme.clone());
            }
        }
        let file = sharing::SharedPreset::new(data, themes);
        Some((name, file.to_toml()))
    }

    /// Imports a shared Preset: its Themes first (reused if identical,
    /// "(2)" if they differ, installed if new), then the Preset under a free
    /// name. Returns it to open; a bad file changes nothing.
    fn import(&mut self, text: &str, now: Duration) -> Option<PresetData> {
        let (mut data, themes) = match sharing::SharedPreset::from_toml(text) {
            Ok(file) => file,
            Err(_) => {
                self.show_note(Note::ImportFailed, now);
                return None;
            }
        };
        for theme in themes {
            if let Some((existing, _)) = self.themes.get(&theme.name)
                && existing.palette == theme.palette
                && existing.styling == theme.styling
            {
                continue;
            }
            let original = theme.name.clone();
            let installed = self.themes.install(theme);
            for reference in [&mut data.theme.light, &mut data.theme.dark] {
                if *reference == original {
                    reference.clone_from(&installed);
                }
            }
        }
        self.presets.import(data)
    }

    fn show_note(&mut self, note: Note, now: Duration) {
        self.note = note;
        self.note_until = Some(now + NOTE_DURATION);
    }

    /// Saves a pending change now, as the app quits.
    pub fn save_pending(&mut self) {
        self.save_now();
    }

    /// Saves the current state into the current Preset if it changed.
    fn save_now(&mut self) {
        if self.save_after.take().is_some() || self.touched {
            let data = self.current_data();
            self.presets.save_current(data);
        }
        self.touched = false;
    }

    /// The app's state as a Preset holds it.
    pub fn current_data(&self) -> PresetData {
        let (light, dark) = self.themes.choice();
        PresetData {
            version: presets::PRESET_VERSION,
            name: String::new(),
            built_in: None,
            mode: self.mode,
            listen_to: self.listen_to,
            theme: ThemeRef { light, dark },
            bar: self.layout.clone(),
            window: self.window.clone(),
            meters: self
                .meters
                .iter()
                .map(|slot| MeterPreset {
                    settings: slot.meter.settings(),
                    send_plugin: slot.pick.clone(),
                    show_source_label: slot.show_source_label,
                    overrides: slot
                        .overrides
                        .iter()
                        .map(|&(role, colour)| RoleColour { role, colour })
                        .collect(),
                })
                .collect(),
        }
    }

    /// Whether the state differs from the current Preset as it was opened.
    fn changed_since_opened(&self) -> bool {
        let Some(opened) = self.presets.opened() else {
            return false;
        };
        let current = PresetData {
            name: opened.name.clone(),
            built_in: opened.built_in,
            ..self.current_data()
        };
        current != *opened
    }

    /// Puts a Preset's layout, Meters, Listen to and Theme into effect.
    fn apply(&mut self, data: PresetData, now: Duration) {
        let data = data.sanitised();
        self.meters = data
            .meters
            .iter()
            .map(|m| MeterSlot {
                meter: Meter::new(m.settings.clamped()),
                pick: m.send_plugin.clone(),
                show_source_label: m.show_source_label,
                overrides: m.overrides.iter().map(|o| (o.role, o.colour)).collect(),
                showing: None,
            })
            .collect();
        self.layout = data.bar;
        // A file from Windows can't reserve screen space on macOS.
        if self.layout.screen == ScreenMode::ReserveSpace && self.platform == Platform::MacOs {
            self.layout.screen = ScreenMode::FloatOnTop;
        }
        self.window = data.window;
        self.mode = data.mode;
        self.menu = None;
        self.themes
            .choose_names(&data.theme.light, &data.theme.dark);
        if data.listen_to != self.listen_to {
            self.listen_to = data.listen_to;
            self.capture = Capture::Starting;
        }
        match self.listen_to {
            ListenTo::SystemCapture => {
                if let (Capture::Live, Some(rate)) = (&self.capture, self.capture_rate) {
                    for slot in &mut self.meters {
                        slot.meter.start(rate);
                    }
                }
            }
            ListenTo::SendPlugins => self.route(now),
        }
        self.touched = false;
        self.save_after = None;
        self.changed = true;
    }

    /// The disk work Presets need since the last call.
    pub fn take_preset_ops(&mut self) -> Vec<PresetOp> {
        self.presets.take_ops()
    }

    fn handle_event(&mut self, event: Event, now: Duration) {
        match event {
            Event::CaptureStarted { sample_rate } => {
                let restarted = matches!(self.capture, Capture::Live);
                self.capture = Capture::Live;
                if self.listen_to != ListenTo::SystemCapture {
                    return;
                }
                if restarted {
                    self.show_note(Note::OutputChanged, now);
                }
                for slot in &mut self.meters {
                    slot.meter.start(sample_rate);
                }
            }
            Event::CaptureFailed(reason) => {
                self.capture = Capture::Failed(reason.to_owned());
                if self.listen_to != ListenTo::SystemCapture {
                    return;
                }
                for slot in &mut self.meters {
                    slot.meter.stop();
                }
            }
            Event::Audio(frames) => {
                if self.listen_to != ListenTo::SystemCapture {
                    return;
                }
                if !self.heard(frames, now) {
                    return;
                }
                let fed = self.fed_meters();
                for (i, slot) in self.meters.iter_mut().enumerate() {
                    if fed.contains(&i) {
                        slot.meter.process(frames);
                    }
                }
            }
            Event::Visible(visible) => self.visible = visible,
            Event::Pointer(pointer) => {
                if pointer == self.pointer {
                    return;
                }
                self.pointer = pointer;
                for slot in &mut self.meters {
                    slot.meter.pointer_changed();
                }
            }
            Event::SetMeter { meter, settings } => match self.meters.get_mut(meter) {
                Some(slot) => slot.meter.set_settings(settings.clamped()),
                None => return,
            },
            Event::SetListenTo(listen_to) => {
                if listen_to == self.listen_to {
                    return;
                }
                self.listen_to = listen_to;
                // System Capture starts over when it comes back; Send Plugins
                // are routed from the last list until the next one arrives.
                self.capture = Capture::Starting;
                for slot in &mut self.meters {
                    slot.meter.stop();
                    slot.showing = None;
                }
                self.route(now);
            }
            Event::SendPlugins(listed) => {
                self.send_plugins = listed.to_vec();
                self.follow_renames();
                self.route(now);
            }
            Event::SendPluginAudio { id, frames } => {
                let wanted = self.fed_meters();
                let mut fed = false;
                for (i, slot) in self.meters.iter_mut().enumerate() {
                    if !wanted.contains(&i) {
                        continue;
                    }
                    if let Some(showing) = slot.showing.as_mut().filter(|s| s.id == id) {
                        slot.meter.process(frames);
                        showing.fed_at = now;
                        fed = true;
                    }
                }
                if !fed {
                    return;
                }
            }
            Event::PickSendPlugin { meter, id } => {
                let Some(plugin) = self.send_plugins.iter().find(|p| p.id == id) else {
                    return;
                };
                if plugin.outdated || meter >= self.meters.len() {
                    return;
                }
                self.meters[meter].pick = Some(Pick::of(plugin));
                self.route(now);
            }
            Event::UseForAllMeters { meter } => {
                let Some(pick) = self.shown_pick(meter) else {
                    return;
                };
                for slot in &mut self.meters {
                    slot.pick = Some(pick.clone());
                }
                self.route(now);
            }
            Event::ShowSourceLabel { meter, shown } => match self.meters.get_mut(meter) {
                Some(slot) => slot.show_source_label = shown,
                None => return,
            },
            Event::Click { window, at } => {
                if self.menu.take().is_some() {
                    self.changed = true;
                    return;
                }
                if let Some((meter, id)) = self.item_at(window, at) {
                    self.handle(Event::PickSendPlugin { meter, id }, now);
                } else if let Some(meter) = self.meter_at(window, at) {
                    self.handle(Event::ResetLoudness { meter }, now);
                }
                return;
            }
            Event::OpenMenu { window, at } => {
                let Some(meter) = self.meter_at(window, at) else {
                    return;
                };
                self.menu = Some(MeterMenu { meter, window, at });
            }
            Event::Display(display) => {
                let screen = Screen {
                    display,
                    fingerprint: None,
                    name: String::new(),
                    main: true,
                };
                if self.screens == [screen.clone()] {
                    return;
                }
                self.screens = vec![screen];
                self.display = Some(display);
            }
            Event::SetEdge(edge) => {
                if edge == self.layout.edge {
                    return;
                }
                self.layout.edge = edge;
            }
            Event::SetBarThickness(thickness) => {
                if thickness.is_nan() {
                    return;
                }
                let thickness = thickness.clamp(layout::MIN_THICKNESS, 4_000.0);
                // What the user sees is capped; storing more than that would
                // make the next drag jump.
                let shown = match self.bar_display() {
                    Some(display) => {
                        let mut capped = self.layout.clone();
                        capped.thickness = thickness;
                        capped.thickness_on(&display)
                    }
                    None => thickness,
                };
                if shown == self.layout.thickness {
                    return;
                }
                self.layout.thickness = shown;
            }
            Event::MoveBarEnd { end, at } => {
                let Some(display) = self.bar_display() else {
                    return;
                };
                if !self.layout.move_end(end, at, &display) {
                    return;
                }
            }
            Event::MoveDivider { divider, at } => {
                if !self.layout.move_divider(divider, at) {
                    return;
                }
            }
            Event::PopOut { meter } => {
                let (frame, display) = self.anchor(self.pop_out_frame(meter));
                if !self.layout.pop_out(meter, frame) {
                    return;
                }
                if let Some(pop_out) = self.layout.pop_out_mut(meter) {
                    pop_out.display = display;
                }
                self.menu = None;
            }
            Event::DockBack { meter } => {
                if !self.layout.dock_back(meter) {
                    return;
                }
                if self
                    .menu
                    .is_some_and(|m| m.window == WindowKey::PopOut(meter))
                {
                    self.menu = None;
                }
            }
            Event::PopOutMoved { meter, frame } => {
                let (min_w, min_h) = layout::MIN_POP_OUT;
                let frame = Rect {
                    width: frame.width.max(min_w),
                    height: frame.height.max(min_h),
                    ..frame
                };
                // Last touch wins: wherever the user puts it is its place now.
                let (frame, display) = self.anchor(frame);
                let Some(pop_out) = self.layout.pop_out_mut(meter) else {
                    return;
                };
                if pop_out.frame == frame && pop_out.display == display {
                    return;
                }
                pop_out.frame = frame;
                pop_out.display = display;
            }
            Event::SetPopOutOnTop { meter, on_top } => match self.layout.pop_out_mut(meter) {
                Some(pop_out) if pop_out.on_top != on_top => pop_out.on_top = on_top,
                _ => return,
            },
            Event::CycleScreenMode => {
                self.layout.screen = self.layout.screen.next(self.platform);
            }
            Event::SetMode(mode) => {
                if mode == self.mode {
                    return;
                }
                self.mode = mode;
                self.menu = None;
            }
            Event::SplitPane { meter, direction } => {
                if self.mode != LayoutMode::Window || meter >= self.meters.len() {
                    return;
                }
                let new = self.spare_meter(meter);
                if !self.window.tree.split_pane(meter, direction, new) {
                    return;
                }
                self.menu = None;
            }
            Event::ClosePane { meter } => {
                if self.mode != LayoutMode::Window || !self.window.tree.close_pane(meter) {
                    return;
                }
                self.menu = None;
            }
            Event::MoveSplit { split, at } => {
                let Some(divider) = self
                    .drawn_window(WindowKey::Main)
                    .and_then(|w| w.dividers.iter().find(|d| d.split == split).copied())
                else {
                    return;
                };
                let a = divider.area;
                let ratio = match divider.direction {
                    Direction::SideBySide => (at - a.x) / a.width,
                    Direction::Stacked => (at - a.y) / a.height,
                };
                if !self.window.tree.set_ratio(split, ratio) {
                    return;
                }
            }
            Event::AssignMeter { meter, kind } => {
                let Some(slot) = self.meters.get_mut(meter) else {
                    return;
                };
                if slot.meter.settings().kind() == kind {
                    return;
                }
                slot.meter.set_settings(MeterSettings::default_of(kind));
            }
            Event::ToggleOnTop => match self.mode {
                LayoutMode::Bar => self.layout.screen = self.layout.screen.next(self.platform),
                LayoutMode::Window => self.window.on_top = !self.window.on_top,
            },
            Event::WindowMoved(frame) => {
                let (frame, display) = self.anchor(frame);
                if self.window.frame == Some(frame) && self.window.display == display {
                    return;
                }
                self.window.frame = Some(frame);
                self.window.display = display;
            }
            Event::ThemeFiles(files) => self.themes.load(files),
            Event::SystemAppearance(appearance) => {
                if !self.themes.set_appearance(appearance) {
                    return;
                }
            }
            Event::ChooseTheme { light, dark } => {
                if !self.themes.choose(light, dark) {
                    return;
                }
            }
            Event::DuplicateTheme { theme } => {
                if self.themes.duplicate(theme).is_none() {
                    return;
                }
            }
            Event::SetThemeColour {
                theme,
                role,
                colour,
            } => {
                if !self.themes.set_colour(theme, role, colour) {
                    return;
                }
            }
            Event::SetThemeStyling { theme, styling } => {
                if !self.themes.set_styling(theme, styling) {
                    return;
                }
            }
            Event::SetOverride {
                meter,
                role,
                colour,
            } => {
                let Some(slot) = self.meters.get_mut(meter) else {
                    return;
                };
                let before = slot.overrides.clone();
                slot.overrides.retain(|(r, _)| *r != role);
                if let Some(colour) = colour {
                    slot.overrides.push((role, colour));
                }
                if slot.overrides == before {
                    return;
                }
            }
            // Handled in `handle`.
            Event::PresetFiles { .. }
            | Event::SwitchPreset { .. }
            | Event::PresetShortcut { .. }
            | Event::RevertPreset
            | Event::SavePresetAsNew
            | Event::DuplicatePreset { .. }
            | Event::RenamePreset { .. }
            | Event::DeletePreset { .. }
            | Event::MovePreset { .. }
            | Event::ResetPreset { .. }
            | Event::ImportPreset(_) => return,
            Event::Screens(screens) => {
                if self.screens == screens {
                    return;
                }
                self.screens = screens.to_vec();
                self.display = self.main_display();
            }
            Event::ShowNote(note) => self.show_note(note, now),
            Event::UpdateChecked { at } => {
                self.presets.settings.last_update_check = at;
                self.presets.save_settings();
            }
            Event::KeepHere => {
                if !self.keep_here() {
                    return;
                }
            }
            Event::ShowOverFullscreen(shown) => {
                if shown == self.layout.show_over_fullscreen {
                    return;
                }
                self.layout.show_over_fullscreen = shown;
            }
            Event::CloseMenu => {
                if self.menu.take().is_none() {
                    return;
                }
            }
            Event::ShowSettings(open) => {
                if open == self.settings_open {
                    return;
                }
                self.settings_open = open;
                if open {
                    self.menu = None;
                }
            }
            Event::ResetLoudness { meter } => {
                if !self.meters.get_mut(meter).is_some_and(|s| s.meter.reset()) {
                    return;
                }
            }
            Event::SetApp(app) => {
                self.app = app;
                self.clamp_frame_rate_cap();
            }
            Event::DisplayRefreshRate(rate) => {
                if self.refresh_rate == Some(rate) {
                    return;
                }
                self.refresh_rate = Some(rate);
                self.clamp_frame_rate_cap();
            }
        }
        self.changed = true;
    }

    /// The pick a Meter shows: its own, or the one it follows, or the only
    /// Send Plugin there is.
    fn shown_pick(&self, meter: usize) -> Option<Pick> {
        match self.resolve(meter) {
            Resolved::Plugin(plugin) => Some(Pick::of(plugin)),
            _ => self.meters.get(meter)?.pick.clone(),
        }
    }

    fn resolve(&self, meter: usize) -> Resolved<'_> {
        let own = self.meters[meter].pick.as_ref();
        let followed = self
            .meters
            .iter()
            .enumerate()
            .find_map(|(i, slot)| slot.pick.as_ref().filter(|_| i != meter));
        sources::resolve(own, followed, &self.send_plugins)
    }

    /// Picks follow their Send Plugin: a rename updates the stored name, and a
    /// pick found only by name (a new ID) takes the new ID.
    fn follow_renames(&mut self) {
        for slot in &mut self.meters {
            if let Some(pick) = &mut slot.pick {
                if let Some(plugin) = sources::find(pick, &self.send_plugins) {
                    *pick = Pick::of(plugin);
                }
            }
        }
    }

    /// Points each Meter's analyser at the Send Plugin it should show, and
    /// feeds idle Send Plugins' Meters silence. Does nothing on System Capture.
    fn route(&mut self, now: Duration) {
        if self.listen_to != ListenTo::SendPlugins {
            return;
        }
        // With exactly one Send Plugin and no picks, every Meter takes it, and
        // keeps it when more appear.
        if self.meters.iter().all(|slot| slot.pick.is_none()) {
            let mut usable = self.send_plugins.iter().filter(|p| p.usable());
            if let (Some(only), None) = (usable.next(), usable.next()) {
                let pick = Pick::of(only);
                for slot in &mut self.meters {
                    slot.pick = Some(pick.clone());
                }
            }
        }
        for meter in 0..self.meters.len() {
            let target = match self.resolve(meter) {
                Resolved::Plugin(plugin) => {
                    Some((plugin.id, plugin.sample_rate, plugin.mono, plugin.state))
                }
                _ => None,
            };
            let slot = &mut self.meters[meter];
            let Some((id, sample_rate, mono, state)) = target else {
                if slot.showing.take().is_some() {
                    slot.meter.stop();
                }
                continue;
            };
            match slot.showing {
                Some(showing) if showing.id == id && showing.sample_rate == sample_rate => {}
                previous => {
                    // Same Send Plugin at a new rate: reset like an output change.
                    if previous.is_some_and(|p| p.id == id) {
                        self.note = Note::OutputChanged;
                        self.note_until = Some(now + NOTE_DURATION);
                    }
                    slot.meter.start(sample_rate);
                    slot.showing = Some(Showing {
                        id,
                        sample_rate,
                        fed_at: now,
                    });
                }
            }
            slot.meter.set_mono(mono);
            if state == SendPluginState::Idle {
                // An idle Send Plugin sends nothing: its Meters fall silent as
                // they would on quiet audio.
                if let Some(showing) = &mut slot.showing {
                    let seconds = now
                        .saturating_sub(showing.fed_at)
                        .as_secs_f64()
                        .min(MAX_IDLE_FEED_SECONDS);
                    let frames = (seconds * f64::from(sample_rate)) as usize;
                    feed_silence(&mut slot.meter, frames);
                    showing.fed_at = now;
                }
            }
        }
    }

    /// The Meters the current layout shows.
    fn shown_meters(&self) -> Vec<usize> {
        match self.mode {
            LayoutMode::Bar => self
                .layout
                .meters
                .iter()
                .map(|(m, _)| *m)
                .chain(self.layout.pop_outs.iter().map(|p| p.meter))
                .collect(),
            LayoutMode::Window => self.window.tree.meters(),
        }
    }

    /// Notes silence and sound. Returns false for a silent block once the
    /// Meters have settled: it would change nothing, so it's skipped, and the
    /// app stays asleep until something audible comes.
    fn heard(&mut self, frames: &[f32], now: Duration) -> bool {
        if frames.iter().any(|&x| x != 0.0) {
            self.silent_since = None;
            self.quiet = false;
            return true;
        }
        self.silent_since.get_or_insert(now);
        !(self.quiet && self.visible)
    }

    /// The longest peak hold of any Meter: how long silence must last before
    /// nothing more can change. (An infinite hold never changes on its own.)
    fn longest_hold(&self) -> Duration {
        self.meters
            .iter()
            .filter_map(|slot| match slot.meter.settings() {
                MeterSettings::Loudness(s) => Some(s.analysis.peak_hold),
                MeterSettings::Spectrum(s) => Some(s.analysis.peak_hold),
                _ => None,
            })
            .filter_map(|hold| match hold {
                dasmeter_analysis::PeakHold::For(time) => Some(time),
                dasmeter_analysis::PeakHold::Infinite => None,
            })
            .max()
            .unwrap_or_default()
    }

    /// The Meters audio goes to: the ones the layout shows, and while nothing
    /// can be seen only the Loudness Meters among them, whose integrated LUFS
    /// and peak hold must keep counting.
    fn fed_meters(&self) -> Vec<usize> {
        let mut shown = self.shown_meters();
        if !self.visible {
            shown
                .retain(|&i| matches!(self.meters[i].meter.settings(), MeterSettings::Loudness(_)));
        }
        shown
    }

    /// A Meter for a new pane: one no layout shows, else a new one. Either
    /// way it becomes a copy of `like` (settings, pick, Source label) and
    /// starts at its rate.
    fn spare_meter(&mut self, like: usize) -> usize {
        let used: Vec<usize> = self
            .window
            .tree
            .meters()
            .into_iter()
            .chain(self.layout.meters.iter().map(|(m, _)| *m))
            .chain(self.layout.pop_outs.iter().map(|p| p.meter))
            .collect();
        let settings = self.meters[like].meter.settings();
        let slot = MeterSlot {
            meter: Meter::new(settings),
            pick: self.meters[like].pick.clone(),
            show_source_label: self.meters[like].show_source_label,
            overrides: self.meters[like].overrides.clone(),
            showing: None,
        };
        let index = match (0..self.meters.len()).find(|i| !used.contains(i)) {
            Some(i) => {
                self.meters[i] = slot;
                i
            }
            None => {
                self.meters.push(slot);
                self.meters.len() - 1
            }
        };
        match self.listen_to {
            ListenTo::SystemCapture => {
                if let Some(rate) = self.meters[like].meter.sample_rate() {
                    self.meters[index].meter.start(rate);
                }
            }
            ListenTo::SendPlugins => {
                let now = self.drawn_at.unwrap_or_default();
                self.route(now);
            }
        }
        index
    }

    fn drawn_window(&self, key: WindowKey) -> Option<&WindowScene> {
        self.drawn.as_ref()?.windows.iter().find(|w| w.key == key)
    }

    /// The pickable item in a "Pick a Send Plugin" list on screen at `point`.
    fn item_at(&self, window: WindowKey, point: [f32; 2]) -> Option<(usize, u64)> {
        self.drawn_window(window)?
            .meters
            .iter()
            .find_map(|scene| match &scene.state {
                MeterState::PickSendPlugin(items) => items
                    .iter()
                    .find(|item| item.pickable && item.frame.locate(point).is_some())
                    .map(|item| (scene.meter, item.id)),
                _ => None,
            })
    }

    /// The Meter at `point` (window fractions) in the last scene drawn.
    fn meter_at(&self, window: WindowKey, point: [f32; 2]) -> Option<usize> {
        self.drawn_window(window)?
            .meters
            .iter()
            .find(|meter| meter.frame.locate(point).is_some())
            .map(|meter| meter.meter)
    }

    /// Decides whether to draw now. On [`Decision::Draw`], the shell draws
    /// [`AppCore::scene`]; the core counts that as drawn at `now`.
    pub fn decide(&mut self, now: Duration) -> Decision {
        if self.save_after.is_some_and(|at| now >= at) {
            self.save_now();
            // Revert's offer follows the saved state.
            self.changed = true;
        }
        if !self.visible {
            return Decision::Sleep {
                until: self.save_after,
            };
        }
        let note_until = self.note_until.filter(|&until| until > now);
        let note_expired = self.note_until.is_some() && note_until.is_none();
        if !self.changed && !note_expired && self.drawn.is_some() {
            return Decision::Sleep {
                until: earliest(note_until, self.save_after),
            };
        }
        // Wait for the next frame before building a scene at all.
        if let Some(drawn_at) = self.drawn_at {
            let next_frame = drawn_at + self.app.frame_interval();
            if now < next_frame {
                return Decision::Sleep {
                    until: Some(next_frame),
                };
            }
        }
        if note_expired {
            self.note_until = None;
        }
        self.changed = false;
        let scene = self.build_scene(note_until.is_some());
        if self.drawn.as_ref() == Some(&scene) {
            // Nothing visible changes any more, and the silence has outlasted
            // every peak hold: the Meters have settled.
            self.quiet = self.silent_since.is_some_and(|since| {
                now.saturating_sub(since) >= self.longest_hold() + SETTLE_MARGIN
            });
            return Decision::Sleep {
                until: earliest(note_until, self.save_after),
            };
        }
        self.drawn = Some(scene);
        self.drawn_at = Some(now);
        Decision::Draw
    }

    /// The scene to draw: the one from the last [`Decision::Draw`], or `None`
    /// before the first.
    pub fn scene(&self) -> Option<&Scene> {
        self.drawn.as_ref()
    }

    /// Bar mode's windows: the Bar and its Pop-outs, and where each Meter goes.
    fn bar_windows(&self) -> (Vec<(WindowKey, usize, Frame)>, Vec<WindowScene>) {
        let bar = &self.layout;
        let mut placed: Vec<(WindowKey, usize, Frame)> = Vec::new();
        let mut start = 0.0;
        for &(meter, share) in &bar.meters {
            let frame = if bar.edge.horizontal() {
                Frame {
                    x: start,
                    y: 0.0,
                    width: share,
                    height: 1.0,
                }
            } else {
                Frame {
                    x: 0.0,
                    y: start,
                    width: 1.0,
                    height: share,
                }
            };
            start += share;
            placed.push((WindowKey::Bar, meter, frame));
        }
        for pop_out in &bar.pop_outs {
            placed.push((WindowKey::PopOut(pop_out.meter), pop_out.meter, WHOLE));
        }
        let mut windows = vec![WindowScene {
            key: WindowKey::Bar,
            title: "Das-Meter".to_owned(),
            frame: self.bar_display().map(|display| bar.frame_on(&display)),
            on_top: bar.screen != ScreenMode::NormalWindow,
            reserve_space: bar.screen == ScreenMode::ReserveSpace,
            over_fullscreen: bar.show_over_fullscreen,
            screen: Some(bar.screen),
            edge: Some(bar.edge),
            meters: Vec::new(),
            dividers: Vec::new(),
        }];
        for pop_out in &bar.pop_outs {
            windows.push(WindowScene {
                key: WindowKey::PopOut(pop_out.meter),
                title: format!(
                    "Das-Meter · {}",
                    self.meters[pop_out.meter].meter.kind_name()
                ),
                frame: Some(self.pop_out_on_screen(pop_out)),
                on_top: pop_out.on_top,
                reserve_space: false,
                over_fullscreen: false,
                screen: None,
                edge: None,
                meters: Vec::new(),
                dividers: Vec::new(),
            });
        }
        (placed, windows)
    }

    /// Window mode's window, its panes and dividers.
    fn pane_windows(&self) -> (Vec<(WindowKey, usize, Frame)>, Vec<WindowScene>) {
        let (panes, dividers) = self.window.tree.place(WHOLE);
        let placed = panes
            .into_iter()
            .map(|(meter, frame)| (WindowKey::Main, meter, frame))
            .collect();
        let window = WindowScene {
            key: WindowKey::Main,
            title: "Das-Meter".to_owned(),
            frame: self.window_on_screen(),
            on_top: self.window.on_top,
            reserve_space: false,
            over_fullscreen: false,
            screen: None,
            edge: None,
            meters: Vec::new(),
            dividers,
        };
        (placed, vec![window])
    }

    fn build_scene(&mut self, note: bool) -> Scene {
        let (placed, mut windows) = match self.mode {
            LayoutMode::Bar => self.bar_windows(),
            LayoutMode::Window => self.pane_windows(),
        };
        for (key, i, frame) in placed {
            let pointer = self.pointer.filter(|(w, _)| *w == key).map(|(_, p)| p);
            let (state, source) = match self.listen_to {
                ListenTo::SystemCapture => {
                    let state = match &self.capture {
                        Capture::Starting => MeterState::Starting,
                        Capture::Failed(reason) => MeterState::Unavailable(reason.clone()),
                        Capture::Live => self.live_state(i, frame, pointer),
                    };
                    let source = SourceLabel {
                        name: "System Capture".to_owned(),
                        colour: None,
                    };
                    (state, Some(source))
                }
                ListenTo::SendPlugins => match self.resolve(i) {
                    Resolved::Plugin(plugin) => {
                        let source = SourceLabel {
                            name: plugin.name.clone(),
                            colour: Some(colour(plugin.colour)),
                        };
                        (self.live_state(i, frame, pointer), Some(source))
                    }
                    Resolved::Waiting(name) => {
                        let source = SourceLabel {
                            name: name.clone(),
                            colour: None,
                        };
                        (MeterState::WaitingFor(name), Some(source))
                    }
                    Resolved::Choose => (MeterState::PickSendPlugin(self.items(frame)), None),
                    Resolved::Nothing => (MeterState::NoSendPlugins, None),
                },
            };
            let slot = &self.meters[i];
            let source = source.filter(|_| slot.show_source_label);
            let picked = match self.listen_to {
                ListenTo::SendPlugins => self.shown_pick(i).map(|pick| pick.id),
                ListenTo::SystemCapture => slot.pick.as_ref().map(|pick| pick.id),
            };
            let window = windows
                .iter_mut()
                .find(|w| w.key == key)
                .expect("placed in a window");
            window.meters.push(MeterScene {
                meter: i,
                frame,
                state,
                source,
                settings: slot.meter.settings(),
                picked,
                show_source_label: slot.show_source_label,
                overrides: slot.overrides.clone(),
            });
        }
        let send_plugins = self
            .send_plugins
            .iter()
            .filter(|p| p.state != SendPluginState::Gone)
            .map(|p| SendPluginItem {
                id: p.id,
                label: p.label(),
                colour: colour(p.colour),
                pickable: !p.outdated,
            })
            .collect();
        Scene {
            windows,
            notes: if note { vec![self.note] } else { Vec::new() },
            palette: self.themes.current().palette.clone(),
            theme: self.themes.scene(),
            presets: self.presets.scene(self.changed_since_opened()),
            missing_displays: self.missing_displays(),
            listen_to: self.listen_to,
            mode: self.mode,
            send_plugins,
            menu: self.menu,
            settings_open: self.settings_open,
            app: self.app,
            max_frame_rate_cap: self.max_frame_rate_cap(),
        }
    }

    fn live_state(&mut self, meter: usize, frame: Frame, pointer: Option<[f32; 2]>) -> MeterState {
        let pointer = pointer.and_then(|p| frame.locate(p));
        match self.meters[meter].meter.view(pointer) {
            Some(view) => MeterState::Live(view.clone()),
            None => MeterState::Starting,
        }
    }

    /// The "Pick a Send Plugin" list for a Meter in `frame`: one row per
    /// listed Send Plugin that isn't gone, below a title row.
    fn items(&self, frame: Frame) -> Vec<SourceItem> {
        let listed: Vec<&SendPlugin> = self
            .send_plugins
            .iter()
            .filter(|p| p.state != SendPluginState::Gone)
            .collect();
        let row = (1.0 / (listed.len() as f32 + 1.0)).min(PICK_ROW);
        listed
            .iter()
            .enumerate()
            .map(|(i, plugin)| SourceItem {
                id: plugin.id,
                label: plugin.label(),
                colour: colour(plugin.colour),
                pickable: !plugin.outdated,
                frame: Frame {
                    x: frame.x,
                    y: frame.y + frame.height * row * (i as f32 + 1.0),
                    width: frame.width,
                    height: frame.height * row,
                },
            })
            .collect()
    }
}

/// Extra silence after the longest peak hold before the Meters count as settled.
const SETTLE_MARGIN: Duration = Duration::from_millis(500);

/// How long after the last change a Preset is saved.
pub const AUTOSAVE_DELAY: Duration = Duration::from_secs(1);

/// The earlier of two optional times.
fn earliest(a: Option<Duration>, b: Option<Duration>) -> Option<Duration> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// A whole window, as fractions of itself.
const WHOLE: Frame = Frame {
    x: 0.0,
    y: 0.0,
    width: 1.0,
    height: 1.0,
};

/// The height of a row in the "Pick a Send Plugin" list, as a fraction of the Meter.
const PICK_ROW: f32 = 0.14;

fn colour(rgb: u32) -> Colour {
    Colour::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

fn feed_silence(meter: &mut Meter, frames: usize) {
    const CHUNK: usize = 4_096;
    let silence = [0.0f32; 2 * CHUNK];
    let mut left = frames;
    while left > 0 {
        let n = left.min(CHUNK);
        meter.process(&silence[..2 * n]);
        left -= n;
    }
}
