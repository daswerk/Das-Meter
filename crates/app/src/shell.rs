//! The platform shell's windows: the Bar and its Pop-outs as the scene lays
//! them out, plus a small window for an open Meter menu and one for the
//! settings panel.
//!
//! The shell is thin: it feeds audio and window events into the app core,
//! makes the windows match the scene, draws when the core says so, and
//! otherwise sleeps.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use dasmeter_core::{
    AppCore, Appearance, BarEnd, Decision, Direction, Edge, Event, ListenTo, MeterScene,
    MeterState, Rect, Scene, SplitId, WindowKey, WindowScene,
};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalPosition, LogicalSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{CursorIcon, Window, WindowAttributes, WindowId, WindowLevel};

use crate::Audio;
use crate::gpu::{Gpu, WindowSurface};
use crate::painter::{Painter, UiPaint};
use crate::send_plugins::{LIST_EVERY, SendPluginInput};
use crate::ui::{Request, Surface, Ui};

pub fn run() {
    let event_loop = EventLoop::<()>::with_user_event()
        .build()
        .expect("create the event loop");
    let proxy = event_loop.create_proxy();
    let mut shell = Shell {
        start: Instant::now(),
        core: AppCore::new(),
        audio: None,
        wake: Arc::new(move || {
            let _ = proxy.send_event(());
        }),
        send_plugins: SendPluginInput::new(),
        windows: HashMap::new(),
        started: false,
        theme_folder: Vec::new(),
        theme_scan_at: Duration::ZERO,
        menu_anchor: None,
        ui_view: None,
        ui_wake: None,
        #[cfg(target_os = "macos")]
        main_menu: None,
    };
    event_loop.run_app(&mut shell).expect("run the event loop");
}

/// What an OS window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Role {
    /// The Bar or a Pop-out, with its Meters.
    Meters(WindowKey),
    /// The open Meter menu.
    Menu,
    /// The settings panel.
    Settings,
}

impl Role {
    fn surface(self) -> Surface {
        match self {
            Role::Meters(key) => Surface::Meters(key),
            Role::Menu => Surface::Menu,
            Role::Settings => Surface::Settings,
        }
    }
}

/// What a drag on the Bar changes.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// The divider after the Bar's n-th Meter.
    Divider(usize),
    /// The Bar's inner edge: its thickness.
    Thickness,
    /// One end of the Bar: its length along the edge.
    End(BarEnd),
    /// A divider between Window mode's panes.
    Split(SplitId, Direction),
}

/// How often the themes folder is looked at, so a Theme file that comes back
/// (or is edited by hand) is picked up.
const THEME_SCAN_EVERY: Duration = Duration::from_secs(2);

/// How close to a divider or the Bar's inner edge the pointer grabs it, in logical px.
const GRAB: f32 = 5.0;

/// The menu window's size for placing it before its content is measured.
/// (It opens at 1 × 1 px and takes its content's size on its first frame, so
/// nothing bigger flashes up.)
const MENU_SIZE: (f32, f32) = (300.0, 420.0);
const SETTINGS_SIZE: (f32, f32) = (460.0, 640.0);

struct AppWindow {
    role: Role,
    egui: egui_winit::State,
    ui: Ui,
    painter: Painter,
    surface: WindowSurface,
    /// The frame last put on the window from the scene, so a move the window
    /// reports back isn't sent to the core as the user's.
    placed: Option<Rect>,
    on_top: Option<bool>,
    over_fullscreen: Option<bool>,
    occluded: bool,
    /// The pointer in logical pixels inside the window.
    cursor: Option<[f32; 2]>,
    /// The handle under the pointer, and the one being dragged.
    handle: Option<Drag>,
    drag: Option<Drag>,
    /// egui had the pointer after the last frame, so it needs one more to let go.
    ui_hot: bool,
    title: String,
    /// The window has had keyboard focus. A new window reports losing focus
    /// before it first gains it, which mustn't close a menu.
    had_focus: bool,
    // Dropped last: the surface must go before the window.
    window: Arc<Window>,
}

impl AppWindow {
    fn logical_size(&self) -> [f32; 2] {
        let size = self
            .window
            .inner_size()
            .to_logical::<f32>(self.window.scale_factor());
        [size.width.max(1.0), size.height.max(1.0)]
    }

