//! The egui layer: each Meter's menu and the settings panel, drawn over the
//! Meters on the same surface.
//!
//! It keeps no settings of its own. Each frame it draws what the scene says
//! and turns what the user changes into app core events.

use std::time::Duration;

use dasmeter_analysis::spectrum::{MAX_FFT_SIZE, MIN_FFT_SIZE};
use dasmeter_analysis::{
    ChannelView, PeakHold, RmsMode, SpectrumStyle, StereoScaling, StereoView, WaveformScale,
    WindowFunction,
};
use dasmeter_core::settings::{self as limits, MEASUREMENTS_NOTE, MEASUREMENTS_URL};
use dasmeter_core::{
    Colour, Event, ListenTo, LoudnessMeterSettings, LufsBar, MeterSettings, Palette, Role, Scene,
    SpectrumMeterSettings, StereoDrawing, StereometerMeterSettings, WaveformColouring,
    WaveformMeterSettings,
};
use egui::{Color32, RichText, Slider};

/// What the user did this frame, as app core events.
pub type Actions = Vec<Event<'static>>;

/// The egui context the menus and the settings panel are built in.
pub struct Ui {
    pub ctx: egui::Context,
    palette: Option<Palette>,
}

impl Ui {
    pub fn new() -> Ui {
        Ui {
            ctx: egui::Context::default(),
            palette: None,
        }
    }

    /// Gives egui the system's sans-serif and monospace fonts, as the Meters'
    /// text uses. (egui's bundled fonts carry font licences we don't take on.)
    pub fn use_fonts(&self, fonts: &glyphon::fontdb::Database) {
        use glyphon::fontdb::{Family, Query};

        let load = |family| {
            let id = fonts.query(&Query {
                families: &[family],
                ..Query::default()
            })?;
            fonts.with_face_data(id, |data, index| {
                let mut font = egui::FontData::from_owned(data.to_vec());
                font.index = index;
                font
            })
        };
        let Some(sans) = load(Family::SansSerif) else {
            return;
        };
        let mono = load(Family::Monospace).unwrap_or_else(|| sans.clone());
        let mut definitions = egui::FontDefinitions::empty();
        definitions.font_data.insert("sans".into(), sans.into());
        definitions.font_data.insert("mono".into(), mono.into());
        definitions.families.insert(
            egui::FontFamily::Proportional,
            vec!["sans".into(), "mono".into()],
        );
        definitions.families.insert(
            egui::FontFamily::Monospace,
            vec!["mono".into(), "sans".into()],
        );
        self.ctx.set_fonts(definitions);
    }

    /// Builds this frame's menus and panel from `scene`. Returns egui's output
    /// (shapes and platform requests) and what the user changed.
    pub fn run(&mut self, input: egui::RawInput, scene: &Scene) -> (egui::FullOutput, Actions) {
        if self.palette.as_ref() != Some(&scene.palette) {
            self.ctx.set_visuals(visuals(&scene.palette));
            self.palette = Some(scene.palette.clone());
        }
        let mut actions = Actions::new();
        let output = self.ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            meter_menu(&ctx, scene, &mut actions);
            settings_panel(&ctx, scene, &mut actions);
        });
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
    match settings {
        MeterSettings::Waveform(_) => "Waveform",
        MeterSettings::Spectrum(_) => "Spectrum",
        MeterSettings::Loudness(_) => "Loudness Meter",
        MeterSettings::Stereometer(_) => "Stereometer",
    }
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
    let meter_scene = &scene.windows[0].meters[meter];
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
            let can_share = meter_scene.picked.is_some() && scene.windows[0].meters.len() > 1;
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

/// The open Meter menu, where it was right-clicked.
fn meter_menu(ctx: &egui::Context, scene: &Scene, actions: &mut Actions) {
    let Some(menu) = scene.menu else { return };
    let Some(meter_scene) = scene.windows.first().and_then(|w| w.meters.get(menu.meter)) else {
        return;
    };
    let screen = ctx.content_rect();
    let at = egui::pos2(
        screen.min.x + menu.at[0] * screen.width(),
        screen.min.y + menu.at[1] * screen.height(),
    );
    let response = egui::Area::new(egui::Id::new("meter menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .constrain(true)
        .show(ctx, |ui| {
            egui::Frame::menu(ui.style()).show(ui, |ui| {
                ui.set_width(280.0);
                ui.label(RichText::new(kind_name(&meter_scene.settings)).strong());
                source_item(ui, scene, menu.meter, actions);
                ui.separator();
                basic(ui, menu.meter, meter_scene.settings, actions);
                ui.separator();
                if ui.button("Settings…").clicked() {
                    actions.push(Event::ShowSettings(true));
                }
            });
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        actions.push(Event::CloseMenu);
    }
    let _ = response;
}

/// The settings panel: app settings, then every setting of every Meter.
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
        .show(ctx, |ui| {
            egui::CollapsingHeader::new("App")
                .default_open(true)
                .show(ui, |ui| {
                    listen_to(ui, scene, actions);
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
                });
            for (meter, meter_scene) in scene.windows[0].meters.iter().enumerate() {
                let title = format!("{} · {}", meter + 1, kind_name(&meter_scene.settings));
                egui::CollapsingHeader::new(title)
                    .id_salt(("meter settings", meter))
                    .default_open(meter == 0)
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
        });
    if !open || (ctx.input(|i| i.key_pressed(egui::Key::Escape)) && scene.menu.is_none()) {
        actions.push(Event::ShowSettings(false));
    }
}
