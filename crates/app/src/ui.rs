//! The egui layer: each Meter's menu, the settings panel and the
//! first-launch card, each in its own window (or over the Meters for snapshots).
//!
//! It keeps no settings of its own. Each frame it draws what the scene says
//! and turns what the user changes into app core events.

use std::time::Duration;

use dasmeter_analysis::Scale;
use dasmeter_analysis::spectrum::{MAX_FFT_SIZE, MIN_FFT_SIZE};
use dasmeter_analysis::tonal_balance::{Key, Parts};
use dasmeter_analysis::{
    ChannelView, LongTerm, PeakHold, RmsMode, SpectrumStyle, StereoScaling, StereoView,
    WaveformScale, WindowFunction,
};
use dasmeter_core::KeyChoice;
use dasmeter_core::docs::Topic;
use dasmeter_core::layout::NO_RESERVE_SPACE_ON_MACOS;
use dasmeter_core::settings::{self as limits, MEASUREMENTS_NOTE, MEASUREMENTS_URL};
use dasmeter_core::tonal_balance as limits_tonal;
use dasmeter_core::{
    BigReading, Card, CepstrumMeterSettings, Colour, CycleLength, Direction, Edge, Event,
    LayoutMode, LineWeight, ListenTo, Look, LoudnessMeterSettings, LufsBar, MeterKind, MeterScene,
    MeterSettings, NeedleMeterSettings, OscilloscopeMeterSettings, Palette,
    PhaseScopeMeterSettings, Platform, Role, Scene, ScreenMode, SpectrogramMeterSettings,
    SpectrumColouring, SpectrumMeterSettings, Steadiness, StereoBandsMeterSettings, StereoDrawing,
    StereometerMeterSettings, TonalBalanceMeterSettings, TonalView, TunerMeterSettings,
    WaveformColouring, WaveformMeterSettings, WindowKey,
};
use egui::{Color32, RichText, Slider};

use crate::widgets::{self, Toggle};

/// What the user did this frame, as app core events.
#[derive(Default)]
pub struct Actions {
    /// App core events.
    pub events: Vec<Event<'static>>,
    /// Things the shell does itself, or events that carry typed text.
    pub requests: Vec<Request>,
}

/// What the egui layer asks of the shell besides core events.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    RenamePreset {
        index: usize,
        name: String,
    },
    RenameTheme {
        index: usize,
        name: String,
    },
    /// Save a Spectrum's long-term average as a Reference curve.
    SaveReferenceCurve {
        meter: usize,
        name: String,
    },
    /// Show a Reference curve on a Spectrum, or none.
    ShowReferenceCurve {
        meter: usize,
        name: Option<String>,
    },
    /// Compare a Tonal Balance Meter with a Target, or none.
    ChooseTarget {
        meter: usize,
        name: Option<String>,
    },
    /// End learning: add what was heard to a Target as a song.
    AddLearnedSong {
        meter: usize,
        target: String,
        song: String,
    },
    /// Ask for audio files to add to a Tonal Balance Meter's Target.
    AddTargetFiles {
        meter: usize,
    },
    /// Ask for an audio file to compare in place of the live Source.
    CompareFile {
        meter: usize,
    },
    /// Clear the list of files that couldn't be read.
    DismissUpload {
        meter: usize,
    },
    /// Remove a song from a Target.
    RemoveTargetSong {
        target: String,
        song: usize,
    },
    /// Show a Preset file in Finder (Explorer on Windows).
    ShowPresetInFolder {
        file_name: String,
    },
    /// Ask where to save the current Preset as a `.dasmeter-preset` file.
    ExportPreset,
    /// Ask for a `.dasmeter-preset` file to import.
    ImportPreset,
    /// Install Send Plugin… (macOS: copy it from the app).
    InstallSendPlugin,
    /// Open a docs page ("Learn more").
    Learn(dasmeter_core::docs::Topic),
    /// macOS: open Privacy & Security ▸ Screen & System Audio Recording.
    OpenPrivacySettings,
    /// Windows: look for an update now (macOS has it in the menu bar).
    CheckForUpdates,
    /// Windows: install the update found.
    Update,
}

/// What the settings offer for updates on Windows, where there's no menu
/// bar: set by the shell before each frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpdateOffer {
    /// The newer version found, if any.
    pub available: Option<String>,
    /// A check or an update is under way.
    pub busy: bool,
}

fn update_offer_id() -> egui::Id {
    egui::Id::new("das-meter update offer")
}

/// Hands the settings what they offer for updates.
#[cfg(windows)]
pub fn set_update_offer(ctx: &egui::Context, offer: UpdateOffer) {
    ctx.data_mut(|data| data.insert_temp(update_offer_id(), offer));
}

impl Actions {
    pub fn new() -> Actions {
        Actions::default()
    }
}

impl std::ops::Deref for Actions {
    type Target = Vec<Event<'static>>;

    fn deref(&self) -> &Self::Target {
        &self.events
    }
}

impl std::ops::DerefMut for Actions {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.events
    }
}

impl IntoIterator for Actions {
    type Item = Event<'static>;
    type IntoIter = std::vec::IntoIter<Event<'static>>;

    fn into_iter(self) -> Self::IntoIter {
        self.events.into_iter()
    }
}

/// What a window's egui layer shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// Over a window's Meters: the Bar's screen button.
    Meters(WindowKey),
    /// The open Meter menu, filling its own small window.
    Menu,
    /// The settings panel, filling its own window.
    Settings,
    /// The first-launch card (welcome, a hint or a note), in its own small
    /// window next to the Bar.
    Card,
    /// The menu and the panel over the Meters, for offscreen snapshots.
    Overlay,
}

/// One window's egui context.
pub struct Ui {
    pub ctx: egui::Context,
    palette: Option<(Palette, Look)>,
    /// How big the last frame's content was, in logical pixels, for sizing a
    /// menu window to fit.
    pub content_size: Option<egui::Vec2>,
}

impl Ui {
    pub fn new() -> Ui {
        Ui {
            ctx: egui::Context::default(),
            palette: None,
            content_size: None,
        }
    }

    /// Gives egui the bundled Geist and Geist Mono, with the system's
    /// sans-serif font as fallback for other scripts. (egui's own bundled
    /// fonts carry licences we don't take on.)
    pub fn use_fonts(&self, fonts: &glyphon::fontdb::Database) {
        use glyphon::fontdb::{Family, Query};

        let bundled = |data: &'static [u8]| egui::FontData::from_static(data);
        let [sans, _, mono, _] = crate::gpu::FONTS;
        let mut definitions = egui::FontDefinitions::empty();
        definitions
            .font_data
            .insert("geist".into(), bundled(sans).into());
        definitions
            .font_data
            .insert("geist mono".into(), bundled(mono).into());
        let mut fallback = Vec::new();
        let system = fonts
            .query(&Query {
                families: &[Family::SansSerif],
                ..Query::default()
            })
            .and_then(|id| {
                fonts.with_face_data(id, |data, index| {
                    let mut font = egui::FontData::from_owned(data.to_vec());
                    font.index = index;
                    font
                })
            });
        if let Some(system) = system {
            definitions.font_data.insert("system".into(), system.into());
            fallback.push("system".to_owned());
        }
        let family = |first: &str, second: &str| {
            let mut names = vec![first.to_owned(), second.to_owned()];
            names.extend(fallback.iter().cloned());
            names
        };
        definitions.families.insert(
            egui::FontFamily::Proportional,
            family("geist", "geist mono"),
        );
        definitions
            .families
            .insert(egui::FontFamily::Monospace, family("geist mono", "geist"));
        self.ctx.set_fonts(definitions);
    }

    /// Builds this frame's menus and panel from `scene`. Returns egui's output
    /// (shapes and platform requests) and what the user changed.
    pub fn run(
        &mut self,
        input: egui::RawInput,
        scene: &Scene,
        surface: Surface,
    ) -> (egui::FullOutput, Actions) {
        let look = scene.theme.styling.look;
        if self.palette.as_ref() != Some(&(scene.palette.clone(), look)) {
            let smooth = look == Look::Smooth;
            self.ctx.set_visuals(visuals(&scene.palette, smooth));
            self.ctx.all_styles_mut(|style| {
                // Smooth: ~120 ms fades on menus and controls; readings
                // themselves are never animated.
                style.animation_time = if smooth { 0.12 } else { 0.2 };
            });
            widgets::set_smooth(&self.ctx, smooth);
            self.palette = Some((scene.palette.clone(), look));
        }
        let mut actions = Actions::new();
        let mut content_size = None;
        let output = self.ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            match surface {
                // Nothing over the Meters: Float on Top is in the menu bar
                // and the settings.
                Surface::Meters(_) => {}
                Surface::Menu => {
                    content_size = menu_window(&ctx, scene, &mut actions);
                }
                Surface::Settings => settings_window(ui, scene, &mut actions),
                Surface::Card => {
                    content_size = card_window(&ctx, scene, &mut actions);
                }
                Surface::Overlay => {
                    meter_menu(&ctx, scene, &mut actions);
                    settings_panel(&ctx, scene, &mut actions);
                    card_overlay(&ctx, scene, &mut actions);
                }
            }
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                actions.push(match surface {
                    Surface::Settings => Event::ShowSettings(false),
                    _ => Event::CloseMenu,
                });
            }
        });
        self.content_size = content_size;
        (output, actions)
    }
}

fn colour32(colour: Colour) -> Color32 {
    Color32::from_rgba_unmultiplied(colour.r, colour.g, colour.b, colour.a)
}

/// egui's look, from the colour roles.
fn visuals(palette: &Palette, smooth: bool) -> egui::Visuals {
    let c = |role| colour32(palette[role]);
    let (background, panel, grid, text, accent) = (
        c(Role::Background),
        c(Role::Panel),
        c(Role::Grid),
        c(Role::Text),
        c(Role::Accent),
    );
    let mut v = egui::Visuals::dark();
    v.override_text_color = Some(text);
    v.panel_fill = panel;
    v.window_fill = panel;
    v.window_stroke = egui::Stroke::new(1.0, grid);
    v.extreme_bg_color = background;
    v.faint_bg_color = background;
    v.hyperlink_color = accent;
    v.selection.bg_fill = accent.gamma_multiply(0.45);
    v.selection.stroke = egui::Stroke::new(1.0, accent);
    let widgets = &mut v.widgets;
    widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, grid);
    widgets.noninteractive.fg_stroke.color = text;
    for (state, fill) in [
        (&mut widgets.inactive, grid.gamma_multiply(0.6)),
        (&mut widgets.hovered, grid),
        (&mut widgets.active, accent.gamma_multiply(0.6)),
        (&mut widgets.open, grid),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.fg_stroke.color = text;
    }
    widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent);
    if smooth {
        smooth_visuals(&mut v, panel, grid, text, accent);
    }
    v
}

/// The Smooth Look's menus and settings: rounded, slightly see-through
/// popups with a soft shadow and no hard edge, accent on hover, filled
/// sliders with round handles.
fn smooth_visuals(
    v: &mut egui::Visuals,
    panel: Color32,
    grid: Color32,
    text: Color32,
    accent: Color32,
) {
    let black = Color32::BLACK;
    let raised = panel.lerp_to_gamma(Color32::WHITE, 0.03);
    v.window_fill = raised;
    v.panel_fill = panel;
    v.window_stroke = egui::Stroke::new(1.0, grid.gamma_multiply(0.5));
    v.window_corner_radius = egui::CornerRadius::same(10);
    v.menu_corner_radius = egui::CornerRadius::same(10);
    let shadow = egui::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: black.gamma_multiply(0.45),
    };
    v.window_shadow = shadow;
    v.popup_shadow = shadow;
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Circle;
    v.selection.bg_fill = accent.gamma_multiply(0.55);
    v.selection.stroke = egui::Stroke::new(1.0, accent);
    let widgets = &mut v.widgets;
    widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, grid.gamma_multiply(0.5));
    widgets.noninteractive.fg_stroke.color = text.gamma_multiply(0.8);
    for (state, fill) in [
        (&mut widgets.inactive, grid.gamma_multiply(0.45)),
        (&mut widgets.hovered, accent.gamma_multiply(0.22)),
        (&mut widgets.active, accent.gamma_multiply(0.4)),
        (&mut widgets.open, grid.gamma_multiply(0.6)),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.corner_radius = egui::CornerRadius::same(6);
        state.bg_stroke = egui::Stroke::NONE;
        state.fg_stroke.color = text;
    }
    widgets.noninteractive.corner_radius = egui::CornerRadius::same(6);
    widgets.hovered.fg_stroke.color = text;
}

fn kind_name(settings: &MeterSettings) -> &'static str {
    settings.kind_name()
}

/// Every Meter in the scene, whichever window it's in, in Meter order.
fn all_meters(scene: &Scene) -> Vec<&MeterScene> {
    let mut meters: Vec<&MeterScene> = scene.windows.iter().flat_map(|w| &w.meters).collect();
    meters.sort_by_key(|m| m.meter);
    meters
}

fn meter_scene(scene: &Scene, meter: usize) -> Option<&MeterScene> {
    scene
        .windows
        .iter()
        .flat_map(|w| &w.meters)
        .find(|m| m.meter == meter)
}