    /// Where the window is on screen, in logical pixels.
    fn frame(&self) -> Option<Rect> {
        let scale = self.window.scale_factor();
        let position = self.window.outer_position().ok()?.to_logical::<f32>(scale);
        let [width, height] = self.logical_size();
        Some(Rect {
            x: position.x,
            y: position.y,
            width,
            height,
        })
    }

    fn fraction(&self, [x, y]: [f32; 2]) -> [f32; 2] {
        let [width, height] = self.logical_size();
        [x / width, y / height]
    }
}

struct Shell {
    start: Instant,
    core: AppCore,
    /// System Capture, running only while Listen to is System Capture.
    audio: Option<Audio>,
    wake: Arc<dyn Fn() + Send + Sync>,
    send_plugins: SendPluginInput,
    windows: HashMap<WindowId, AppWindow>,
    started: bool,
    /// The themes folder as last read, and when it's next looked at.
    theme_folder: crate::theme_files::Signature,
    theme_scan_at: Duration,
    /// Where the open menu was right-clicked, on screen.
    menu_anchor: Option<[f32; 2]>,
    /// The scene without the Meters' live content, as the menu and panel last drew it.
    ui_view: Option<Scene>,
    /// When an egui layer asked to be drawn again (an animation, a tooltip).
    ui_wake: Option<Duration>,
    #[cfg(target_os = "macos")]
    main_menu: Option<crate::main_menu::MainMenu>,
}

fn logical(rect: Rect) -> (LogicalPosition<f64>, LogicalSize<f64>) {
    (
        LogicalPosition::new(f64::from(rect.x), f64::from(rect.y)),
        LogicalSize::new(f64::from(rect.width), f64::from(rect.height)),
    )
}

/// `scene` without what live Meters show: all the menu and the panel depend on.
fn ui_view(scene: &Scene) -> Scene {
    let windows = scene
        .windows
        .iter()
        .map(|window| WindowScene {
            meters: window
                .meters
                .iter()
                .map(|meter| MeterScene {
                    state: MeterState::Starting,
                    frame: meter.frame,
                    meter: meter.meter,
                    source: meter.source.clone(),
                    settings: meter.settings,
                    picked: meter.picked,
                    show_source_label: meter.show_source_label,
                    overrides: meter.overrides.clone(),
                })
                .collect(),
            key: window.key,
            title: window.title.clone(),
            frame: window.frame,
            on_top: window.on_top,
            reserve_space: window.reserve_space,
            over_fullscreen: window.over_fullscreen,
            screen: window.screen,
            edge: window.edge,
            dividers: window.dividers.clone(),
        })
        .collect();
    Scene {
        windows,
        notes: Vec::new(),
        palette: scene.palette.clone(),
        theme: scene.theme.clone(),
        presets: scene.presets.clone(),
        missing_displays: scene.missing_displays.clone(),
        listen_to: scene.listen_to,
        mode: scene.mode,
        send_plugins: scene.send_plugins.clone(),
        menu: scene.menu,
        settings_open: scene.settings_open,
        app: scene.app,
        max_frame_rate_cap: scene.max_frame_rate_cap,
    }
}

fn appearance(theme: winit::window::Theme) -> Appearance {
    match theme {
        winit::window::Theme::Light => Appearance::Light,
        winit::window::Theme::Dark => Appearance::Dark,
    }
}

fn close(a: Rect, b: Rect) -> bool {
    (a.x - b.x).abs() < 1.0
        && (a.y - b.y).abs() < 1.0
        && (a.width - b.width).abs() < 1.0
        && (a.height - b.height).abs() < 1.0
}

impl Shell {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    /// Feeds the active Source's audio into the core, starting System Capture
    /// or pausing it to follow Listen to. Returns when to pump again, if a
    /// timer is needed (Send Plugins can't wake the app).
    fn pump(&mut self, now: Duration) -> Option<Duration> {
        match self.core.listen_to() {
            ListenTo::SystemCapture => {
                self.send_plugins.stop();
                let wake = self.wake.clone();
                let audio = self
                    .audio
                    .get_or_insert_with(|| Audio::start(move || wake()));
                audio.pump(&mut self.core, now);
                None
            }
            ListenTo::SendPlugins => {
                // System Capture pauses while listening to Send Plugins.
                self.audio = None;
                self.send_plugins
                    .pump(&mut self.core, now, self.start + now);
                let every = if self.send_plugins.listening() {
                    self.core.app_settings().frame_interval()
                } else {
                    LIST_EVERY
                };
                Some(now + every)
            }
        }
    }

