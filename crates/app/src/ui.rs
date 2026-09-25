//! The egui layer: each Meter's menu, the settings panel and the
//! first-launch card, each in its own window (or over the Meters for snapshots).
//!
//! It keeps no settings of its own. Each frame it draws what the scene says
//! and turns what the user changes into app core events.

use std::time::Duration;

use dasmeter_analysis::spectrum::{MAX_FFT_SIZE, MIN_FFT_SIZE};
use dasmeter_analysis::{
    ChannelView, PeakHold, RmsMode, SpectrumStyle, StereoScaling, StereoView, WaveformScale,
    WindowFunction,
};
use dasmeter_core::docs::Topic;
use dasmeter_core::layout::NO_RESERVE_SPACE_ON_MACOS;
use dasmeter_core::settings::{self as limits, MEASUREMENTS_NOTE, MEASUREMENTS_URL};
use dasmeter_core::{
    Card, Colour, Direction, Edge, Event, LayoutMode, LineWeight, ListenTo, LoudnessMeterSettings,
    LufsBar, MeterKind, MeterScene, MeterSettings, Palette, Platform, Role, Scene, ScreenMode,
    SpectrumMeterSettings, StereoDrawing, StereometerMeterSettings, WaveformColouring,
    WaveformMeterSettings, WindowKey,
};
use egui::{Color32, RichText, Slider};

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
    palette: Option<Palette>,
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
        if self.palette.as_ref() != Some(&scene.palette) {
            self.ctx.set_visuals(visuals(&scene.palette));
            self.palette = Some(scene.palette.clone());
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
fn visuals(palette: &Palette) -> egui::Visuals {
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
    v
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
    ui.horizontal(|ui| {
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
        .checkbox(&mut infinite, "Hold peaks until reset")
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
    changed |= ui.checkbox(&mut s.show_peak_hold, "Peak hold").changed();
    changed
}

/// The Loudness Meter's basic settings; `reset` is set when Reset is clicked.
fn loudness_basic(ui: &mut egui::Ui, s: &mut LoudnessMeterSettings, reset: &mut bool) -> bool {
    let mut on = s.target.is_some();
    let mut target = s.target.unwrap_or(-14.0);
    let mut changed = false;
    ui.horizontal(|ui| {
        changed |= ui.checkbox(&mut on, "Target").changed();
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
    changed |= ui.checkbox(&mut s.show_balance, "Balance bar").changed();
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
        .checkbox(&mut s.show_true_peak, "Show true peak")
        .changed();
    changed |= ui.checkbox(&mut s.show_range, "Show LRA").changed();
    changed |= ui
        .checkbox(&mut s.show_peak_to_loudness, "Show PLR and PSR")
        .on_hover_text(
            "Peak to loudness: the true-peak maximum minus integrated loudness (PLR), \
             and the last 3 s' true peak minus short-term loudness (PSR)",
        )
        .changed();
    changed |= ui
        .checkbox(&mut s.show_history, "Loudness graph")
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
    let mut reset = false;
    let changed = match &mut s {
        MeterSettings::Waveform(w) => waveform_basic(ui, w),
        MeterSettings::Spectrum(w) => spectrum_basic(ui, w),
        MeterSettings::Loudness(w) => loudness_basic(ui, w, &mut reset),
        MeterSettings::Stereometer(w) => stereometer_basic(ui, w),
    };
    if changed {
        actions.push(Event::SetMeter { meter, settings: s });
    }
    if reset {
        actions.push(Event::ResetLoudness { meter });
    }
}

fn advanced(ui: &mut egui::Ui, meter: usize, settings: MeterSettings, actions: &mut Actions) {
    let mut s = settings;
    let changed = match &mut s {
        MeterSettings::Waveform(w) => waveform_advanced(ui, w),
        MeterSettings::Spectrum(w) => spectrum_advanced(ui, w),
        MeterSettings::Loudness(w) => loudness_advanced(ui, w),
        MeterSettings::Stereometer(w) => stereometer_advanced(ui, w),
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
    if ui.checkbox(&mut shown, "Show Source label").changed() {
        actions.push(Event::ShowSourceLabel { meter, shown });
    }
}

/// A pane's items in Window mode: which Meter it shows, split and close.
fn pane_item(ui: &mut egui::Ui, scene: &Scene, meter: usize, actions: &mut Actions) {
    let Some(meter_scene) = meter_scene(scene, meter) else {
        return;
    };
    let mut kind = meter_scene.settings.kind();
    let kinds = MeterKind::ALL.map(|k| {
        (
            k,
            match k {
                MeterKind::Waveform => "Waveform",
                MeterKind::Spectrum => "Spectrum",
                MeterKind::Loudness => "Loudness",
                MeterKind::Stereometer => "Stereo",
            },
        )
    });
    if choice(ui, "Show", &mut kind, &kinds) {
        actions.push(Event::AssignMeter { meter, kind });
    }
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
    let panes = scene.windows.first().map_or(0, |w| w.meters.len());
    if ui
        .add_enabled(panes > 1, egui::Button::new("Close pane"))
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
    let pop_out = scene
        .windows
        .iter()
        .find(|w| w.key == WindowKey::PopOut(meter));
    match pop_out {
        Some(window) => {
            let mut on_top = window.on_top;
            if ui.checkbox(&mut on_top, "Always on top").changed() {
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
            if ui
                .add_enabled(in_bar > 1, egui::Button::new("Pop out"))
                .clicked()
            {
                actions.push(Event::PopOut { meter });
            }
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
    ui.separator();
    basic(ui, meter, meter_scene.settings, actions);
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
            } else {
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
    let Some(bar) = scene.windows.iter().find(|w| w.key == WindowKey::Bar) else {
        return;
    };
    if let Some(mut edge) = bar.edge {
        let edges = [
            (Edge::Top, "Top"),
            (Edge::Bottom, "Bottom"),
            (Edge::Left, "Left"),
            (Edge::Right, "Right"),
        ];
        if choice(ui, "Edge", &mut edge, &edges) {
            actions.push(Event::SetEdge(edge));
        }
    }
    if let (Some(frame), Some(edge)) = (bar.frame, bar.edge) {
        let mut thickness = if edge.horizontal() {
            frame.height
        } else {
            frame.width
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
    }
    if let Some(mode) = bar.screen {
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
    }
    let mut over = bar.over_fullscreen;
    if ui
        .checkbox(&mut over, "Show over fullscreen apps")
        .changed()
    {
        actions.push(Event::ShowOverFullscreen(over));
    }
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
        .checkbox(&mut follow, "Follow the system's light / dark setting")
        .changed()
    {
        // Following: Light and Dark; not: the Theme in use for both.
        let (light, dark) = if follow {
            (1, 0)
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
    if !editable {
        ui.label(
            RichText::new("Built-in Themes can't be edited. Duplicate one to make your own.")
                .weak(),
        );
        return;
    }
    // Styling.
    let mut styling = theme.styling;
    let mut changed = ui
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
        .checkbox(&mut styling.shape_cues, "Shape cues besides colour")
        .changed();
    if changed {
        actions.push(Event::SetThemeStyling {
            theme: current,
            styling,
        });
    }
    // Colours, with a live preview: the Meters and this panel use them at once.
    egui::Grid::new("theme colours")
        .num_columns(2)
        .show(ui, |ui| {
            for role in Role::ALL {
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

/// The roles a Meter draws with, which it can override.
fn meter_roles(settings: &MeterSettings) -> &'static [Role] {
    match settings {
        MeterSettings::Waveform(_) => &[Role::WaveformLow, Role::WaveformMid, Role::WaveformHigh],
        MeterSettings::Spectrum(_) => &[
            Role::SpectrumLine,
            Role::SpectrumFill,
            Role::SpectrumPeakHold,
        ],
        MeterSettings::Loudness(_) => &[
            Role::LoudnessBar,
            Role::LoudnessPeak,
            Role::LoudnessOverTarget,
        ],
        MeterSettings::Stereometer(_) => &[
            Role::StereometerTrace,
            Role::CorrelationPositive,
            Role::CorrelationNegative,
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
    for (i, preset) in presets.list.iter().enumerate() {
        ui.horizontal(|ui| {
            let shortcut = if i < 9 {
                format!("⌘{} ", i + 1)
            } else {
                "    ".to_owned()
            };
            ui.label(RichText::new(shortcut).weak().monospace());
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
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(i + 1 < count, egui::Button::new("↓").small())
                    .clicked()
                {
                    actions.push(Event::MovePreset { from: i, to: i + 1 });
                }
                if ui
                    .add_enabled(i > 0, egui::Button::new("↑").small())
                    .clicked()
                {
                    actions.push(Event::MovePreset { from: i, to: i - 1 });
                }
            });
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

/// App settings, the Theme, the Bar, then every setting of every Meter.
fn panel_contents(ui: &mut egui::Ui, scene: &Scene, actions: &mut Actions) {
    missing_displays_note(ui, scene, actions);
    egui::CollapsingHeader::new("Presets")
        .default_open(true)
        .show(ui, |ui| preset_settings(ui, scene, actions));
    egui::CollapsingHeader::new("App")
        .default_open(true)
        .show(ui, |ui| {
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
                .checkbox(&mut app.check_for_updates, "Check for updates daily")
                .changed();
            if changed {
                actions.push(Event::SetApp(app));
            }
            let mut launch_at_login = scene.launch_at_login;
            if ui
                .checkbox(&mut launch_at_login, "Launch at login")
                .changed()
            {
                actions.push(Event::SetLaunchAtLogin(launch_at_login));
            }
            if cfg!(target_os = "macos") {
                let mut show_in_dock = scene.show_in_dock;
                if ui
                    .checkbox(&mut show_in_dock, "Show in Dock")
                    .on_hover_text("The menu bar icon is always there")
                    .changed()
                {
                    actions.push(Event::SetShowInDock(show_in_dock));
                }
            }
        });
    egui::CollapsingHeader::new("Theme")
        .default_open(false)
        .show(ui, |ui| theme_settings(ui, scene, actions));
    egui::CollapsingHeader::new("Bar")
        .default_open(false)
        .show(ui, |ui| bar_settings(ui, scene, actions));
    for meter_scene in all_meters(scene) {
        let meter = meter_scene.meter;
        let title = format!("{} · {}", meter + 1, kind_name(&meter_scene.settings));
        egui::CollapsingHeader::new(title)
            .id_salt(("meter settings", meter))
            .default_open(false)
            .show(ui, |ui| {
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
            });
    }
}

/// The settings panel filling its own window.
fn settings_window(root: &mut egui::Ui, scene: &Scene, actions: &mut Actions) {
    egui::CentralPanel::default().show(root, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| panel_contents(ui, scene, actions));
    });
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
        .default_size(egui::vec2(420.0, (screen.height() - 24.0).max(200.0)))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 12.0))
        .vscroll(true)
        .show(ctx, |ui| panel_contents(ui, scene, actions));
    if !open {
        actions.push(Event::ShowSettings(false));
    }
}