/// A row of mutually exclusive choices. Returns whether the value changed.
fn choice<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    options: &[(T, &str)],
) -> bool {
    let before = *value;
    ui.horizontal_wrapped(|ui| {
        ui.label(label);
        for &(option, name) in options {
            ui.selectable_value(value, option, name);
        }
    });
    *value != before
}

/// A slider over a time, shown in ms or s. Returns whether it changed.
fn duration_slider(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Duration,
    (low, high): (Duration, Duration),
    unit_seconds: bool,
) -> bool {
    let (scale, suffix) = if unit_seconds {
        (1.0, " s")
    } else {
        (1_000.0, " ms")
    };
    let mut shown = value.as_secs_f64() * scale;
    let changed = ui
        .add(
            Slider::new(
                &mut shown,
                low.as_secs_f64() * scale..=high.as_secs_f64() * scale,
            )
            .text(label)
            .suffix(suffix)
            .logarithmic(high > low * 20)
            // From zero, a log scale would spend half its travel below
            // 1 ms; start the curve at one step of what's shown instead.
            .smallest_positive(if unit_seconds { 0.1 } else { 1.0 })
            .max_decimals(if unit_seconds { 1 } else { 0 }),
        )
        .changed();
    if changed {
        *value = Duration::from_secs_f64(shown / scale);
    }
    changed
}

/// Peak hold: a time, or held until reset.
fn peak_hold(ui: &mut egui::Ui, value: &mut PeakHold) -> bool {
    let mut infinite = *value == PeakHold::Infinite;
    let mut changed = ui
        .add(Toggle::new(&mut infinite, "Hold peaks until reset"))
        .changed();
    let mut time = match *value {
        PeakHold::For(time) => time,
        PeakHold::Infinite => Duration::from_secs(2),
    };
    if !infinite {
        changed |= duration_slider(ui, "Peak hold", &mut time, limits::PEAK_HOLD, true);
    }
    if changed {
        *value = if infinite {
            PeakHold::Infinite
        } else {
            PeakHold::For(time)
        };
    }
    changed
}

/// A pair of sliders for a range, as floor and top. Returns whether it changed.
fn range_sliders(
    ui: &mut egui::Ui,
    labels: (&str, &str),
    range: &mut (f64, f64),
    limits: (f64, f64),
    suffix: &str,
) -> bool {
    let a = ui
        .add(
            Slider::new(&mut range.0, limits.0..=limits.1)
                .text(labels.0)
                .suffix(suffix),
        )
        .changed();
    let b = ui
        .add(
            Slider::new(&mut range.1, limits.0..=limits.1)
                .text(labels.1)
                .suffix(suffix),
        )
        .changed();
    a || b
}

const CHANNEL_VIEWS: [(ChannelView, &str); 3] = [
    (ChannelView::Mono, "Mono"),
    (ChannelView::LeftRight, "L/R"),
    (ChannelView::MidSide, "M/S"),
];

// ---- Basic settings: the ones a Meter's menu holds (at most four each). ----

fn waveform_basic(ui: &mut egui::Ui, s: &mut WaveformMeterSettings) -> bool {
    let mut changed = duration_slider(
        ui,
        "Time span",
        &mut s.analysis.span,
        limits::WAVEFORM_SPAN,
        true,
    );
    changed |= choice(ui, "Channels", &mut s.analysis.channel_view, &CHANNEL_VIEWS);
    changed |= choice(
        ui,
        "Colour",
        &mut s.colouring,
        &[
            (WaveformColouring::Rgb, "RGB"),
            (WaveformColouring::Rekordbox, "Rekordbox"),
            (WaveformColouring::Solid, "Solid"),
        ],
    );
    changed
}

fn spectrum_basic(ui: &mut egui::Ui, s: &mut SpectrumMeterSettings) -> bool {
    let mut changed = choice(ui, "Channels", &mut s.analysis.channel_view, &CHANNEL_VIEWS);
    let slopes = limits::SPECTRUM_SLOPES.map(|slope| {
        (
            slope,
            match slope {
                0.0 => "0",
                3.0 => "3",
                4.5 => "4.5",
                _ => "6",
            },
        )
    });
    changed |= choice(ui, "Slope dB/oct", &mut s.analysis.slope, &slopes);
    let mut bars = matches!(s.analysis.style, SpectrumStyle::Bars { .. });
    if choice(ui, "Style", &mut bars, &[(false, "Line"), (true, "Bars")]) {
        s.analysis.style = if bars {
            SpectrumStyle::Bars {
                bands_per_octave: limits::DEFAULT_SPECTRUM_BANDS,
            }
        } else {
            SpectrumStyle::Line {
                points: limits::SPECTRUM_LINE_POINTS,
            }
        };
        changed = true;
    }
    changed |= ui
        .add(Toggle::new(&mut s.show_peak_hold, "Peak hold"))
        .changed();
    changed |= ui
        .add(Toggle::new(&mut s.show_peak_line, "Peak line"))
        .on_hover_text("A line on the loudest peak, with its frequency and note")
        .changed();
    changed |= ui
        .add(Toggle::new(&mut s.show_cursor, "Mouse readout"))
        .on_hover_text("A line under the mouse, with its frequency and note")
        .changed();
    changed |= ui
        .add(Toggle::new(&mut s.slow_on_hover, "Slow down under the mouse"))
        .on_hover_text("Under the mouse the Spectrum falls slower and keeps its peaks, marked with their notes")
        .changed();
    changed |= choice(
        ui,
        "Colour",
        &mut s.colouring,
        &[
            (SpectrumColouring::Plain, "Plain"),
            (SpectrumColouring::SteadyHarmonics, "Steady harmonics"),
        ],
    );
    changed |= choice(
        ui,
        "Long-term average",
        &mut s.analysis.long_term,
        &[
            (LongTerm::Off, "Off"),
            (LongTerm::Seconds(3.0), "3 s"),
            (LongTerm::Seconds(10.0), "10 s"),
            (LongTerm::Seconds(30.0), "30 s"),
            (LongTerm::Whole, "Whole"),
        ],
    );
    changed
}

/// A Spectrum's Reference curve: which one it shows, saving its long-term
/// average as one, and starting that average over.
fn reference_items(ui: &mut egui::Ui, scene: &Scene, meter: &MeterScene, actions: &mut Actions) {
    let MeterSettings::Spectrum(settings) = meter.settings else {
        return;
    };
    let shown = meter.reference.as_ref().map(|r| r.name.clone());
    let long_term = settings.analysis.long_term != LongTerm::Off;
    if long_term
        && ui
            .button("Reset long-term average")
            .on_hover_text("Start the long-term average over from now")
            .clicked()
    {
        actions.push(Event::ResetLongTerm { meter: meter.meter });
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("Reference curve");
        let mut pick = |ui: &mut egui::Ui, name: Option<&String>, label: &str| {
            if ui.selectable_label(shown.as_ref() == name, label).clicked()
                && shown.as_ref() != name
            {
                actions.requests.push(Request::ShowReferenceCurve {
                    meter: meter.meter,
                    name: name.cloned(),
                });
            }
        };
        pick(ui, None, "None");
        for name in &scene.reference_curves {
            pick(ui, Some(name), name);
        }
        if let Some(reference) = meter.reference.as_ref().filter(|r| r.missing) {
            ui.label(RichText::new(format!("{} (missing)", reference.name)).weak());
        }
    });
    // Save: the typed name is kept in egui's memory until it's saved.
    let id = egui::Id::new(("reference curve name", meter.meter));
    let mut name = ui
        .data_mut(|d| d.get_temp::<String>(id))
        .unwrap_or_default();
    ui.horizontal(|ui| {
        let field = ui.add_enabled(
            long_term,
            egui::TextEdit::singleline(&mut name)
                .hint_text("Name")
                .desired_width(140.0),
        );
        let save = ui
            .add_enabled(
                long_term && !name.trim().is_empty(),
                egui::Button::new("Save as Reference curve"),
            )
            .on_hover_text("Keep the long-term average as a curve to compare with")
            .on_disabled_hover_text("Turn on the long-term average, then name the curve");
        let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if long_term && (save.clicked() || entered) && !name.trim().is_empty() {
            actions.requests.push(Request::SaveReferenceCurve {
                meter: meter.meter,
                name: name.trim().to_owned(),
            });
            ui.data_mut(|d| d.remove::<String>(id));
            return;
        }
        ui.data_mut(|d| d.insert_temp(id, name.clone()));
    });
}

/// The 24 keys, C major to B minor.
fn all_keys() -> impl Iterator<Item = Key> {
    (0..24).map(|i| Key {
        tonic: i % 12,
        minor: i >= 12,
    })
}

/// The Tonal Balance Meter's basic settings.
fn tonal_balance_basic(ui: &mut egui::Ui, s: &mut TonalBalanceMeterSettings) -> bool {
    let mut changed = choice(
        ui,
        "View",
        &mut s.view,
        &[
            (TonalView::Auto, "Auto"),
            (TonalView::Fine, "Fine"),
            (TonalView::Broad, "Broad"),
        ],
    );
    changed |= ui
        .add(Toggle::new(&mut s.low_end_zoom, "Low end zoom"))
        .on_hover_text("Fine shows only 20–300 Hz, a grid line on every note")
        .changed();
    changed |= choice(
        ui,
        "Average",
        &mut s.averaging,
        &[
            (LongTerm::Seconds(3.0), "3 s"),
            (LongTerm::Seconds(10.0), "10 s"),
            (LongTerm::Seconds(30.0), "30 s"),
            (LongTerm::Whole, "Whole"),
        ],
    );
    ui.horizontal(|ui| {
        ui.label("Key");
        let shown = match s.key {
            KeyChoice::Auto => "Auto (detected)".to_owned(),
            KeyChoice::Fixed(key) => key.name(),
        };
        egui::ComboBox::from_id_salt("tonal balance key")
            .selected_text(shown)
            .show_ui(ui, |ui| {
                changed |= ui
                    .selectable_value(&mut s.key, KeyChoice::Auto, "Auto (detected)")
                    .changed();
                for key in all_keys() {
                    changed |= ui
                        .selectable_value(&mut s.key, KeyChoice::Fixed(key), key.name())
                        .changed();
                }
            });
    });
    changed |= ui
        .add(Toggle::new(
            &mut s.shift_to_key,
            "Shift Target to the mix's key",
        ))
        .on_hover_text("Move the Target's low end (below 300 Hz) to the mix's key")
        .changed();
    changed
}

fn tonal_balance_advanced(ui: &mut egui::Ui, s: &mut TonalBalanceMeterSettings) -> bool {
    let mut changed = choice(
        ui,
        "Teach with",
        &mut s.parts,
        &[(Parts::Loud, "Loud parts"), (Parts::Whole, "Whole song")],
    );
    changed |= ui
        .add(
            Slider::new(
                &mut s.mono_cutoff,
                limits_tonal::MONO_CUTOFF_RANGE.0..=limits_tonal::MONO_CUTOFF_RANGE.1,
            )
            .text("Low end mono below")
            .suffix(" Hz")
            .logarithmic(true),
        )
        .changed();
    changed
}