    fn find(&self, role: Role) -> Option<WindowId> {
        self.windows
            .iter()
            .find(|(_, w)| w.role == role)
            .map(|(id, _)| *id)
    }

    fn open(&mut self, event_loop: &ActiveEventLoop, role: Role, attributes: WindowAttributes) {
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("Das-Meter couldn't open a window: {error}");
                return;
            }
        };
        let (gpu, surface) = match pollster::block_on(Gpu::for_window(window.clone())) {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("Das-Meter needs a GPU it can draw with: {error}");
                event_loop.exit();
                return;
            }
        };
        let painter = Painter::new(gpu);
        let ui = Ui::new();
        ui.use_fonts(painter.text.font_system.db());
        let egui = egui_winit::State::new(
            ui.ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(painter.gpu.device.limits().max_texture_dimension_2d as usize),
        );
        window.request_redraw();
        if let Some(theme) = window.theme() {
            let now = self.now();
            self.core
                .handle(Event::SystemAppearance(appearance(theme)), now);
        }
        self.windows.insert(
            window.id(),
            AppWindow {
                role,
                egui,
                ui,
                painter,
                surface,
                placed: None,
                on_top: None,
                over_fullscreen: None,
                occluded: false,
                cursor: None,
                handle: None,
                drag: None,
                ui_hot: false,
                title: String::new(),
                had_focus: false,
                window,
            },
        );
    }

    fn attributes_for(scene: &WindowScene) -> WindowAttributes {
        // See-through when the Theme's background opacity is below 100 %.
        let mut attributes = Window::default_attributes()
            .with_title(scene.title.clone())
            .with_transparent(true);
        if let Some(frame) = scene.frame {
            let (position, size) = logical(frame);
            attributes = attributes.with_position(position).with_inner_size(size);
        } else {
            attributes = attributes.with_inner_size(LogicalSize::new(1200.0, 180.0));
        }
        match scene.key {
            // The Bar is a borderless strip; its edges are its handles.
            WindowKey::Bar => attributes.with_decorations(false).with_resizable(false),
            WindowKey::PopOut(_) => attributes
                .with_min_inner_size(LogicalSize::new(160.0, 120.0))
                .with_resizable(true),
            WindowKey::Main => {
                let attributes = attributes
                    .with_min_inner_size(LogicalSize::new(400.0, 250.0))
                    .with_resizable(true);
                if scene.frame.is_none() {
                    attributes.with_inner_size(LogicalSize::new(1200.0, 720.0))
                } else {
                    attributes
                }
            }
        }
    }

    /// Makes the OS windows match the scene: opens, places and closes them.
    fn sync_windows(&mut self, event_loop: &ActiveEventLoop) {
        let Some(scene) = self.core.scene().cloned() else {
            return;
        };
        // Meter windows.
        for window_scene in &scene.windows {
            let role = Role::Meters(window_scene.key);
            if self.find(role).is_none() {
                self.open(event_loop, role, Shell::attributes_for(window_scene));
            }
            let Some(id) = self.find(role) else { continue };
            let app = self.windows.get_mut(&id).expect("found");
            if let Some(frame) = window_scene.frame {
                let current = app.frame();
                if app.placed != Some(frame) && current.is_none_or(|c| !close(c, frame)) {
                    let (position, size) = logical(frame);
                    app.window.set_outer_position(position);
                    let _ = app.window.request_inner_size(size);
                }
                app.placed = Some(frame);
            }
            if app.on_top != Some(window_scene.on_top) {
                app.window.set_window_level(if window_scene.on_top {
                    WindowLevel::AlwaysOnTop
                } else {
                    WindowLevel::Normal
                });
                app.on_top = Some(window_scene.on_top);
            }
            if app.over_fullscreen != Some(window_scene.over_fullscreen) {
                #[cfg(target_os = "macos")]
                crate::macos::set_over_fullscreen(&app.window, window_scene.over_fullscreen);
                app.over_fullscreen = Some(window_scene.over_fullscreen);
            }
            if app.title != window_scene.title {
                app.window.set_title(&window_scene.title);
                app.title.clone_from(&window_scene.title);
            }
        }
        #[cfg(target_os = "macos")]
        crate::macos::set_dock_icon(!scene.windows.iter().any(|w| w.over_fullscreen));
        let gone: Vec<WindowId> = self
            .windows
            .iter()
            .filter(|(_, w)| match w.role {
                Role::Meters(key) => !scene.windows.iter().any(|s| s.key == key),
                Role::Menu => scene.menu.is_none(),
                Role::Settings => !scene.settings_open,
            })
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            self.windows.remove(&id);
        }

        // The Meter menu, where it was right-clicked.
        if let Some(menu) = scene.menu {
            let parent = self
                .find(Role::Meters(menu.window))
                .and_then(|id| self.windows[&id].frame());
            if let Some(parent) = parent {
                let anchor = [
                    parent.x + menu.at[0] * parent.width,
                    parent.y + menu.at[1] * parent.height,
                ];
                self.menu_anchor = Some(anchor);
                let size = self
                    .find(Role::Menu)
                    .map_or([MENU_SIZE.0, MENU_SIZE.1], |id| {
                        self.windows[&id].logical_size()
                    });
                let at = self.menu_position(anchor, size);
                match self.find(Role::Menu) {
                    Some(id) => self.windows[&id].window.set_outer_position(at),
                    None => {
                        let attributes = Window::default_attributes()
                            .with_title("Meter menu")
                            .with_decorations(false)
                            .with_resizable(false)
                            .with_window_level(WindowLevel::AlwaysOnTop)
                            .with_position(at)
                            .with_inner_size(LogicalSize::new(1.0, 1.0));
                        self.open(event_loop, Role::Menu, attributes);
                    }
                }
            }
        }
        if scene.settings_open && self.find(Role::Settings).is_none() {
            let attributes = Window::default_attributes()
                .with_title("Das-Meter Settings")
                .with_inner_size(LogicalSize::new(SETTINGS_SIZE.0, SETTINGS_SIZE.1))
                .with_min_inner_size(LogicalSize::new(360.0, 240.0));
            self.open(event_loop, Role::Settings, attributes);
        }
        // The menu and the panel show settings, not audio: they redraw only
        // when what they show changes.
        let ui_view = ui_view(&scene);
        let ui_changed = self.ui_view.as_ref() != Some(&ui_view);
        self.ui_view = Some(ui_view);
        for app in self.windows.values() {
            if matches!(app.role, Role::Meters(_)) || ui_changed {
                app.window.request_redraw();
            }
        }
    }

    fn draw(&mut self, id: WindowId) {
        let now = self.now();
        let Some(app) = self.windows.get_mut(&id) else {
            return;
        };
        let Some(frame) = app.surface.frame(&app.painter.gpu) else {
            app.window.request_redraw();
            return;
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut actions = crate::ui::Actions::new();
        let ui = self.core.scene().map(|scene| {
            let input = app.egui.take_egui_input(&app.window);
            let (mut output, done) = app.ui.run(input, scene, app.role.surface());
            actions = done;
            let platform = std::mem::take(&mut output.platform_output);
            app.egui.handle_platform_output(&app.window, platform);
            let delay = output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .map(|viewport| viewport.repaint_delay);
            // egui asks for a repaint after a delay (an animation, a tooltip); a
            // very long one means it has nothing to show.
            if let Some(delay) = delay.filter(|d| *d < Duration::from_secs(60)) {
                let at = now + delay;
                self.ui_wake = Some(self.ui_wake.map_or(at, |w| w.min(at)));
            }
            UiPaint::new(&app.ui.ctx, output)
        });
        app.ui_hot = app.ui.ctx.is_pointer_over_egui();
        let key = match app.role {
            Role::Meters(key) => Some(key),
            _ => None,
        };
        app.painter.paint(
            self.core.scene(),
            key,
            app.window.scale_factor() as f32,
            &view,
            ui,
        );
        app.window.pre_present_notify();
        app.painter.gpu.queue.present(frame);
        // A menu window takes the size of what it shows.
        if let (Role::Menu, Some(size)) = (app.role, app.ui.content_size) {
            let [width, height] = app.logical_size();
            if (size.x - width).abs() > 1.0 || (size.y - height).abs() > 1.0 {
                app.window.request_redraw();
                let _ = app
                    .window
                    .request_inner_size(LogicalSize::new(size.x, size.y));
                if let Some(anchor) = self.menu_anchor {
                    let at = self.menu_position(anchor, [size.x, size.y]);
                    self.windows[&id].window.set_outer_position(at);
                }
            }
        }
        let requests = std::mem::take(&mut actions.requests);
        for action in actions {
            self.core.handle(action, now);
        }
        for request in requests {
            match request {
                Request::RenamePreset { index, name } => {
                    self.core
                        .handle(Event::RenamePreset { index, name: &name }, now);
                }
                Request::ShowPresetInFolder { file_name } => {
                    crate::preset_files::show_in_folder(&file_name);
                }
                Request::ExportPreset => crate::sharing::export(&self.core),
                Request::ImportPreset => crate::sharing::import_chosen(&mut self.core, now),
            }
        }
    }

    /// Where a menu of `size` opened at `anchor` goes: below and right of it,
    /// or above or left where the display ends.
    fn menu_position(&self, [x, y]: [f32; 2], [width, height]: [f32; 2]) -> LogicalPosition<f64> {
        let Some(usable) = self.core.display().map(|d| d.usable) else {
            return LogicalPosition::new(f64::from(x), f64::from(y));
        };
        let x = if x + width <= usable.right() {
            x
        } else {
            x - width
        };
        let y = if y + height <= usable.bottom() {
            y
        } else {
            y - height
        };
        let x = x.clamp(usable.x, (usable.right() - width).max(usable.x));
        let y = y.clamp(usable.y, (usable.bottom() - height).max(usable.y));
        LogicalPosition::new(f64::from(x), f64::from(y))
    }

    /// Reads the themes folder into the core if it changed since last time.
    fn scan_themes(&mut self, now: Duration) {
        self.theme_scan_at = now + THEME_SCAN_EVERY;
        let signature = crate::theme_files::signature();
        if signature == self.theme_folder && now > Duration::ZERO {
            return;
        }
        self.theme_folder = signature;
        let files = crate::theme_files::read();
        self.core.handle(Event::ThemeFiles(&files), now);
    }

    /// Carries out the Preset writes, trashing and settings the core asked for.
    fn save_presets(&mut self) {
        let ops = self.core.take_preset_ops();
        if !ops.is_empty() {
            crate::preset_files::apply(ops);
        }
    }

    /// Saves Theme edits the core made, and remembers the folder as written.
    fn save_themes(&mut self) {
        let writes = self.core.take_writes();
        if !writes.is_empty() {
            crate::theme_files::write(&writes);
            self.theme_folder = crate::theme_files::signature();
        }
    }

    /// The display the Bar lives on, and its refresh rate for the frame-rate cap.
    fn report_display(&mut self) {
        let now = self.now();
        #[cfg(target_os = "macos")]
        {
            let screens = crate::macos::screens();
            if !screens.is_empty() {
                self.core.handle(Event::Screens(&screens), now);
            }
        }
        let rate = self
            .find(Role::Meters(WindowKey::Bar))
            .and_then(|id| self.windows[&id].window.current_monitor())
            .and_then(|monitor| monitor.refresh_rate_millihertz());
        if let Some(millihertz) = rate {
            self.core
                .handle(Event::DisplayRefreshRate((millihertz + 500) / 1000), now);
        }
    }

    /// Whether any window with Meters can be seen.
    fn report_visible(&mut self) {
        let visible = self
            .windows
            .values()
            .any(|w| matches!(w.role, Role::Meters(_)) && !w.occluded);
        let now = self.now();
        self.core.handle(Event::Visible(visible), now);
    }

    /// The Bar's handle under `cursor` (logical px in the Bar window).
    fn handle_at(&self, scene: &Scene, app: &AppWindow, [x, y]: [f32; 2]) -> Option<Drag> {
        if app.role == Role::Meters(WindowKey::Main) {
            let main = scene.windows.iter().find(|w| w.key == WindowKey::Main)?;
            let [width, height] = app.logical_size();
            return main.dividers.iter().find_map(|d| {
                let a = d.area;
                let (left, top) = (a.x * width, a.y * height);
                let (right, bottom) = ((a.x + a.width) * width, (a.y + a.height) * height);
                let near = match d.direction {
                    Direction::SideBySide => {
                        let at = left + d.ratio * a.width * width;
                        (x - at).abs() < GRAB && (top..bottom).contains(&y)
                    }
                    Direction::Stacked => {
                        let at = top + d.ratio * a.height * height;
                        (y - at).abs() < GRAB && (left..right).contains(&x)
                    }
                };
                near.then_some(Drag::Split(d.split, d.direction))
            });
        }
        let bar = scene.windows.iter().find(|w| w.key == WindowKey::Bar)?;
        let edge = bar.edge?;
        let [width, height] = app.logical_size();
        let inner_edge = match edge {
            Edge::Bottom => y < GRAB,
            Edge::Top => y > height - GRAB,
            Edge::Left => x > width - GRAB,
            Edge::Right => x < GRAB,
        };
        if inner_edge {
            return Some(Drag::Thickness);
        }
        let (along, length) = if edge.horizontal() {
            (x, width)
        } else {
            (y, height)
        };
        if along < GRAB {
            return Some(Drag::End(BarEnd::Start));
        }
        if along > length - GRAB {
            return Some(Drag::End(BarEnd::End));
        }
        let n = bar.meters.len();
        bar.meters
            .iter()
            .take(n.saturating_sub(1))
            .enumerate()
            .find_map(|(i, m)| {
                let end = if edge.horizontal() {
                    m.frame.x + m.frame.width
                } else {
                    m.frame.y + m.frame.height
                };
                ((end * length - along).abs() < GRAB).then_some(Drag::Divider(i))
            })
    }

    /// A drag on the Bar moved to `cursor` (logical px in the Bar window).
    fn dragged(&mut self, drag: Drag, id: WindowId, [x, y]: [f32; 2]) {
        let now = self.now();
        let app = &self.windows[&id];
        let [width, height] = app.logical_size();
        if let Drag::Split(split, direction) = drag {
            let at = match direction {
                Direction::SideBySide => x / width,
                Direction::Stacked => y / height,
            };
            self.core.handle(Event::MoveSplit { split, at }, now);
            return;
        }
        let Some(scene) = self.core.scene() else {
            return;
        };
        let Some(bar) = scene.windows.iter().find(|w| w.key == WindowKey::Bar) else {
            return;
        };
        let (Some(edge), Some(frame)) = (bar.edge, bar.frame.or(app.frame())) else {
            return;
        };
        let event = match drag {
            Drag::Divider(divider) => Event::MoveDivider {
                divider,
                at: if edge.horizontal() {
                    x / width
                } else {
                    y / height
                },
            },
            Drag::Split(..) => return,
            Drag::End(end) => {
                // Measured on screen: the window moves while it's dragged.
                let Some(window) = app.frame() else { return };
                Event::MoveBarEnd {
                    end,
                    at: if edge.horizontal() {
                        window.x + x
                    } else {
                        window.y + y
                    },
                }
            }
            Drag::Thickness => {
                // Measured on screen: the window moves while it's dragged.
                let Some(window) = app.frame() else { return };
                let (sx, sy) = (window.x + x, window.y + y);
                Event::SetBarThickness(match edge {
                    Edge::Bottom => frame.bottom() - sy,
                    Edge::Top => sy - frame.y,
                    Edge::Left => sx - frame.x,
                    Edge::Right => frame.right() - sx,
                })
            }
        };
        self.core.handle(event, now);
    }
}

