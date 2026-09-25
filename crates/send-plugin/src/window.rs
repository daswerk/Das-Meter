//! The plugin window: a name field, the connection status and an
//! **Open Das-Meter** button, drawn with egui on a baseview child window.
//!
//! The look is fixed and neutral; it doesn't follow the DAW or the app theme.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use baseview::{Size, WindowHandle, WindowOpenOptions, WindowScalePolicy};
use egui_baseview::egui::{self, Color32, RichText};
use egui_baseview::{EguiWindow, GraphicsConfig, Queue};
use raw_window_handle::HasRawWindowHandle;

use crate::link::{Link, Status};

/// The window's size in logical pixels. It can't be resized.
pub const WIDTH: u32 = 320;
pub const HEIGHT: u32 = 104;

/// How often an open window rereads the status.
const REFRESH: Duration = Duration::from_millis(250);

/// The app's bundle ID, for **Open Das-Meter** on macOS.
pub const APP_BUNDLE_ID: &str = "com.daswerk.das-meter";

/// The Link, shared by the plugin's main thread and its window.
pub type SharedLink = Arc<Mutex<Link>>;

pub fn lock(link: &SharedLink) -> MutexGuard<'_, Link> {
    link.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the window keeps between frames.
struct Panel {
    link: SharedLink,
    /// The name field's text: the typed name.
    name: String,
    /// The name shown when nothing is typed (track or animal name), as a hint.
    hint: String,
    status: Status,
    refreshed: Option<Instant>,
    /// Why the app couldn't be opened, if it couldn't.
    open_error: Option<String>,
}

impl Panel {
    fn new(link: SharedLink) -> Panel {
        let name = lock(&link).identity().typed_name.clone();
        Panel {
            link,
            name,
            hint: String::new(),
            status: Status::AppNotRunning,
            refreshed: None,
            open_error: None,
        }
    }

    /// Rereads the status and the name shown, at most every [`REFRESH`].
    /// Returns whether either changed.
    fn refresh(&mut self, now: Instant) -> bool {
        if self
            .refreshed
            .is_some_and(|at| now.duration_since(at) < REFRESH)
        {
            return false;
        }
        self.refreshed = Some(now);
        let mut link = lock(&self.link);
        // Hosts without a timer (clap-wrapper's AU) never tick the Link, so the
        // window does while it is open; a tick more or less changes nothing.
        link.tick(now);
        let hint = link.shown_name().unwrap_or_default();
        let changed = self.status != link.status() || self.hint != hint;
        if changed {
            self.status = link.status();
            self.hint = hint.to_owned();
        }
        changed
    }

    fn ui(&mut self, ctx: &egui::Context) {
        // egui-baseview calls this every frame but paints only when asked to,
        // and drops the font texture updates of frames it doesn't paint: so ask
        // for an immediate repaint on a change, never a delayed one, or new
        // glyphs go missing.
        if self.refresh(Instant::now()) {
            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing.y = 10.0;
            ui.horizontal(|ui| {
                ui.label("Name");
                let field = egui::TextEdit::singleline(&mut self.name)
                    .hint_text(self.hint.as_str())
                    .desired_width(f32::INFINITY);
                if ui.add(field).changed() {
                    lock(&self.link).set_typed_name(&self.name);
                    self.refreshed = None;
                }
            });
            ui.horizontal(|ui| {
                let dot = match self.status {
                    Status::Connected => Color32::from_rgb(0x3c, 0xb3, 0x71),
                    Status::AppNotRunning => Color32::GRAY,
                    Status::UpdateApp => Color32::from_rgb(0xe0, 0x9b, 0x2d),
                };
                // Painted: the default fonts have no round bullet.
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, dot);
                ui.label(self.status.text());
            });
            ui.horizontal(|ui| {
                if ui.button("Open Das-Meter").clicked() {
                    self.open_error = open_app().err();
                }
                if let Some(error) = &self.open_error {
                    ui.label(RichText::new(error).small().weak());
                }
            });
        });
    }
}

/// An open plugin window. Closes when dropped.
pub struct Window {
    handle: WindowHandle,
}

impl Window {
    /// Opens the window inside the host's `parent`. Fails only if the
    /// system's UI font can't be read.
    pub fn open(
        parent: &impl HasRawWindowHandle,
        link: SharedLink,
        scale: Option<f64>,
    ) -> std::io::Result<Window> {
        let fonts = system_fonts()?;
        let options = WindowOpenOptions {
            title: "Das-Meter Send".into(),
            size: Size::new(f64::from(WIDTH), f64::from(HEIGHT)),
            scale: scale.map_or(
                WindowScalePolicy::SystemScaleFactor,
                WindowScalePolicy::ScaleFactor,
            ),
            gl_config: None,
        };
        let handle = EguiWindow::open_parented(
            parent,
            options,
            GraphicsConfig::default(),
            Panel::new(link),
            move |ctx: &egui::Context, _queue: &mut Queue, _panel: &mut Panel| {
                ctx.set_fonts(fonts.clone());
                ctx.set_visuals(egui::Visuals::dark());
            },
            |ctx: &egui::Context, _queue: &mut Queue, panel: &mut Panel| panel.ui(ctx),
        );
        Ok(Window { handle })
    }
}

/// The system's UI font, for all text. The window uses it rather than
/// egui's bundled fonts, which come under font licences of their own.
fn system_fonts() -> std::io::Result<egui::FontDefinitions> {
    #[cfg(target_os = "macos")]
    const CANDIDATES: &[&str] = &["/System/Library/Fonts/Helvetica.ttc"];
    #[cfg(target_os = "windows")]
    const CANDIDATES: &[&str] = &[
        r"C:\Windows\Fonts\segoeui.ttf",
        r"C:\Windows\Fonts\arial.ttf",
    ];
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    const CANDIDATES: &[&str] = &[
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ];

    let mut error = std::io::Error::from(std::io::ErrorKind::NotFound);
    for path in CANDIDATES {
        match std::fs::read(path) {
            Ok(bytes) => {
                let mut fonts = egui::FontDefinitions::empty();
                fonts
                    .font_data
                    .insert("system".into(), Arc::new(egui::FontData::from_owned(bytes)));
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    fonts.families.insert(family, vec!["system".into()]);
                }
                return Ok(fonts);
            }
            Err(e) => error = e,
        }
    }
    Err(error)
}

impl Drop for Window {
    fn drop(&mut self) {
        self.handle.close();
    }
}

/// Starts the app, or brings it to the front if it is running. Never called
/// without the user pressing the button: the plugin doesn't launch the app itself.
pub fn open_app() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("/usr/bin/open")
        .args(["-b", APP_BUNDLE_ID])
        .status();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("cmd")
        .args(["/C", "start", "", "Das-Meter"])
        .status();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let status = std::process::Command::new("das-meter").status();

    match status {
        Ok(status) if status.success() => Ok(()),
        Ok(_) => Err("Das-Meter isn't installed".into()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_font_is_found() {
        let fonts = system_fonts().expect("a system UI font");
        assert!(!fonts.font_data["system"].font.is_empty());
    }
}