/// A Tonal Balance Meter's Target: which one, the key suggestion,
/// teaching a song by playing, its songs, Freeze and Reset.
fn target_items(ui: &mut egui::Ui, scene: &Scene, meter: &MeterScene, actions: &mut Actions) {
    let MeterSettings::TonalBalance(settings) = meter.settings else {
        return;
    };
    let Some(state) = &meter.tonal_balance else {
        return;
    };
    let index = meter.meter;
    let (learning, frozen, target) = (state.learning, state.frozen, state.target.as_ref());
    let chosen = target.map(|t| t.name.clone());
    ui.horizontal_wrapped(|ui| {
        ui.label("Target");
        let mut pick = |ui: &mut egui::Ui, name: Option<&String>, label: &str| {
            if ui
                .selectable_label(chosen.as_ref() == name, label)
                .clicked()
                && chosen.as_ref() != name
            {
                actions.requests.push(Request::ChooseTarget {
                    meter: index,
                    name: name.cloned(),
                });
            }
        };
        pick(ui, None, "None");
        for name in &scene.targets {
            pick(ui, Some(name), name);
        }
        if let Some(target) = target.filter(|t| t.missing) {
            ui.label(RichText::new(format!("{} (missing)", target.name)).weak());
        }
    });
    if let Some(target) = target
        && let (Some(mix), Some(home)) = (target.suggest_shift, target.home_key)
    {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "Your mix is in {}, this Target's songs in {}.",
                mix.name(),
                home.name()
            ));
            if ui.button(format!("Shift to {}", mix.name())).clicked() {
                actions.push(Event::SetMeter {
                    meter: index,
                    settings: MeterSettings::TonalBalance(TonalBalanceMeterSettings {
                        shift_to_key: true,
                        ..settings
                    }),
                });
            }
        });
    }
    ui.horizontal(|ui| {
        if ui
            .selectable_label(frozen, "Freeze")
            .on_hover_text("Keep the curve as it is now as a ghost, to compare before and after")
            .clicked()
        {
            actions.push(Event::FreezeTonalBalance { meter: index });
        }
        if ui
            .button("Reset average")
            .on_hover_text("Start the mix's average over from now")
            .clicked()
        {
            actions.push(Event::ResetLongTerm { meter: index });
        }
    });
    // Files: songs for the Target, or a bounce to compare.
    ui.horizontal(|ui| {
        if ui
            .button("Add files…")
            .on_hover_text(
                "Analyse songs (WAV, AIFF, FLAC, MP3, Ogg, AAC/M4A, ALAC) for the Target; \
                 files and folders can also be dropped on the Meter",
            )
            .clicked()
        {
            actions
                .requests
                .push(Request::AddTargetFiles { meter: index });
        }
        match &state.comparing {
            None => {
                if ui
                    .button("Compare file…")
                    .on_hover_text("Compare a bounce of the mix in place of the live Source")
                    .clicked()
                {
                    actions.requests.push(Request::CompareFile { meter: index });
                }
            }
            Some(name) => {
                if ui
                    .button("Stop comparing")
                    .on_hover_text(format!("Back to the live Source from {name}"))
                    .clicked()
                {
                    actions.push(Event::CompareFile {
                        meter: index,
                        file: None,
                    });
                }
            }
        }
    });
    if let Some(upload) = &state.upload {
        if upload.done < upload.total {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!(
                    "Analysing {} of {} files…",
                    upload.done + 1,
                    upload.total
                ));
            });
        }
        if !upload.failed.is_empty() {
            ui.label(RichText::new("Couldn't read:").weak());
            for (file, reason) in &upload.failed {
                ui.label(RichText::new(format!("{file}: {reason}")).small());
            }
            if upload.done >= upload.total && ui.small_button("OK").clicked() {
                actions
                    .requests
                    .push(Request::DismissUpload { meter: index });
            }
        }
    }
    // Teaching by playing: Learn, play the song, Add.
    match learning {
        None => {
            if ui
                .button("Learn a song…")
                .on_hover_text("Play a reference song from start to end, then Add it to a Target")
                .clicked()
            {
                actions.push(Event::LearnSong { meter: index });
            }
        }
        Some(s) => {
            ui.label(format!("Learning: {}:{:02} heard", s / 60, s % 60));
            let ids = (
                egui::Id::new(("learn target", index)),
                egui::Id::new(("learn song", index)),
            );
            let mut target = ui
                .data_mut(|d| d.get_temp::<String>(ids.0))
                .or_else(|| chosen.clone())
                .unwrap_or_default();
            let mut song = ui
                .data_mut(|d| d.get_temp::<String>(ids.1))
                .unwrap_or_default();
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut song)
                        .hint_text("Song")
                        .desired_width(110.0),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut target)
                        .hint_text("Target")
                        .desired_width(110.0),
                );
            });
            let mut done = false;
            ui.horizontal(|ui| {
                let can_add = !target.trim().is_empty() && s > 0;
                if ui
                    .add_enabled(can_add, egui::Button::new("Add"))
                    .on_disabled_hover_text("Name the Target and play something")
                    .clicked()
                {
                    actions.requests.push(Request::AddLearnedSong {
                        meter: index,
                        target: target.trim().to_owned(),
                        song: song.trim().to_owned(),
                    });
                    done = true;
                }
                if ui.button("Cancel").clicked() {
                    actions.push(Event::CancelLearning { meter: index });
                    done = true;
                }
            });
            ui.data_mut(|d| {
                if done {
                    d.remove::<String>(ids.0);
                    d.remove::<String>(ids.1);
                } else {
                    d.insert_temp(ids.0, target);
                    d.insert_temp(ids.1, song);
                }
            });
        }
    }
    let Some(target) = target.filter(|t| !t.songs.is_empty()) else {
        return;
    };
    ui.collapsing(format!("Songs ({})", target.songs.len()), |ui| {
        for (k, song) in target.songs.iter().enumerate() {
            ui.horizontal(|ui| {
                let s = song.seconds.round() as u64;
                let key = song
                    .key
                    .map(|k| format!(" · {}", k.name()))
                    .unwrap_or_default();
                ui.label(format!("{} ({}:{:02}{key})", song.name, s / 60, s % 60));
                if ui
                    .small_button("×")
                    .on_hover_text("Remove this song from the Target")
                    .clicked()
                {
                    actions.requests.push(Request::RemoveTargetSong {
                        target: target.name.clone(),
                        song: k,
                    });
                }
            });
        }
    });
}

/// The Loudness Meter's basic settings; `reset` is set when Reset is clicked.
fn loudness_basic(ui: &mut egui::Ui, s: &mut LoudnessMeterSettings, reset: &mut bool) -> bool {
    let mut on = s.target.is_some();
    let mut target = s.target.unwrap_or(-14.0);
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui.add(Toggle::new(&mut on, "Target")).changed();
        ui.add_enabled_ui(on, |ui| {
            changed |= ui
                .add(
                    Slider::new(
                        &mut target,
                        limits::LOUDNESS_TARGET.0..=limits::LOUDNESS_TARGET.1,
                    )
                    .suffix(" LUFS")
                    .step_by(0.5),
                )
                .changed();
        });
    });
    if changed {
        s.target = on.then_some(target);
    }
    changed |= ui
        .add(Toggle::new(&mut s.show_bars, "Show bars"))
        .on_hover_text("L/R and LUFS bars beside the numbers")
        .changed();
    if !s.show_bars {
        changed |= choice(
            ui,
            "Big reading",
            &mut s.big_reading,
            &[
                (BigReading::ShortTerm, "Short-term"),
                (BigReading::Momentary, "Momentary"),
                (BigReading::Integrated, "Integrated"),
                (BigReading::TruePeak, "True peak"),
            ],
        );
        changed |= ui
            .add(Toggle::new(&mut s.show_thin_bars, "Thin bars"))
            .on_hover_text("A thin bar beside each big number")
            .changed();
    } else {
        ui.horizontal(|ui| {
            ui.label("Scale");
            egui::ComboBox::from_id_salt("loudness scale")
                .selected_text(s.analysis.scale.name())
                .show_ui(ui, |ui| {
                    for scale in Scale::ALL {
                        changed |= ui
                            .selectable_value(&mut s.analysis.scale, scale, scale.name())
                            .changed();
                    }
                })
                .response
                .on_hover_text(
                    "The bars' scale and ballistics: sample peak and RMS, VU, \
                     a PPM (DIN, Nordic, BBC, EBU) or the K-System",
                );
        });
        if s.analysis.scale == Scale::Vu {
            changed |= ui
                .add(
                    Slider::new(
                        &mut s.analysis.vu_reference,
                        limits::VU_REFERENCE.0..=limits::VU_REFERENCE.1,
                    )
                    .text("0 VU at")
                    .suffix(" dBFS")
                    .step_by(1.0),
                )
                .on_hover_text("The peak of the sine that reads 0 VU")
                .changed();
        }
    }
    changed |= choice(
        ui,
        "Bar shows",
        &mut s.lufs_bar,
        &[
            (LufsBar::ShortTerm, "Short-term"),
            (LufsBar::Momentary, "Momentary"),
        ],
    );
    if ui.button("Reset I, LRA and maxima").clicked() {
        *reset = true;
    }
    changed
}

fn stereometer_basic(ui: &mut egui::Ui, s: &mut StereometerMeterSettings) -> bool {
    let mut changed = choice(
        ui,
        "View",
        &mut s.view,
        &[
            (StereoView::Polar, "Polar"),
            (StereoView::Lissajous, "Lissajous"),
        ],
    );
    changed |= ui
        .add(Toggle::new(&mut s.show_balance, "Balance bar"))
        .changed();
    changed
}

// ---- Advanced settings: the rest, in the settings panel. ----

fn waveform_advanced(ui: &mut egui::Ui, s: &mut WaveformMeterSettings) -> bool {
    let a = &mut s.analysis;
    let mut changed = choice(
        ui,
        "Scale",
        &mut a.scale,
        &[
            (WaveformScale::Linear, "Linear"),
            (WaveformScale::Decibels, "dB"),
        ],
    );
    let (low, high) = limits::WAVEFORM_GAIN;
    changed |= ui
        .add(
            Slider::new(&mut a.gain, low..=high)
                .text("Gain")
                .logarithmic(true),
        )
        .changed();
    let (low, high) = limits::WAVEFORM_LOW_CROSSOVER;
    changed |= ui
        .add(
            Slider::new(&mut a.low_crossover, low..=high)
                .text("Low/mid crossover")
                .suffix(" Hz")
                .logarithmic(true),
        )
        .changed();
    let (low, high) = limits::WAVEFORM_HIGH_CROSSOVER;
    changed |= ui
        .add(
            Slider::new(&mut a.high_crossover, low..=high)
                .text("Mid/high crossover")
                .suffix(" Hz")
                .logarithmic(true),
        )
        .changed();
    changed
}

fn spectrum_advanced(ui: &mut egui::Ui, s: &mut SpectrumMeterSettings) -> bool {
    let a = &mut s.analysis;
    let sizes: Vec<(usize, String)> = (MIN_FFT_SIZE.trailing_zeros()
        ..=MAX_FFT_SIZE.trailing_zeros())
        .map(|bits| (1 << bits, (1usize << bits).to_string()))
        .collect();
    let sizes: Vec<(usize, &str)> = sizes.iter().map(|(n, s)| (*n, s.as_str())).collect();
    let mut changed = choice(ui, "FFT size", &mut a.fft_size, &sizes);
    changed |= choice(
        ui,
        "Window",
        &mut a.window,
        &[
            (WindowFunction::Hann, "Hann"),
            (WindowFunction::BlackmanHarris, "Blackman-Harris"),
            (WindowFunction::Rectangular, "Rectangular"),
        ],
    );
    let (low, high) = limits::SPECTRUM_FREQUENCIES;
    let mut range = (
        f64::from(a.frequency_range.0),
        f64::from(a.frequency_range.1),
    );
    let mut freq = false;
    freq |= ui
        .add(
            Slider::new(&mut range.0, f64::from(low)..=f64::from(high))
                .text("Lowest frequency")
                .suffix(" Hz")
                .logarithmic(true),
        )
        .changed();
    freq |= ui
        .add(
            Slider::new(&mut range.1, f64::from(low)..=f64::from(high))
                .text("Highest frequency")
                .suffix(" Hz")
                .logarithmic(true),
        )
        .changed();
    if freq {
        a.frequency_range = (range.0 as f32, range.1 as f32);
        changed = true;
    }
    let mut db = (f64::from(a.db_range.0), f64::from(a.db_range.1));
    let (low, high) = limits::SPECTRUM_DB;
    if range_sliders(
        ui,
        ("Floor", "Top"),
        &mut db,
        (f64::from(low), f64::from(high)),
        " dB",
    ) {
        a.db_range = (db.0 as f32, db.1 as f32);
        changed = true;
    }
    changed |= duration_slider(
        ui,
        "Attack",
        &mut a.attack,
        limits::SPECTRUM_BALLISTICS,
        false,
    );
    changed |= duration_slider(
        ui,
        "Release",
        &mut a.release,
        limits::SPECTRUM_BALLISTICS,
        false,
    );
    let smoothing = [
        (0.0, "Off"),
        (1.0 / 24.0, "1/24 oct"),
        (1.0 / 12.0, "1/12"),
        (1.0 / 6.0, "1/6"),
        (1.0 / 3.0, "1/3"),
    ];
    changed |= choice(ui, "Smoothing", &mut a.smoothing_octaves, &smoothing);
    changed |= peak_hold(ui, &mut a.peak_hold);
    if let SpectrumStyle::Bars { bands_per_octave } = &mut a.style {
        let bands = limits::SPECTRUM_BANDS.map(|b| {
            (
                b,
                match b {
                    3 => "1/3 oct",
                    6 => "1/6",
                    _ => "1/12",
                },
            )
        });
        changed |= choice(ui, "Bar width", bands_per_octave, &bands);
    }
    changed
}

fn loudness_advanced(ui: &mut egui::Ui, s: &mut LoudnessMeterSettings) -> bool {
    let mut changed = duration_slider(
        ui,
        "RMS window",
        &mut s.analysis.rms_window,
        limits::RMS_WINDOW,
        false,
    );
    changed |= choice(
        ui,
        "RMS",
        &mut s.analysis.rms_mode,
        &[(RmsMode::Aes17, "AES17"), (RmsMode::Plain, "Plain")],
    );
    changed |= peak_hold(ui, &mut s.analysis.peak_hold);
    changed |= range_sliders(
        ui,
        ("Bar floor", "Bar top"),
        &mut s.bar_range,
        limits::LOUDNESS_BAR,
        " dB",
    );
    changed |= ui
        .add(Toggle::new(&mut s.show_true_peak, "Show true peak"))
        .changed();
    changed |= ui.add(Toggle::new(&mut s.show_range, "Show LRA")).changed();
    changed |= ui
        .add(Toggle::new(&mut s.show_counters, "Count clips and overs"))
        .on_hover_text(
            "Per channel: clips (samples at or over 0 dBFS) and true peaks over the ceiling, \
             each run counted once; reset with the other maxima",
        )
        .changed();
    if s.show_counters {
        changed |= ui
            .add(
                Slider::new(
                    &mut s.analysis.ceiling,
                    limits::TRUE_PEAK_CEILING.0..=limits::TRUE_PEAK_CEILING.1,
                )
                .text("True-peak ceiling")
                .suffix(" dBTP")
                .step_by(0.1),
            )
            .changed();
    }
    changed |= ui
        .add(Toggle::new(
            &mut s.show_peak_to_loudness,
            "Show PLR and PSR",
        ))
        .on_hover_text(
            "Peak to loudness: the true-peak maximum minus integrated loudness (PLR), \
             and the last 3 s' true peak minus short-term loudness (PSR)",
        )
        .changed();
    changed |= ui
        .add(Toggle::new(&mut s.show_history, "Loudness graph"))
        .on_hover_text("The LUFS bar's reading over time, where there's room")
        .changed();
    if s.show_history {
        let mut seconds = s.history_span.as_secs();
        let spans: Vec<(u64, String)> = limits::HISTORY_SPANS
            .iter()
            .map(|&span| (span, format!("{span} s")))
            .collect();
        let spans: Vec<(u64, &str)> = spans.iter().map(|(v, l)| (*v, l.as_str())).collect();
        if choice(ui, "Graph span", &mut seconds, &spans) {
            s.history_span = Duration::from_secs(seconds);
            changed = true;
        }
    }
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(MEASUREMENTS_NOTE).small());
        ui.hyperlink_to(RichText::new("What it measures").small(), MEASUREMENTS_URL);
    });
    changed
}