impl ApplicationHandler for Shell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.started {
            return;
        }
        self.started = true;
        self.scan_themes(Duration::ZERO);
        let (files, settings) = crate::preset_files::read();
        self.core.handle(
            Event::PresetFiles {
                files: &files,
                settings: settings.as_deref(),
            },
            Duration::ZERO,
        );
        self.save_presets();
        self.report_display();
        // A first scene, so the Bar opens where it belongs.
        let now = self.now();
        self.core.decide(now);
        self.sync_windows(event_loop);
        self.report_display();
        #[cfg(target_os = "macos")]
        if self.main_menu.is_none() {
            let wake = self.wake.clone();
            self.main_menu = Some(crate::main_menu::MainMenu::install(move || wake()));
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // A change still waiting for its auto-save pause is saved now.
        self.core.save_pending();
        self.save_presets();
        self.save_themes();
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _wake: ()) {
        // Audio or a menu click arrived; about_to_wait feeds it in and decides.
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let now = self.now();
        let Some(app) = self.windows.get_mut(&id) else {
            return;
        };
        let role = app.role;
        // The egui layer sees every event first; a click on it stays its own.
        let response = app.egui.on_window_event(&app.window, &event);
        // egui asks for a repaint after nearly every event, a redraw included;
        // only its own windows, or the Bar's button under the pointer, need one.
        let wants = match role {
            Role::Menu | Role::Settings => true,
            // The Bar's screen button shows while the pointer is over the Bar.
            Role::Meters(_) => {
                app.ui_hot
                    || app.ui.ctx.is_pointer_over_egui()
                    || matches!(
                        event,
                        WindowEvent::CursorEntered { .. } | WindowEvent::CursorLeft { .. }
                    )
            }
        };
        if response.repaint && wants && !matches!(event, WindowEvent::RedrawRequested) {
            app.window.request_redraw();
        }
        let on_ui = response.consumed;

        match event {
            WindowEvent::CloseRequested => match role {
                Role::Meters(WindowKey::Bar | WindowKey::Main) => event_loop.exit(),
                Role::Meters(WindowKey::PopOut(meter)) => {
                    self.core.handle(Event::DockBack { meter }, now)
                }
                Role::Menu => self.core.handle(Event::CloseMenu, now),
                Role::Settings => self.core.handle(Event::ShowSettings(false), now),
            },
            WindowEvent::Resized(size) => {
                app.surface
                    .resize(&mut app.painter.gpu, size.width, size.height);
                app.window.request_redraw();
                self.moved(id);
            }
            WindowEvent::Moved(_) => self.moved(id),
            WindowEvent::ScaleFactorChanged { .. } => {
                app.window.request_redraw();
                self.report_display();
            }
            WindowEvent::Focused(true) => {
                app.had_focus = true;
                // Back in Das-Meter through the Bar or a Pop-out: its other
                // windows come forward too (the Bar floats, so clicking it
                // doesn't bring them on its own).
                #[cfg(target_os = "macos")]
                if matches!(role, Role::Meters(_)) {
                    for other in self.windows.values() {
                        if other.window.id() != id && other.role != Role::Menu {
                            crate::macos::order_front(&other.window);
                        }
                    }
                }
            }
            // Clicking anywhere else closes the menu.
            WindowEvent::Focused(false) if role == Role::Menu && app.had_focus => {
                self.core.handle(Event::CloseMenu, now);
            }
            WindowEvent::ThemeChanged(theme) => {
                self.core
                    .handle(Event::SystemAppearance(appearance(theme)), now);
            }
            WindowEvent::Occluded(occluded) => {
                app.occluded = occluded;
                self.report_visible();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = app.window.scale_factor();
                let cursor = position.to_logical::<f32>(scale);
                let cursor = [cursor.x, cursor.y];
                app.cursor = Some(cursor);
                let Role::Meters(key) = role else { return };
                if let Some(drag) = app.drag {
                    self.dragged(drag, id, cursor);
                    return;
                }
                let point = app.fraction(cursor);
                let scene = self.core.scene();
                let handle = match (key, scene) {
                    (WindowKey::Bar | WindowKey::Main, Some(scene)) => {
                        self.handle_at(scene, &self.windows[&id], cursor)
                    }
                    _ => None,
                };
                let horizontal = scene
                    .and_then(|s| s.windows.iter().find(|w| w.key == WindowKey::Bar))
                    .and_then(|w| w.edge)
                    .is_none_or(Edge::horizontal);
                let app = self.windows.get_mut(&id).expect("still open");
                if handle != app.handle {
                    // A divider moves along the Bar; the inner edge across it.
                    let along = match handle {
                        Some(Drag::Divider(_) | Drag::End(_)) => Some(horizontal),
                        Some(Drag::Thickness) => Some(!horizontal),
                        Some(Drag::Split(_, direction)) => Some(direction == Direction::SideBySide),
                        None => None,
                    };
                    app.window.set_cursor(match along {
                        Some(true) => CursorIcon::ColResize,
                        Some(false) => CursorIcon::RowResize,
                        None => CursorIcon::Default,
                    });
                    app.handle = handle;
                }
                self.core.handle(Event::Pointer(Some((key, point))), now);
            }
            WindowEvent::CursorLeft { .. } => {
                app.cursor = None;
                if matches!(role, Role::Meters(_)) {
                    self.core.handle(Event::Pointer(None), now);
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => app.drag = None,
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } if !on_ui => {
                let Role::Meters(window) = role else { return };
                let Some(cursor) = app.cursor else { return };
                if button == MouseButton::Left && app.handle.is_some() {
                    app.drag = app.handle;
                    return;
                }
                let at = app.fraction(cursor);
                match button {
                    MouseButton::Left => self.core.handle(Event::Click { window, at }, now),
                    MouseButton::Right => self.core.handle(Event::OpenMenu { window, at }, now),
                    _ => {}
                }
            }
            // A .dasmeter-preset file dropped on a Bar, Pop-out or Window.
            WindowEvent::DroppedFile(path) if crate::sharing::is_preset(&path) => {
                crate::sharing::import(&mut self.core, &path, now);
            }
            WindowEvent::RedrawRequested => self.draw(id),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = self.now();
        #[cfg(target_os = "macos")]
        if let Some(menu) = &mut self.main_menu {
            let commands = menu.commands();
            let clicked = !commands.is_empty();
            for command in commands {
                match command {
                    crate::main_menu::Command::Core(event) => self.core.handle(event, now),
                    crate::main_menu::Command::Export => crate::sharing::export(&self.core),
                    crate::main_menu::Command::Import => {
                        crate::sharing::import_chosen(&mut self.core, now);
                    }
                    crate::main_menu::Command::Open(url) => {
                        if let Some(app) = self.windows.values().next() {
                            app.ui.ctx.open_url(egui::OpenUrl::new_tab(url));
                            app.window.request_redraw();
                        }
                    }
                }
            }
            let float_on_top = match self.core.mode() {
                dasmeter_core::LayoutMode::Bar => {
                    self.core.layout().screen == dasmeter_core::ScreenMode::FloatOnTop
                }
                dasmeter_core::LayoutMode::Window => self.core.window_layout().on_top,
            };
            menu.show(
                self.core.listen_to(),
                self.core.mode(),
                float_on_top,
                clicked,
            );
            if let Some(scene) = self.core.scene() {
                menu.show_presets(&scene.presets, clicked);
                menu.show_missing_displays(&scene.missing_displays);
            }
        }
        self.save_themes();
        self.save_presets();
        if now >= self.theme_scan_at {
            self.scan_themes(now);
            // Displays plugged in or out, or rearranged, since the last look.
            self.report_display();
        }
        let pump_at = self.pump(now);
        let decision = self.core.decide(now);
        if let Some(audio) = &self.audio {
            let pace = if !self.core.visible() {
                crate::capture::HIDDEN
            } else if decision == (Decision::Sleep { until: None }) {
                crate::capture::SETTLED
            } else {
                crate::capture::DRAWING
            };
            audio.pace.store(pace, Ordering::Release);
        }
        let wake_at = match decision {
            Decision::Draw => {
                self.sync_windows(event_loop);
                None
            }
            Decision::Sleep { until } => until,
        };
        if self.ui_wake.is_some_and(|at| at <= now) {
            self.ui_wake = None;
            for app in self.windows.values() {
                app.window.request_redraw();
            }
        }
        let wake_at = [wake_at, pump_at, self.ui_wake, Some(self.theme_scan_at)]
            .into_iter()
            .flatten()
            .min();
        match wake_at {
            Some(at) => event_loop.set_control_flow(ControlFlow::WaitUntil(self.start + at)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

impl Shell {
    /// A Pop-out moved or was resized: by the user if it isn't where the
    /// scene last put it.
    fn moved(&mut self, id: WindowId) {
        let Some(app) = self.windows.get(&id) else {
            return;
        };
        let Some(frame) = app.frame() else { return };
        if app.placed.is_some_and(|placed| close(placed, frame)) {
            return;
        }
        let event = match app.role {
            Role::Meters(WindowKey::PopOut(meter)) => Event::PopOutMoved { meter, frame },
            Role::Meters(WindowKey::Main) => Event::WindowMoved(frame),
            _ => return,
        };
        let now = self.now();
        self.core.handle(event, now);
    }
}