fn cepstrum_basic(ui: &mut egui::Ui, s: &mut CepstrumMeterSettings) -> bool {
    ui.add(Toggle::new(&mut s.show_pitch, "Show pitch"))
        .on_hover_text("Mark the period of the pitch found, with its frequency and note")
        .changed()
}

fn needle_basic(ui: &mut egui::Ui, s: &mut NeedleMeterSettings) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label("Scale");
        egui::ComboBox::from_id_salt("needle scale")
            .selected_text(s.scale.name())
            .show_ui(ui, |ui| {
                for scale in Scale::ALL.into_iter().filter(|s| s.ballistic()) {
                    changed |= ui
                        .selectable_value(&mut s.scale, scale, scale.name())
                        .changed();
                }
            });
    });
    if s.scale == Scale::Vu {
        changed |= ui
            .add(
                Slider::new(
                    &mut s.vu_reference,
                    limits::VU_REFERENCE.0..=limits::VU_REFERENCE.1,
                )
                .text("0 VU at")
                .suffix(" dBFS")
                .step_by(1.0),
            )
            .on_hover_text("The peak of the sine that reads 0 VU")
            .changed();
    }
    changed |= ui
        .add(Toggle::new(&mut s.mono, "One needle (mono)"))
        .on_hover_text("One needle on the mono sum instead of Left and Right")
        .changed();
    changed
}

fn stereo_bands_basic(ui: &mut egui::Ui, s: &mut StereoBandsMeterSettings) -> bool {
    let mut changed = ui
        .add(Toggle::new(&mut s.show_width, "Show width"))
        .on_hover_text(
            "Each band's width: 0 mono, half-way unrelated channels, full opposite polarity",
        )
        .changed();
    changed |= duration_slider(
        ui,
        "Averaging",
        &mut s.time,
        limits::CORRELATION_TIME,
        false,
    );
    changed
}

fn tuner_basic(ui: &mut egui::Ui, s: &mut TunerMeterSettings) -> bool {
    ui.add(
        Slider::new(&mut s.a4, limits::TUNER_A4.0..=limits::TUNER_A4.1)
            .text("A4")
            .suffix(" Hz")
            .step_by(0.1),
    )
    .on_hover_text("The reference pitch the notes are tuned to")
    .changed()
}

/// The Tuner's pitch finding, as the Cepstrum's.
fn tuner_advanced(ui: &mut egui::Ui, s: &mut TunerMeterSettings) -> bool {
    let mut cepstrum = CepstrumMeterSettings {
        analysis: s.analysis,
        show_pitch: true,
    };
    let changed = cepstrum_advanced(ui, &mut cepstrum);
    s.analysis = cepstrum.analysis;
    changed
}

fn oscilloscope_basic(ui: &mut egui::Ui, s: &mut OscilloscopeMeterSettings) -> bool {
    let a = &mut s.analysis;
    let mut changed = choice(ui, "Channels", &mut a.channels, &CHANNEL_VIEWS);
    changed |= duration_slider(
        ui,
        "Window",
        &mut a.window,
        limits::OSCILLOSCOPE_WINDOW,
        false,
    );
    changed |= ui
        .add(Toggle::new(&mut a.trigger, "Hold still"))
        .on_hover_text(
            "Start each picture on a rising zero crossing, so a steady wave stands still",
        )
        .changed();
    changed |= ui
        .add(Toggle::new(&mut a.auto_gain, "Auto gain"))
        .on_hover_text(
            "Raise quiet audio to fill the height (up to +48 dB); falls at once, rises slowly",
        )
        .changed();
    changed
}

fn cepstrum_advanced(ui: &mut egui::Ui, s: &mut CepstrumMeterSettings) -> bool {
    let a = &mut s.analysis;
    let sizes: Vec<(usize, String)> = limits::CEPSTRUM_FFT_SIZES
        .iter()
        .map(|&n| (n, n.to_string()))
        .collect();
    let sizes: Vec<(usize, &str)> = sizes.iter().map(|(n, s)| (*n, s.as_str())).collect();
    let mut changed = choice(ui, "FFT size", &mut a.fft_size, &sizes);
    let (low, high) = limits::CEPSTRUM_PITCH;
    let (mut from, mut to) = a.pitch_range;
    let mut range = false;
    range |= ui
        .add(
            Slider::new(&mut from, low..=high / 2.0)
                .logarithmic(true)
                .text("Lowest pitch")
                .suffix(" Hz"),
        )
        .changed();
    range |= ui
        .add(
            Slider::new(&mut to, low * 2.0..=high)
                .logarithmic(true)
                .text("Highest pitch")
                .suffix(" Hz"),
        )
        .changed();
    if range {
        a.pitch_range = (from.round(), to.round());
        changed = true;
    }
    changed |= duration_slider(
        ui,
        "Smoothing",
        &mut a.smoothing,
        limits::CEPSTRUM_SMOOTHING,
        false,
    );
    changed
}

fn spectrogram_basic(ui: &mut egui::Ui, s: &mut SpectrogramMeterSettings) -> bool {
    let mut seconds = s.analysis.span.as_secs();
    let spans: Vec<(u64, String)> = limits::SPECTROGRAM_SPANS
        .iter()
        .map(|&v| (v, format!("{v} s")))
        .collect();
    let spans: Vec<(u64, &str)> = spans.iter().map(|(v, l)| (*v, l.as_str())).collect();
    let mut changed = choice(ui, "Span", &mut seconds, &spans);
    if changed {
        s.analysis.span = Duration::from_secs(seconds);
    }
    changed |= ui
        .add(Toggle::new(&mut s.show_scale, "Frequency scale"))
        .changed();
    changed
}

fn spectrogram_advanced(ui: &mut egui::Ui, s: &mut SpectrogramMeterSettings) -> bool {
    let a = &mut s.analysis;
    let sizes: Vec<(usize, String)> = limits::SPECTROGRAM_FFT_SIZES
        .iter()
        .map(|&n| (n, n.to_string()))
        .collect();
    let sizes: Vec<(usize, &str)> = sizes.iter().map(|(n, s)| (*n, s.as_str())).collect();
    let mut changed = choice(ui, "FFT size", &mut a.fft_size, &sizes);
    changed |= choice(
        ui,
        "Window",
        &mut a.window,
        &[
            (WindowFunction::Hann, "Hann"),
            (WindowFunction::BlackmanHarris, "Blackman-Harris"),
            (WindowFunction::Rectangular, "Rectangular"),
        ],
    );
    let slopes: Vec<(f32, String)> = limits::SPECTRUM_SLOPES
        .iter()
        .map(|&v| (v, format!("{v} dB/oct")))
        .collect();
    let slopes: Vec<(f32, &str)> = slopes.iter().map(|(v, l)| (*v, l.as_str())).collect();
    changed |= choice(ui, "Slope", &mut a.slope, &slopes);
    let (low, high) = limits::SPECTRUM_FREQUENCIES;
    let mut range = (
        f64::from(a.frequency_range.0),
        f64::from(a.frequency_range.1),
    );
    let mut freq = false;
    freq |= ui
        .add(
            Slider::new(&mut range.0, f64::from(low)..=f64::from(high))
                .text("Lowest frequency")
                .suffix(" Hz")
                .logarithmic(true),
        )
        .changed();
    freq |= ui
        .add(
            Slider::new(&mut range.1, f64::from(low)..=f64::from(high))
                .text("Highest frequency")
                .suffix(" Hz")
                .logarithmic(true),
        )
        .changed();
    if freq {
        a.frequency_range = (range.0 as f32, range.1 as f32);
        changed = true;
    }
    let mut db = (f64::from(a.db_range.0), f64::from(a.db_range.1));
    let (low, high) = limits::SPECTRUM_DB;
    if range_sliders(
        ui,
        ("Floor", "Top"),
        &mut db,
        (f64::from(low), f64::from(high)),
        " dB",
    ) {
        a.db_range = (db.0 as f32, db.1 as f32);
        changed = true;
    }
    changed
}

/// Every Phase Scope option, all in the right-click menu. `tap` is set when
/// the Tap button is pressed.
fn phase_scope_basic(ui: &mut egui::Ui, s: &mut PhaseScopeMeterSettings, tap: &mut bool) -> bool {
    let mut changed = choice(
        ui,
        "Cycle",
        &mut s.cycle,
        &[(CycleLength::Beat, "Beat"), (CycleLength::Bar, "Bar")],
    );
    let (low, high) = limits::PHASE_SCOPE_TEMPO;
    ui.horizontal(|ui| {
        changed |= ui
            .add(
                Slider::new(&mut s.tempo, low..=high)
                    .text("Tempo")
                    .suffix(" BPM")
                    .max_decimals(1),
            )
            .on_hover_text(
                "Used when no DAW says its tempo: System Capture, or an older Send Plugin",
            )
            .changed();
        *tap |= ui
            .button("Tap")
            .on_hover_text("Tap along: two or more taps set the tempo")
            .clicked();
    });
    changed |= choice(
        ui,
        "Look",
        &mut s.filled,
        &[(false, "Line"), (true, "Filled")],
    );
    changed |= choice(
        ui,
        "Channels",
        &mut s.channel_view,
        &[(ChannelView::Mono, "Mono"), (ChannelView::LeftRight, "L/R")],
    );
    changed |= choice(
        ui,
        "Gain",
        &mut s.auto_gain,
        &[(true, "Auto"), (false, "Manual")],
    );
    if !s.auto_gain {
        let (low, high) = limits::PHASE_SCOPE_GAIN;
        changed |= ui
            .add(
                Slider::new(&mut s.gain, low..=high)
                    .text("Gain")
                    .suffix(" dB"),
            )
            .changed();
    }
    changed |= choice(
        ui,
        "Steadiness",
        &mut s.steadiness,
        &[
            (Steadiness::Trail, "Fading trail"),
            (Steadiness::Average, "Average"),
        ],
    );
    changed
}

fn stereometer_advanced(ui: &mut egui::Ui, s: &mut StereometerMeterSettings) -> bool {
    let mut changed = duration_slider(
        ui,
        "Persistence",
        &mut s.persistence,
        limits::STEREO_PERSISTENCE,
        false,
    );
    changed |= choice(
        ui,
        "Draw as",
        &mut s.drawing,
        &[
            (StereoDrawing::Dots, "Dots"),
            (StereoDrawing::Lines, "Lines"),
        ],
    );
    let mut fixed = matches!(s.analysis.scaling, StereoScaling::Fixed { .. });
    if choice(ui, "Scale", &mut fixed, &[(false, "Auto"), (true, "Fixed")]) {
        s.analysis.scaling = if fixed {
            StereoScaling::Fixed { gain: 1.0 }
        } else {
            StereoScaling::Auto
        };
        changed = true;
    }
    if let StereoScaling::Fixed { gain } = &mut s.analysis.scaling {
        let (low, high) = limits::STEREO_GAIN;
        changed |= ui
            .add(Slider::new(gain, low..=high).text("Gain").logarithmic(true))
            .changed();
    }
    changed |= duration_slider(
        ui,
        "Correlation averaging",
        &mut s.analysis.correlation_time,
        limits::CORRELATION_TIME,
        false,
    );
    changed |= ui
        .add(
            Slider::new(&mut s.correlation_threshold, -1.0..=1.0)
                .text("Correlation warning below")
                .step_by(0.05),
        )
        .changed();
    changed
}

/// A Meter's basic settings, sent as one event if any changed.
fn basic(ui: &mut egui::Ui, meter: usize, settings: MeterSettings, actions: &mut Actions) {
    let mut s = settings;
    let (mut reset, mut tap) = (false, false);
    let changed = match &mut s {
        MeterSettings::Waveform(w) => waveform_basic(ui, w),
        MeterSettings::Spectrum(w) => spectrum_basic(ui, w),
        MeterSettings::Loudness(w) => loudness_basic(ui, w, &mut reset),
        MeterSettings::Stereometer(w) => stereometer_basic(ui, w),
        MeterSettings::Cepstrum(w) => cepstrum_basic(ui, w),
        MeterSettings::Spectrogram(w) => spectrogram_basic(ui, w),
        MeterSettings::PhaseScope(w) => phase_scope_basic(ui, w, &mut tap),
        MeterSettings::Oscilloscope(w) => oscilloscope_basic(ui, w),
        MeterSettings::TonalBalance(w) => tonal_balance_basic(ui, w),
        MeterSettings::Tuner(w) => tuner_basic(ui, w),
        MeterSettings::StereoBands(w) => stereo_bands_basic(ui, w),
        MeterSettings::Needle(w) => needle_basic(ui, w),
    };
    if changed {
        actions.push(Event::SetMeter { meter, settings: s });
    }
    if reset {
        actions.push(Event::ResetLoudness { meter });
    }
    if tap {
        actions.push(Event::TapTempo { meter });
    }
}

fn advanced(ui: &mut egui::Ui, meter: usize, settings: MeterSettings, actions: &mut Actions) {
    let mut s = settings;
    let changed = match &mut s {
        MeterSettings::Waveform(w) => waveform_advanced(ui, w),
        MeterSettings::Spectrum(w) => spectrum_advanced(ui, w),
        MeterSettings::Loudness(w) => loudness_advanced(ui, w),
        MeterSettings::Stereometer(w) => stereometer_advanced(ui, w),
        MeterSettings::Cepstrum(w) => cepstrum_advanced(ui, w),
        MeterSettings::Spectrogram(w) => spectrogram_advanced(ui, w),
        MeterSettings::PhaseScope(_) => false, // all in the menu
        MeterSettings::Oscilloscope(_) => false, // all in the basics
        MeterSettings::TonalBalance(w) => tonal_balance_advanced(ui, w),
        MeterSettings::Tuner(w) => tuner_advanced(ui, w),
        MeterSettings::StereoBands(_) | MeterSettings::Needle(_) => false, // all in the basics
    };
    if changed {
        actions.push(Event::SetMeter { meter, settings: s });
    }
}

fn listen_to(ui: &mut egui::Ui, scene: &Scene, actions: &mut Actions) {
    let mut value = scene.listen_to;
    if choice(
        ui,
        "Listen to",
        &mut value,
        &[
            (ListenTo::SystemCapture, "System Capture"),
            (ListenTo::SendPlugins, "Send Plugins"),
        ],
    ) {
        actions.push(Event::SetListenTo(value));
    }
}

/// The Source item at the top of a Meter's menu: Listen to, which Send Plugin
/// the Meter shows, "Use for all Meters" and the Source label.
fn source_item(ui: &mut egui::Ui, scene: &Scene, meter: usize, actions: &mut Actions) {
    let Some(meter_scene) = meter_scene(scene, meter) else {
        return;
    };
    listen_to(ui, scene, actions);
    match scene.listen_to {
        ListenTo::SystemCapture => {
            ui.label(RichText::new("Source: System Capture").weak());
        }
        ListenTo::SendPlugins if scene.send_plugins.is_empty() => {
            ui.label(RichText::new("No Send Plugins yet. Add Das-Meter Send to a track").weak());
        }
        ListenTo::SendPlugins => {
            ui.label("Source");
            for plugin in &scene.send_plugins {
                let selected = meter_scene.picked == Some(plugin.id);
                ui.add_enabled_ui(plugin.pickable, |ui| {
                    ui.horizontal(|ui| {
                        let (dot, _) =
                            ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                        ui.painter()
                            .circle_filled(dot.center(), 4.0, colour32(plugin.colour));
                        if ui.radio(selected, &plugin.label).clicked() && !selected {
                            actions.push(Event::PickSendPlugin {
                                meter,
                                id: plugin.id,
                            });
                        }
                    });
                });
            }
            let can_share = meter_scene.picked.is_some() && all_meters(scene).len() > 1;
            if ui
                .add_enabled(can_share, egui::Button::new("Use for all Meters"))
                .clicked()
            {
                actions.push(Event::UseForAllMeters { meter });
            }
        }
    }
    let mut shown = meter_scene.show_source_label;
    if ui
        .add(Toggle::new(&mut shown, "Show Source label"))
        .changed()
    {
        actions.push(Event::ShowSourceLabel { meter, shown });
    }
}

/// A Phase Scope's Overlay Source: a second Send Plugin from the same DAW,
/// or none, and the ± ms offset that nudges it. Shown disabled on System
/// Capture, which has only the one Source.
fn overlay_item(ui: &mut egui::Ui, scene: &Scene, meter_scene: &MeterScene, actions: &mut Actions) {
    let Some(overlay) = &meter_scene.overlay else {
        return;
    };
    let meter = meter_scene.meter;
    ui.separator();
    ui.label("Overlay Source");
    if scene.listen_to == ListenTo::SystemCapture {
        ui.add_enabled(
            false,
            egui::Label::new("Needs Send Plugins: listen to Send Plugins"),
        );
        return;
    }
    if ui.radio(overlay.picked.is_none(), "None").clicked() && overlay.picked.is_some() {
        actions.push(Event::PickOverlay { meter, id: None });
    }
    let mut listed = false;
    for plugin in &overlay.choices {
        listed |= overlay.picked == Some(plugin.id);
        let selected = overlay.picked == Some(plugin.id);
        ui.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(dot.center(), 4.0, colour32(plugin.colour));
            if ui.radio(selected, &plugin.label).clicked() && !selected {
                actions.push(Event::PickOverlay {
                    meter,
                    id: Some(plugin.id),
                });
            }
        });
    }
    if let (Some(_), false) = (overlay.picked, listed) {
        ui.label(RichText::new("The picked one isn't there: waiting for it").weak());
    }
    if overlay.choices.is_empty() && overlay.picked.is_none() {
        ui.label(RichText::new("No other Send Plugin in this DAW").weak());
    }
    if overlay.picked.is_none() {
        return;
    }
    // A Spectrum: where the two collide.
    if let MeterSettings::Spectrum(mut settings) = meter_scene.settings {
        let mut changed = ui
            .add(
                Slider::new(
                    &mut settings.collision_floor,
                    limits::COLLISION_FLOOR.0..=limits::COLLISION_FLOOR.1,
                )
                .text("Collisions above")
                .suffix(" dB")
                .step_by(1.0),
            )
            .on_hover_text("Both must be louder than this, on the Spectrum's scale")
            .changed();
        changed |= ui
            .add(
                Slider::new(
                    &mut settings.collision_within,
                    limits::COLLISION_WITHIN.0..=limits::COLLISION_WITHIN.1,
                )
                .text("…and within")
                .suffix(" dB")
                .step_by(1.0),
            )
            .on_hover_text("of each other: then they fight for the same frequencies")
            .changed();
        if changed {
            actions.push(Event::SetMeter {
                meter,
                settings: MeterSettings::Spectrum(settings),
            });
        }
        return;
    }
    if let MeterSettings::PhaseScope(mut settings) = meter_scene.settings {
        let (low, high) = limits::PHASE_SCOPE_OFFSET;
        let mut changed = ui
            .add(
                Slider::new(&mut settings.overlay_offset, low..=high)
                    .text("Offset")
                    .suffix(" ms")
                    .max_decimals(1),
            )
            .on_hover_text("Nudges the overlay later (+) or earlier (−)")
            .changed();
        let (low, high) = limits::PHASE_SCOPE_CUTOFF;
        changed |= ui
            .add(
                Slider::new(&mut settings.cutoff, low..=high)
                    .text("Cycle correlation below")
                    .suffix(" Hz")
                    .logarithmic(true)
                    .max_decimals(0),
            )
            .on_hover_text("Cycle correlation: how well the two agree below this frequency over one Cycle, −1 to +1")
            .changed();
        changed |= ui
            .add(Toggle::new(&mut settings.show_sum, "Show sum"))
            .on_hover_text("The two added together: what's left in the mix")
            .changed();
        changed |= ui
            .add_enabled(
                settings.show_sum,
                Toggle::new(&mut settings.split_sum, "Sum in its own row"),
            )
            .on_hover_text("The two on top, their sum under them")
            .changed();
        changed |= ui
            .add(Toggle::new(&mut settings.suggestions, "Suggestions"))
            .on_hover_text("What would make them fit: flip, move or pitch one")
            .changed();
        if changed {
            actions.push(Event::SetMeter {
                meter,
                settings: MeterSettings::PhaseScope(settings),
            });
        }
    }
}

/// "Add Meter": a new Meter of the picked kind, on the side of this one
/// nearest where the menu was opened, which the item names.
fn add_meter_item(ui: &mut egui::Ui, scene: &Scene, meter: usize, actions: &mut Actions) {
    let label = match add_side(scene, meter) {
        Some(side) => format!("Add Meter {side}"),
        None => "Add Meter".to_owned(),
    };
    ui.menu_button(label, |ui| {
        for kind in MeterKind::ALL {
            if ui.button(kind_label(kind)).clicked() {
                actions.push(Event::AddMeter { meter, kind });
                ui.close();
            }
        }
    })
    .response
    .on_hover_text("Right-click nearer another edge of the Meter to add it there");
}

/// Where Add Meter puts the new Meter, as the core decides it: by the edge
/// nearest the right-click in a pane, by the half along the Bar.
fn add_side(scene: &Scene, meter: usize) -> Option<&'static str> {
    let menu = scene.menu.filter(|m| m.meter == meter)?;
    let [u, v] = scene
        .windows
        .iter()
        .find(|w| w.key == menu.window)?
        .meters
        .iter()
        .find(|m| m.meter == meter)?
        .frame
        .locate(menu.at)?;
    if scene.mode == LayoutMode::Bar {
        let edge = scene
            .windows
            .iter()
            .find(|w| w.key == WindowKey::Bar)
            .and_then(|w| w.edge);
        let along_x = matches!(edge, Some(Edge::Top | Edge::Bottom) | None);
        let before = if along_x { u < 0.5 } else { v < 0.5 };
        return Some(match (along_x, before) {
            (true, true) => "on the left",
            (true, false) => "on the right",
            (false, true) => "above",
            (false, false) => "below",
        });
    }
    let edges = [
        (u, "on the left"),
        (1.0 - u, "on the right"),
        (v, "above"),
        (1.0 - v, "below"),
    ];
    edges
        .into_iter()
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, side)| side)
}

fn kind_label(kind: MeterKind) -> &'static str {
    match kind {
        MeterKind::Waveform => "Waveform",
        MeterKind::Spectrum => "Spectrum",
        MeterKind::Loudness => "Loudness",
        MeterKind::Stereometer => "Stereo",
        MeterKind::Cepstrum => "Cepstrum",
        MeterKind::Spectrogram => "Spectrogram",
        MeterKind::PhaseScope => "Phase Scope",
        MeterKind::Oscilloscope => "Oscilloscope",
        MeterKind::TonalBalance => "Tonal Balance",
        MeterKind::Tuner => "Tuner",
        MeterKind::StereoBands => "Stereo Bands",
        MeterKind::Needle => "Needle Meter",
    }
}

/// "Change Meter": what this Meter shows; switching gives it that kind's
/// defaults.
fn kind_item(ui: &mut egui::Ui, scene: &Scene, meter: usize, actions: &mut Actions) {
    let Some(meter_scene) = meter_scene(scene, meter) else {
        return;
    };
    let current = meter_scene.settings.kind();
    ui.menu_button(format!("Change Meter ({})", kind_label(current)), |ui| {
        for kind in MeterKind::ALL {
            if ui
                .add(egui::Button::selectable(kind == current, kind_label(kind)))
                .clicked()
            {
                if kind != current {
                    actions.push(Event::AssignMeter { meter, kind });
                }
                ui.close();
            }
        }
    });
}

/// A pane's items in Window mode: which Meter it shows, split and close.
fn pane_item(ui: &mut egui::Ui, scene: &Scene, meter: usize, actions: &mut Actions) {
    kind_item(ui, scene, meter, actions);
    ui.horizontal(|ui| {
        if ui.button("Split side by side").clicked() {
            actions.push(Event::SplitPane {
                meter,
                direction: Direction::SideBySide,
            });
        }
        if ui.button("Split stacked").clicked() {
            actions.push(Event::SplitPane {
                meter,
                direction: Direction::Stacked,
            });
        }
    });
    add_meter_item(ui, scene, meter, actions);
    let panes = scene.windows.first().map_or(0, |w| w.meters.len());
    if ui
        .add_enabled(panes > 1, egui::Button::new("Remove Meter"))
        .clicked()
    {
        actions.push(Event::ClosePane { meter });
    }
}

/// Where a Meter sits: Pop out or Dock back, and a Pop-out's Always on top.
fn placement_item(ui: &mut egui::Ui, scene: &Scene, meter: usize, actions: &mut Actions) {
    if scene.mode == LayoutMode::Window {
        pane_item(ui, scene, meter, actions);
        return;
    }
    kind_item(ui, scene, meter, actions);
    let pop_out = scene
        .windows
        .iter()
        .find(|w| w.key == WindowKey::PopOut(meter));
    match pop_out {
        Some(window) => {
            let mut on_top = window.on_top;
            if ui.add(Toggle::new(&mut on_top, "Always on top")).changed() {
                actions.push(Event::SetPopOutOnTop { meter, on_top });
            }
            if ui.button("Dock back").clicked() {
                actions.push(Event::DockBack { meter });
            }
        }
        None => {
            let in_bar = scene
                .windows
                .iter()
                .find(|w| w.key == WindowKey::Bar)
                .map_or(0, |w| w.meters.len());
            add_meter_item(ui, scene, meter, actions);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(in_bar > 1, egui::Button::new("Pop out"))
                    .clicked()
                {
                    actions.push(Event::PopOut { meter });
                }
                if ui
                    .add_enabled(in_bar > 1, egui::Button::new("Remove from Bar"))
                    .clicked()
                {
                    actions.push(Event::RemoveFromBar { meter });
                }
            });
        }
    }
}

/// The quiet note about displays that aren't connected, with Keep here.
fn missing_displays_note(ui: &mut egui::Ui, scene: &Scene, actions: &mut Actions) {
    if scene.missing_displays.is_empty() {
        return;
    }
    let names = scene.missing_displays.join(", ");
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(format!(
                "{names} isn't connected: its windows are here for now."
            ))
            .weak()
            .small(),
        );
        if ui
            .small_button("Keep here")
            .on_hover_text("Make where the windows are now their place")
            .clicked()
        {
            actions.push(Event::KeepHere);
        }
    });
    ui.separator();
}

/// A Meter's menu: its Source item, basic settings, where it sits, Settings….
fn menu_contents(ui: &mut egui::Ui, scene: &Scene, meter: usize, actions: &mut Actions) {
    let Some(meter_scene) = meter_scene(scene, meter) else {
        return;
    };
    missing_displays_note(ui, scene, actions);
    ui.set_width(280.0);
    ui.label(RichText::new(kind_name(&meter_scene.settings)).strong());
    source_item(ui, scene, meter, actions);
    overlay_item(ui, scene, meter_scene, actions);
    ui.separator();
    basic(ui, meter, meter_scene.settings, actions);
    reference_items(ui, scene, meter_scene, actions);
    target_items(ui, scene, meter_scene, actions);
    ui.separator();
    placement_item(ui, scene, meter, actions);
    if ui.button("Settings…").clicked() {
        actions.push(Event::ShowSettings(true));
    }
}

/// The open Meter menu in its own window. Returns the content's size.
fn menu_window(ctx: &egui::Context, scene: &Scene, actions: &mut Actions) -> Option<egui::Vec2> {
    let menu = scene.menu?;
    let frame = egui::Frame::menu(&ctx.global_style());
    // Measured whole, even while the window is still smaller than the menu.
    let response = egui::Area::new(egui::Id::new("meter menu"))
        .fixed_pos(egui::Pos2::ZERO)
        .constrain(false)
        .show(ctx, |ui| {
            frame.show(ui, |ui| menu_contents(ui, scene, menu.meter, actions));
        });
    Some(response.response.rect.size())
}

/// The card's width, in logical pixels.
const CARD_WIDTH: f32 = 340.0;

/// The first-launch card, filling its own window, measured for its size.
fn card_window(ctx: &egui::Context, scene: &Scene, actions: &mut Actions) -> Option<egui::Vec2> {
    let card = scene.card.as_ref()?;
    let frame = egui::Frame::menu(&ctx.global_style()).inner_margin(egui::Margin::same(14));
    let response = egui::Area::new(egui::Id::new("card"))
        .fixed_pos(egui::Pos2::ZERO)
        .constrain(false)
        .show(ctx, |ui| {
            frame.show(ui, |ui| {
                ui.set_width(CARD_WIDTH);
                card_contents(ui, card, actions);
            });
        });
    Some(response.response.rect.size())
}

/// The card over the Meters, for snapshots.
fn card_overlay(ctx: &egui::Context, scene: &Scene, actions: &mut Actions) {
    let Some(card) = &scene.card else { return };
    egui::Area::new(egui::Id::new("card"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(16.0, 16.0))
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style())
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(CARD_WIDTH);
                    card_contents(ui, card, actions);
                });
        });
}

/// The ✕ in a card's top-right corner.
fn close_button(ui: &mut egui::Ui) -> bool {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
        // "×": Geist has no "✕".
        ui.add(egui::Button::new(RichText::new("×").size(18.0)).frame(false))
            .on_hover_text("Close")
            .clicked()
    })
    .inner
}

fn card_contents(ui: &mut egui::Ui, card: &Card, actions: &mut Actions) {
    let macos = cfg!(target_os = "macos");
    ui.spacing_mut().item_spacing.y = 6.0;
    match card {
        Card::Welcome => {
            ui.horizontal(|ui| {
                ui.heading("Welcome to Das-Meter");
                if close_button(ui) {
                    actions.push(Event::CloseWelcome);
                }
            });
            ui.label("Meters show what your computer plays");
            ui.label("Using a DAW? Add the Send Plugin");
            ui.label("Right-click any Meter for settings");
            if macos {
                ui.label(
                    egui::RichText::new(
                        "macOS will ask for permission; nothing is recorded or saved",
                    )
                    .weak(),
                );
            } else if cfg!(windows) {
                ui.label(
                    egui::RichText::new(
                        "Send Plugins are installed: add Das-Meter Send to a track",
                    )
                    .weak(),
                );
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let accent = ui.visuals().selection.stroke.color;
                let start = egui::Button::new(RichText::new("Start listening").strong())
                    .fill(accent.gamma_multiply(0.8));
                if ui.add(start).clicked() {
                    actions.push(Event::StartListening);
                }
                if macos && ui.button("Install Send Plugin…").clicked() {
                    actions.requests.push(Request::InstallSendPlugin);
                }
                if ui.link("Learn more").clicked() {
                    actions.requests.push(Request::Learn(Topic::GettingStarted));
                }
            });
        }
        Card::SilenceHint => {
            ui.horizontal(|ui| {
                ui.strong("Hearing nothing?");
                if close_button(ui) {
                    actions.push(Event::CloseCard);
                }
            });
            ui.label("Play some audio. If your DAW uses ASIO, switch to Send Plugins.");
            ui.horizontal(|ui| {
                if ui.button("Switch to Send Plugins").clicked() {
                    actions.push(Event::SetListenTo(ListenTo::SendPlugins));
                }
                if macos && ui.button("Privacy Settings…").clicked() {
                    actions.requests.push(Request::OpenPrivacySettings);
                }
                if ui.link("Learn more").clicked() {
                    actions.requests.push(Request::Learn(Topic::Asio));
                }
            });
        }
        Card::SendPluginFound { name } => {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(name).italics().strong());
                ui.label("is sending.");
                if close_button(ui) {
                    actions.push(Event::CloseCard);
                }
            });
            ui.label("Listen to Send Plugins?");
            ui.horizontal(|ui| {
                if ui.button("Switch").clicked() {
                    actions.push(Event::SetListenTo(ListenTo::SendPlugins));
                }
                if ui.link("Learn more").clicked() {
                    actions.requests.push(Request::Learn(Topic::SendPlugins));
                }
            });
        }
    }
}

/// The open Meter menu over the Meters, where it was right-clicked (snapshots).
fn meter_menu(ctx: &egui::Context, scene: &Scene, actions: &mut Actions) {
    let Some(menu) = scene.menu else { return };
    let screen = ctx.content_rect();
    let at = egui::pos2(
        screen.min.x + menu.at[0] * screen.width(),
        screen.min.y + menu.at[1] * screen.height(),
    );
    egui::Area::new(egui::Id::new("meter menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                menu_contents(ui, scene, menu.meter, actions);
            });
        });
}

/// The Bar's settings: edge, thickness, screen button, fullscreen apps.
fn bar_settings(ui: &mut egui::Ui, scene: &Scene, actions: &mut Actions) {
    let in_bar = scene.mode == LayoutMode::Bar;
    if !in_bar {
        ui.label(RichText::new("These apply in Bar mode.").weak());
        if ui.button("Switch to Bar mode").clicked() {
            actions.push(Event::SetMode(LayoutMode::Bar));
        }
        ui.add_space(6.0);
    }
    let bar = scene.windows.iter().find(|w| w.key == WindowKey::Bar);
    let settings = scene.bar;
    ui.add_enabled_ui(in_bar, |ui| {
        let mut edge = settings.edge;
        let edges = [
            (Edge::Top, "Top"),
            (Edge::Bottom, "Bottom"),
            (Edge::Left, "Left"),
            (Edge::Right, "Right"),
        ];
        if choice(ui, "Edge", &mut edge, &edges) {
            actions.push(Event::SetEdge(edge));
        }
        // What the Bar is drawn at (capped to the display), else as set.
        let mut thickness = match bar.and_then(|b| b.frame) {
            Some(frame) if settings.edge.horizontal() => frame.height,
            Some(frame) => frame.width,
            None => settings.thickness,
        };
        if ui
            .add(
                Slider::new(&mut thickness, dasmeter_core::layout::MIN_THICKNESS..=600.0)
                    .text("Thickness")
                    .suffix(" px"),
            )
            .changed()
        {
            actions.push(Event::SetBarThickness(thickness));
        }
        let mode = settings.screen;
        ui.horizontal(|ui| {
            ui.label("Screen");
            let mut hover = format!("Click for {}.", mode.next(Platform::current()).label());
            if Platform::current() == Platform::MacOs && mode != ScreenMode::ReserveSpace {
                hover = format!("{hover}\n{NO_RESERVE_SPACE_ON_MACOS}");
            }
            if ui.button(mode.label()).on_hover_text(hover).clicked() {
                actions.push(Event::CycleScreenMode);
            }
        });
        let mut over = settings.over_fullscreen;
        if ui
            .add(Toggle::new(&mut over, "Show over fullscreen apps"))
            .changed()
        {
            actions.push(Event::ShowOverFullscreen(over));
        }
    });
}

/// A colour button for `colour`; returns the new colour when it's changed.
fn colour_button(ui: &mut egui::Ui, colour: Colour) -> Option<Colour> {
    let mut edited = colour32(colour);
    let changed = egui::color_picker::color_edit_button_srgba(
        ui,
        &mut edited,
        egui::color_picker::Alpha::OnlyBlend,
    )
    .changed();
    changed.then(|| {
        let [r, g, b, a] = edited.to_srgba_unmultiplied();
        Colour { r, g, b, a }
    })
}

/// A combo box over the Theme list. Returns the index picked.
fn theme_combo(
    ui: &mut egui::Ui,
    id: &str,
    scene: &Scene,
    selected: Option<usize>,
) -> Option<usize> {
    let themes = &scene.theme.themes;
    let label = |i: usize| {
        let theme = &themes[i];
        if theme.built_in {
            format!("{} (built-in)", theme.name)
        } else {
            theme.name.clone()
        }
    };
    let shown = selected.map_or_else(|| scene.theme.name.clone(), label);
    let mut picked = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(shown)
        .show_ui(ui, |ui| {
            for i in 0..themes.len() {
                if ui.selectable_label(selected == Some(i), label(i)).clicked() {
                    picked = Some(i);
                }
            }
        });
    picked
}

/// Which Theme is used, Duplicate, and the editor for the folder's Themes.
fn theme_settings(ui: &mut egui::Ui, scene: &Scene, actions: &mut Actions) {
    let theme = &scene.theme;
    ui.label(format!("In use: {}", theme.name));
    let mut follow = theme.follows_system();
    if ui
        .add(Toggle::new(
            &mut follow,
            "Follow the system's light / dark setting",
        ))
        .changed()
    {
        // Following: Light and Dark; not: the Theme in use for both.
        let (light, dark) = if follow {
            let named = |name: &str| theme.themes.iter().position(|t| t.name == name);
            (
                named(dasmeter_core::theme::LIGHT).unwrap_or(1),
                named(dasmeter_core::theme::NOCTURNE).unwrap_or(0),
            )
        } else {
            let current = theme.current.unwrap_or(0);
            (current, current)
        };
        actions.push(Event::ChooseTheme { light, dark });
    }
    if follow {
        ui.horizontal(|ui| {
            ui.label("Light");
            if let Some(light) = theme_combo(ui, "light theme", scene, theme.light) {
                let dark = theme.dark.unwrap_or(0);
                actions.push(Event::ChooseTheme { light, dark });
            }
        });
        ui.horizontal(|ui| {
            ui.label("Dark");
            if let Some(dark) = theme_combo(ui, "dark theme", scene, theme.dark) {
                let light = theme.light.unwrap_or(1);
                actions.push(Event::ChooseTheme { light, dark });
            }
        });
    } else {
        ui.horizontal(|ui| {
            ui.label("Theme");
            if let Some(pick) = theme_combo(ui, "theme", scene, theme.current) {
                actions.push(Event::ChooseTheme {
                    light: pick,
                    dark: pick,
                });
            }
        });
    }
    let current = theme.current.unwrap_or(0);
    if ui
        .button("Duplicate")
        .on_hover_text("Copy the Theme in use into one you can edit")
        .clicked()
    {
        actions.push(Event::DuplicateTheme { theme: current });
    }
    let editable = theme.current.is_some_and(|i| !theme.themes[i].built_in);
    if editable {
        rename_theme(ui, actions, theme, current);
    } else {
        ui.label(
            RichText::new("Changing a built-in Theme saves your changes as a copy of it.").weak(),
        );
    }
    theme_styling(ui, scene, actions, current);
}

/// The name field for one of the folder's Themes.
fn rename_theme(
    ui: &mut egui::Ui,
    actions: &mut Actions,
    theme: &dasmeter_core::ThemeScene,
    current: usize,
) {
    // Rename: the typed name is kept in egui's memory until it's applied.
    let id = egui::Id::new(("theme name", current));
    let mut name = ui
        .data_mut(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| theme.name.clone());
    ui.horizontal(|ui| {
        ui.label("Name");
        let field = ui.add(egui::TextEdit::singleline(&mut name).desired_width(180.0));
        let rename = ui.add_enabled(
            !name.trim().is_empty() && name.trim() != theme.name,
            egui::Button::new("Rename"),
        );
        let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (rename.clicked() || entered) && !name.trim().is_empty() && name.trim() != theme.name {
            actions.requests.push(Request::RenameTheme {
                index: current,
                name: name.trim().to_owned(),
            });
            // Show the name the core gives it (" (2)" if taken) once applied.
            ui.data_mut(|d| d.remove::<String>(id));
            return;
        }
        ui.data_mut(|d| d.insert_temp(id, name.clone()));
    });
}

/// The Theme's styling and colours. Edits to a built-in go to a copy.
fn theme_styling(ui: &mut egui::Ui, scene: &Scene, actions: &mut Actions, current: usize) {
    let mut styling = scene.theme.styling;
    widgets::heading(ui, "Look");
    let mut changed = choice(
        ui,
        "Look",
        &mut styling.look,
        &[(Look::Smooth, "Smooth"), (Look::Classic, "Classic")],
    );
    if styling.look == Look::Smooth {
        let percent =
            |ui: &mut egui::Ui,
             value: &mut f32,
             range: std::ops::RangeInclusive<f32>,
             text: &str| { ui.add(Slider::new(value, range).text(text)).changed() };
        changed |= percent(ui, &mut styling.glow, 0.0..=1.0, "Glow");
        let (lo, hi) = dasmeter_core::theme::GRID_FADE_RANGE;
        changed |= percent(ui, &mut styling.grid_fade, lo..=hi, "Grid fade");
        changed |= percent(ui, &mut styling.gradient, 0.0..=1.0, "Gradient");
        changed |= percent(ui, &mut styling.vignette, 0.0..=1.0, "Vignette");
        changed |= ui
            .add(Toggle::new(&mut styling.dim_when_silent, "Dim when silent"))
            .changed();
    }
    widgets::heading(ui, "Layout");
    changed |= ui
        .add(Slider::new(&mut styling.background_opacity, 0.0..=1.0).text("Background opacity"))
        .changed();
    changed |= choice(
        ui,
        "Lines",
        &mut styling.line,
        &[
            (LineWeight::Thin, "Thin"),
            (LineWeight::Normal, "Normal"),
            (LineWeight::Thick, "Thick"),
        ],
    );
    let (lo, hi) = dasmeter_core::theme::GAP_RANGE;
    changed |= ui
        .add(
            Slider::new(&mut styling.gap, lo..=hi)
                .text("Gap")
                .suffix(" px"),
        )
        .changed();
    let (lo, hi) = dasmeter_core::theme::CORNER_RANGE;
    changed |= ui
        .add(
            Slider::new(&mut styling.corner_radius, lo..=hi)
                .text("Corner radius")
                .suffix(" px"),
        )
        .changed();
    let (lo, hi) = dasmeter_core::theme::TEXT_SCALE_RANGE;
    changed |= ui
        .add(Slider::new(&mut styling.text_scale, lo..=hi).text("Text size"))
        .changed();
    changed |= ui
        .add(Toggle::new(
            &mut styling.shape_cues,
            "Shape cues besides colour",
        ))
        .changed();
    if changed {
        actions.push(Event::SetThemeStyling {
            theme: current,
            styling,
        });
    }
    // Colours, grouped by Meter, under a small live preview; the Meters
    // themselves change at once too.
    widgets::heading(ui, "Colours");
    theme_preview(ui, &scene.palette, scene.theme.styling);
    for (group, roles) in COLOUR_GROUPS {
        widgets::heading(ui, group);
        egui::Grid::new(("theme colours", group))
            .num_columns(2)
            .min_col_width(150.0)
            .show(ui, |ui| {
                for &role in roles {
                    ui.label(role.label());
                    if let Some(colour) = colour_button(ui, scene.palette[role]) {
                        actions.push(Event::SetThemeColour {
                            theme: current,
                            role,
                            colour,
                        });
                    }
                    ui.end_row();
                }
            });
    }
}

/// The Theme's colours by the Meter that uses them.
const COLOUR_GROUPS: [(&str, &[Role]); 10] = [
    (
        "General",
        &[
            Role::Background,
            Role::Panel,
            Role::Grid,
            Role::Text,
            Role::Accent,
        ],
    ),
    (
        "Waveform",
        &[Role::WaveformLow, Role::WaveformMid, Role::WaveformHigh],
    ),
    (
        "Spectrum",
        &[
            Role::SpectrumLine,
            Role::SpectrumFill,
            Role::SpectrumPeakHold,
            Role::SpectrumPeakDots,
            Role::SpectrumHarmonics,
            Role::SpectrumSecond,
            Role::SpectrumCollision,
        ],
    ),
    (
        "Loudness",
        &[
            Role::LoudnessBar,
            Role::LoudnessPeak,
            Role::LoudnessOverTarget,
            Role::LoudnessCounter,
        ],
    ),
    (
        "Stereometer",
        &[
            Role::StereometerTrace,
            Role::CorrelationPositive,
            Role::CorrelationNegative,
        ],
    ),
    (
        "Phase Scope",
        &[
            Role::PhaseScopeTrace,
            Role::PhaseScopeCancel,
            Role::PhaseScopeSum,
            Role::PhaseScopeGrid,
        ],
    ),
    ("Cepstrum", &[Role::CepstrumTrace]),
    (
        "Oscilloscope",
        &[Role::OscilloscopeTrace, Role::OscilloscopeSecond],
    ),
    ("Tonal Balance", TONAL_BALANCE_ROLES),
    ("Tuner", &[Role::TunerInTune, Role::TunerOff]),
];

const TONAL_BALANCE_ROLES: &[Role] = &[
    Role::TonalBalanceMix,
    Role::TonalBalanceRange,
    Role::TonalBalanceOver,
    Role::TonalBalanceUnder,
];

/// A small Meter drawn in the Theme's colours: a panel with a grid, a
/// spectrum curve with its fill, a loudness bar and a stereo trace.
fn theme_preview(ui: &mut egui::Ui, palette: &Palette, styling: dasmeter_core::Styling) {
    let c = |role| colour32(palette[role]);
    let width = ui.available_width().min(360.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 96.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let radius = styling.corner_radius.clamp(0.0, 12.0);
    painter.rect_filled(rect, 0.0, c(Role::Background));
    let panel = rect.shrink(6.0);
    painter.rect_filled(panel, radius, c(Role::Panel));
    let plot = panel.shrink(8.0);
    let smooth = styling.look == Look::Smooth;
    let grid = c(Role::Grid).gamma_multiply(if smooth { 0.5 } else { 1.0 });
    for i in 1..4 {
        let y = egui::lerp(plot.top()..=plot.bottom(), i as f32 / 4.0);
        painter.hline(plot.x_range(), y, egui::Stroke::new(1.0, grid));
    }
    // Spectrum: the left two thirds.
    let spectrum = egui::Rect::from_min_max(
        plot.min,
        egui::pos2(plot.left() + plot.width() * 0.62, plot.bottom()),
    );
    let curve: Vec<egui::Pos2> = (0..=48)
        .map(|i| {
            let t = i as f32 / 48.0;
            let wave = 0.55 + 0.25 * (t * 9.0).sin() * (1.0 - t) - 0.2 * t;
            egui::pos2(
                egui::lerp(spectrum.x_range(), t),
                egui::lerp(spectrum.y_range(), 1.0 - wave),
            )
        })
        .collect();
    // The fill as one mesh, so no seams show between its pieces.
    let mut mesh = egui::Mesh::default();
    let fill_colour = c(Role::SpectrumFill).gamma_multiply(0.5);
    for (i, point) in curve.iter().enumerate() {
        mesh.colored_vertex(*point, fill_colour);
        mesh.colored_vertex(egui::pos2(point.x, spectrum.bottom()), fill_colour);
        if i > 0 {
            let n = (i * 2) as u32;
            mesh.add_triangle(n - 2, n - 1, n);
            mesh.add_triangle(n - 1, n, n + 1);
        }
    }
    painter.add(mesh);
    painter.add(egui::Shape::line(
        curve,
        egui::Stroke::new(1.5, c(Role::SpectrumLine)),
    ));
    // Loudness: a bar, over the target at the top.
    let bar = egui::Rect::from_min_max(
        egui::pos2(
            plot.left() + plot.width() * 0.68,
            plot.top() + plot.height() * 0.3,
        ),
        egui::pos2(plot.left() + plot.width() * 0.68 + 8.0, plot.bottom()),
    );
    painter.rect_filled(bar, 2.0, c(Role::LoudnessBar));
    let over = egui::Rect::from_min_max(
        egui::pos2(bar.left(), plot.top() + plot.height() * 0.15),
        egui::pos2(bar.right(), bar.top()),
    );
    painter.rect_filled(over, 2.0, c(Role::LoudnessOverTarget));
    // Stereo: a few dots in a cloud at the right.
    let centre = egui::pos2(plot.left() + plot.width() * 0.86, plot.center().y);
    for i in 0..40 {
        let a = i as f32 * 2.399;
        let r = 4.0 + (i as f32).sqrt() * 4.2;
        let p = centre + egui::vec2(a.cos() * r * 0.6, a.sin() * r);
        if plot.contains(p) {
            painter.circle_filled(p, 1.3, c(Role::StereometerTrace));
        }
    }
    painter.text(
        plot.left_top() + egui::vec2(2.0, 0.0),
        egui::Align2::LEFT_TOP,
        "Preview",
        egui::FontId::proportional(10.0),
        c(Role::Text).gamma_multiply(0.6),
    );
}

/// The roles a Meter draws with, which it can override.
fn meter_roles(settings: &MeterSettings) -> &'static [Role] {
    match settings {
        MeterSettings::Waveform(_) => &[Role::WaveformLow, Role::WaveformMid, Role::WaveformHigh],
        MeterSettings::Spectrum(_) => &[
            Role::SpectrumLine,
            Role::SpectrumFill,
            Role::SpectrumPeakHold,
            Role::SpectrumPeakDots,
            Role::SpectrumHarmonics,
            Role::SpectrumSecond,
            Role::SpectrumCollision,
        ],
        MeterSettings::Loudness(_) => &[
            Role::LoudnessBar,
            Role::LoudnessPeak,
            Role::LoudnessOverTarget,
            Role::LoudnessCounter,
        ],
        MeterSettings::Stereometer(_) => &[
            Role::StereometerTrace,
            Role::CorrelationPositive,
            Role::CorrelationNegative,
        ],
        MeterSettings::Cepstrum(_) => &[Role::CepstrumTrace, Role::Accent],
        MeterSettings::Spectrogram(_) => &[
            Role::SpectrumFill,
            Role::SpectrumLine,
            Role::Accent,
            Role::Text,
        ],
        MeterSettings::PhaseScope(_) => &[
            Role::PhaseScopeTrace,
            Role::PhaseScopeCancel,
            Role::PhaseScopeSum,
            Role::PhaseScopeGrid,
        ],
        MeterSettings::Oscilloscope(_) => &[Role::OscilloscopeTrace, Role::OscilloscopeSecond],
        MeterSettings::TonalBalance(_) => TONAL_BALANCE_ROLES,
        MeterSettings::Tuner(_) => &[Role::TunerInTune, Role::TunerOff],
        MeterSettings::Needle(_) => &[Role::Text, Role::LoudnessCounter],
        MeterSettings::StereoBands(_) => &[
            Role::CorrelationPositive,
            Role::CorrelationNegative,
            Role::StereometerTrace,
        ],
    }
}

/// A Meter's own colours: each of its roles, the Theme's or overridden.
fn meter_colours(ui: &mut egui::Ui, scene: &Scene, meter: &MeterScene, actions: &mut Actions) {
    ui.label(RichText::new("Colours (this Meter only)").small());
    egui::Grid::new(("meter colours", meter.meter))
        .num_columns(3)
        .show(ui, |ui| {
            for &role in meter_roles(&meter.settings) {
                let own = meter.overrides.iter().find(|(r, _)| *r == role);
                ui.label(role.label());
                let shown = own.map_or(scene.palette[role], |(_, c)| *c);
                if let Some(colour) = colour_button(ui, shown) {
                    actions.push(Event::SetOverride {
                        meter: meter.meter,
                        role,
                        colour: Some(colour),
                    });
                }
                if own.is_some() && ui.small_button("Theme's").clicked() {
                    actions.push(Event::SetOverride {
                        meter: meter.meter,
                        role,
                        colour: None,
                    });
                }
                ui.end_row();
            }
        });
}

/// The Presets: the list to switch and order, and what to do with the current one.
fn preset_settings(ui: &mut egui::Ui, scene: &Scene, actions: &mut Actions) {
    let presets = &scene.presets;
    let count = presets.list.len();
    let modifier = if Platform::current() == Platform::MacOs {
        "Cmd"
    } else {
        "Ctrl"
    };
    for (i, preset) in presets.list.iter().enumerate() {
        ui.horizontal_wrapped(|ui| {
            // Order first, so a long name never pushes them out of sight.
            if ui
                .add_enabled(i > 0, egui::Button::new("↑").small())
                .on_hover_text("Move up")
                .clicked()
            {
                actions.push(Event::MovePreset { from: i, to: i - 1 });
            }
            if ui
                .add_enabled(i + 1 < count, egui::Button::new("↓").small())
                .on_hover_text("Move down")
                .clicked()
            {
                actions.push(Event::MovePreset { from: i, to: i + 1 });
            }
            let shortcut = if i < 9 {
                format!("{modifier}+{}", i + 1)
            } else {
                String::new()
            };
            ui.add_sized(
                [52.0, 18.0],
                egui::Label::new(RichText::new(shortcut).weak().small()),
            );
            let current = presets.current == Some(i);
            let label = if preset.built_in {
                format!("{} (built-in)", preset.name)
            } else {
                preset.name.clone()
            };
            let row = ui.add_enabled(!preset.broken, egui::Button::selectable(current, label));
            if row.clicked() && !current {
                actions.push(Event::SwitchPreset { index: i });
            }
            if preset.broken && ui.small_button("Show in Finder").clicked() {
                actions.requests.push(Request::ShowPresetInFolder {
                    file_name: preset.file_name.clone(),
                });
            }
        });
    }
    let Some(current) = presets.current else {
        return;
    };
    let info = &presets.list[current];
    ui.separator();
    // Rename: the typed name is kept in egui's memory until it's applied.
    let id = egui::Id::new(("preset name", &info.file_name));
    let mut name = ui
        .data_mut(|d| d.get_temp::<String>(id))
        .unwrap_or_else(|| info.name.clone());
    ui.horizontal(|ui| {
        let field = ui.add_enabled(
            !info.read_only,
            egui::TextEdit::singleline(&mut name).desired_width(180.0),
        );
        let rename = ui.add_enabled(
            !info.read_only && name.trim() != info.name,
            egui::Button::new("Rename"),
        );
        let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (rename.clicked() || entered) && name.trim() != info.name {
            actions.requests.push(Request::RenamePreset {
                index: current,
                name: name.trim().to_owned(),
            });
        }
    });
    ui.data_mut(|d| d.insert_temp(id, name));
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(presets.can_revert, egui::Button::new("Revert"))
            .on_hover_text("Back to how this Preset was when you opened it")
            .clicked()
        {
            actions.push(Event::RevertPreset);
        }
        if ui.button("Save as new").clicked() {
            actions.push(Event::SavePresetAsNew);
        }
        if ui.button("Duplicate").clicked() {
            actions.push(Event::DuplicatePreset { index: current });
        }
        if info.built_in && ui.button("Reset to built-in").clicked() {
            actions.push(Event::ResetPreset { index: current });
        }
        let readable = presets.list.iter().filter(|p| !p.broken).count();
        if ui
            .add_enabled(readable > 1, egui::Button::new("Delete"))
            .on_hover_text("Moves the file to the Trash")
            .clicked()
        {
            actions.push(Event::DeletePreset { index: current });
        }
        if ui.button("Show in Finder").clicked() {
            actions.requests.push(Request::ShowPresetInFolder {
                file_name: info.file_name.clone(),
            });
        }
        if ui
            .button("Export…")
            .on_hover_text("One file with the Preset and its own Themes, to share")
            .clicked()
        {
            actions.requests.push(Request::ExportPreset);
        }
        if ui.button("Import…").clicked() {
            actions.requests.push(Request::ImportPreset);
        }
    });
    if info.read_only {
        ui.label(
            RichText::new(
                "Made by a newer Das-Meter: changes aren't saved. Save as new to keep them.",
            )
            .weak(),
        );
    }
}

fn meter_title(meter_scene: &MeterScene) -> String {
    format!(
        "{} · {}",
        meter_scene.meter + 1,
        kind_name(&meter_scene.settings)
    )
}

/// Where the settings remember their page.
const PAGE_ID: &str = "settings page";

impl Ui {
    /// Opens the settings on the Theme page next time they show (for
    /// snapshots).
    pub fn show_theme_page(&self) {
        self.ctx
            .data_mut(|d| d.insert_temp(egui::Id::new(PAGE_ID), Page::Theme));
    }
}

/// A page of the settings sidebar.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Presets,
    App,
    Theme,
    Bar,
    Meter(usize),
    /// A kind no Meter shows now: its settings, greyed, and Add.
    Kind(MeterKind),
}

/// The settings: a sidebar of pages at the left, the chosen page at the
/// right, scrolling on its own. Every kind of Meter is listed: the ones
/// shown now by number, the others marked as not added.
fn sidebar_contents(ui: &mut egui::Ui, scene: &Scene, actions: &mut Actions) {
    let id = egui::Id::new(PAGE_ID);
    let meters = all_meters(scene);
    let mut page = ui.data(|d| d.get_temp::<Page>(id)).unwrap_or(Page::Presets);
    // A removed Meter's page falls back to the first.
    if let Page::Meter(m) = page
        && !meters.iter().any(|s| s.meter == m)
    {
        page = Page::Presets;
    }
    let unused: Vec<MeterKind> = MeterKind::ALL
        .into_iter()
        .filter(|&kind| !meters.iter().any(|m| m.settings.kind() == kind))
        .collect();
    // Just added: its page is the new Meter's.
    if let Page::Kind(kind) = page
        && let Some(m) = meters.iter().find(|m| m.settings.kind() == kind)
    {
        page = Page::Meter(m.meter);
    }
    egui::Panel::left("settings sidebar")
        .resizable(false)
        .exact_size(150.0)
        .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(4, 4)))
        .show(ui, |ui| {
            widgets::heading(ui, "Settings");
            for (choice, name) in [
                (Page::Presets, "Presets"),
                (Page::App, "App"),
                (Page::Theme, "Theme"),
                (Page::Bar, "Bar"),
            ] {
                page_button(ui, &mut page, choice, name);
            }
            widgets::heading(ui, "Meters");
            for meter_scene in &meters {
                page_button(
                    ui,
                    &mut page,
                    Page::Meter(meter_scene.meter),
                    &meter_title(meter_scene),
                );
            }
            for &kind in &unused {
                let dim = ui.visuals().weak_text_color();
                let text = RichText::new(format!("○ {}", kind_label(kind))).color(dim);
                let button = egui::Button::selectable(page == Page::Kind(kind), text)
                    .min_size(egui::vec2(ui.available_width(), 24.0));
                if ui
                    .add(button)
                    .on_hover_text("Not added: open to see its settings and add it")
                    .clicked()
                {
                    page = Page::Kind(kind);
                }
            }
        });
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.inner_margin(egui::Margin {
            left: 12,
            right: 4,
            top: 4,
            bottom: 4,
        }))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("settings page", page_key(page)))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // Wrap to the window, so a narrower one reflows the
                    // page instead of cutting it off.
                    ui.set_max_width(ui.available_width());
                    missing_displays_note(ui, scene, actions);
                    match page {
                        Page::Presets => preset_settings(ui, scene, actions),
                        Page::App => app_settings(ui, scene, actions),
                        Page::Theme => theme_settings(ui, scene, actions),
                        Page::Bar => bar_settings(ui, scene, actions),
                        Page::Meter(meter) => {
                            if let Some(meter_scene) = meters.iter().find(|s| s.meter == meter) {
                                meter_settings(ui, scene, meter_scene, actions);
                            }
                        }
                        Page::Kind(kind) => unused_kind(ui, scene, &meters, kind, actions),
                    }
                });
        });
    ui.data_mut(|d| d.insert_temp(id, page));
}

fn page_key(page: Page) -> usize {
    match page {
        Page::Presets => usize::MAX,
        Page::App => usize::MAX - 1,
        Page::Theme => usize::MAX - 2,
        Page::Bar => usize::MAX - 3,
        Page::Meter(meter) => meter,
        Page::Kind(kind) => usize::MAX - 10 - kind as usize,
    }
}

/// A kind of Meter no Meter shows: Add puts one after the last Meter, and
/// its default settings show, greyed, for what it offers.
fn unused_kind(
    ui: &mut egui::Ui,
    scene: &Scene,
    meters: &[&MeterScene],
    kind: MeterKind,
    actions: &mut Actions,
) {
    ui.label(RichText::new(format!("{} isn't added.", kind_label(kind))).weak());
    let last = meters.iter().map(|m| m.meter).max();
    let added = scene.mode == LayoutMode::Window
        || scene.windows.iter().any(|w| {
            w.key == WindowKey::Bar && last.is_some_and(|l| w.meters.iter().any(|m| m.meter == l))
        });
    if let Some(meter) = last.filter(|_| added)
        && ui.button(format!("Add {}", kind_label(kind))).clicked()
    {
        actions.push(Event::AddMeter { meter, kind });
    }
    ui.add_space(6.0);
    ui.add_enabled_ui(false, |ui| {
        let mut ignored = Actions::new();
        let settings = MeterSettings::default_of(kind);
        basic(ui, usize::MAX, settings, &mut ignored);
        ui.separator();
        advanced(ui, usize::MAX, settings, &mut ignored);
    });
}

fn page_button(ui: &mut egui::Ui, page: &mut Page, choice: Page, name: &str) {
    let button = egui::Button::selectable(*page == choice, name)
        .min_size(egui::vec2(ui.available_width(), 24.0));
    if ui.add(button).clicked() {
        *page = choice;
    }
}

/// Where Das-Meter listens, the layout, frame rate and start-up.
fn app_settings(ui: &mut egui::Ui, scene: &Scene, actions: &mut Actions) {
    listen_to(ui, scene, actions);
    let mut mode = scene.mode;
    let modes = [(LayoutMode::Bar, "Bar"), (LayoutMode::Window, "Window")];
    if choice(ui, "Layout", &mut mode, &modes) {
        actions.push(Event::SetMode(mode));
    }
    let mut app = scene.app;
    let mut changed = ui
        .add(
            Slider::new(
                &mut app.frame_rate_cap,
                limits::MIN_FRAME_RATE_CAP..=scene.max_frame_rate_cap,
            )
            .text("Frame-rate cap")
            .suffix(" fps"),
        )
        .changed();
    changed |= ui
        .add(Toggle::new(
            &mut app.check_for_updates,
            "Check for updates daily",
        ))
        .changed();
    if changed {
        actions.push(Event::SetApp(app));
    }
    if cfg!(windows) {
        let offer: UpdateOffer = ui
            .ctx()
            .data(|data| data.get_temp(update_offer_id()))
            .unwrap_or_default();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!offer.busy, egui::Button::new("Check Now"))
                .clicked()
            {
                actions.requests.push(Request::CheckForUpdates);
            }
            if let Some(version) = &offer.available {
                let button = egui::Button::new(
                    RichText::new(format!("Update to Das-Meter {version}…")).strong(),
                );
                if ui.add_enabled(!offer.busy, button).clicked() {
                    actions.requests.push(Request::Update);
                }
            }
        });
    }
    let mut launch_at_login = scene.launch_at_login;
    if ui
        .add(Toggle::new(&mut launch_at_login, "Launch at login"))
        .changed()
    {
        actions.push(Event::SetLaunchAtLogin(launch_at_login));
    }
    if cfg!(target_os = "macos") {
        let mut show_in_dock = scene.show_in_dock;
        if ui
            .add(Toggle::new(&mut show_in_dock, "Show in Dock"))
            .on_hover_text("The menu bar icon is always there")
            .changed()
        {
            actions.push(Event::SetShowInDock(show_in_dock));
        }
    }
}

/// Every setting of one Meter, and its colours.
fn meter_settings(
    ui: &mut egui::Ui,
    scene: &Scene,
    meter_scene: &MeterScene,
    actions: &mut Actions,
) {
    let meter = meter_scene.meter;
    // Both halves edit one copy, so a change in each is one event.
    let mut edited = Actions::new();
    basic(ui, meter, meter_scene.settings, &mut edited);
    let settings = edited
        .iter()
        .find_map(|event| match event {
            Event::SetMeter { settings, .. } => Some(*settings),
            _ => None,
        })
        .unwrap_or(meter_scene.settings);
    reference_items(ui, scene, meter_scene, actions);
    target_items(ui, scene, meter_scene, actions);
    ui.separator();
    advanced(ui, meter, settings, &mut edited);
    ui.separator();
    meter_colours(ui, scene, meter_scene, &mut edited);
    if let Some(last) = edited
        .iter()
        .rev()
        .find(|e| matches!(e, Event::SetMeter { .. }))
        .copied()
    {
        edited.retain(|e| !matches!(e, Event::SetMeter { .. }));
        edited.push(last);
    }
    actions.extend(edited);
}

/// The settings panel filling its own window.
fn settings_window(root: &mut egui::Ui, scene: &Scene, actions: &mut Actions) {
    egui::CentralPanel::default().show(root, |ui| sidebar_contents(ui, scene, actions));
}

/// The settings panel over the Meters (snapshots).
fn settings_panel(ctx: &egui::Context, scene: &Scene, actions: &mut Actions) {
    if !scene.settings_open {
        return;
    }
    let mut open = true;
    let screen = ctx.content_rect();
    egui::Window::new("Settings")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size(egui::vec2(640.0, (screen.height() - 24.0).max(200.0)))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 12.0))
        .vscroll(false)
        .show(ctx, |ui| sidebar_contents(ui, scene, actions));
    if !open {
        actions.push(Event::ShowSettings(false));
    }
}
