//! A plain Loudness Meter renderer: level bars and numbers in fixed colours.
//!
//! It turns one [`LoudnessScene`] into rectangles and text. All layout is in
//! physical pixels, scaled by the window's scale factor.

use dasmeter_core::{Level, LoudnessDisplay, LoudnessScene, Note};
use glyphon::{
    Attrs, Buffer, Family, Metrics, Shaping, TextArea, TextBounds, TextRenderer, Weight,
};

use super::rects::{Rect, Rects};
use crate::gpu::{Colour, Gpu, Text};

const TEXT: Colour = Colour::rgb(0xd8, 0xdd, 0xe3);
const DIM: Colour = Colour::rgb(0x7c, 0x85, 0x91);
const NOTE: Colour = Colour::rgb(0xff, 0xb3, 0x47);
const TRACK: Colour = Colour::rgb(0x22, 0x26, 0x2b);
const TICK: Colour = Colour::rgb(0x33, 0x39, 0x40);
const RMS: Colour = Colour::rgb(0x2f, 0xbf, 0x71);
const PEAK: Colour = RMS.with_alpha(0.4);
const HOLD: Colour = Colour::rgb(0xe8, 0xe8, 0xe8);
const MOMENTARY: Colour = Colour::rgb(0x4d, 0xa3, 0xff);

/// The bars show −60 dB to 0 dB (dBFS for levels, LUFS for momentary loudness).
const RANGE_DB: f32 = 60.0;
const SCALE: [i32; 8] = [0, -6, -12, -18, -24, -36, -48, -60];

/// One line of text to draw, at a position in physical pixels.
struct Label {
    text: String,
    x: f32,
    y: f32,
    size: f32,
    colour: Colour,
    bold: bool,
}

pub struct LoudnessRenderer {
    rects: Rects,
    text: TextRenderer,
    /// Shaped text, reused while a label's text and size stay the same.
    buffers: Vec<(String, f32, bool, Buffer)>,
    labels: Vec<Label>,
}

impl LoudnessRenderer {
    pub fn new(gpu: &Gpu, text: &mut Text) -> LoudnessRenderer {
        LoudnessRenderer {
            rects: Rects::new(gpu),
            text: TextRenderer::new(
                &mut text.atlas,
                &gpu.device,
                wgpu::MultisampleState::default(),
                None,
            ),
            buffers: Vec::new(),
            labels: Vec::new(),
        }
    }

    /// Lays out the Meter and uploads everything for [`LoudnessRenderer::render`].
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        text: &mut Text,
        scale: f32,
        scene: &LoudnessScene,
        notes: &[Note],
    ) {
        self.rects.clear();
        self.labels.clear();
        let (width, height) = gpu.size();
        let (width, height) = (width as f32, height as f32);
        let pad = 16.0 * scale;

        match scene {
            LoudnessScene::Starting => {
                self.label("Starting System Capture…", pad, pad, 15.0 * scale, DIM)
            }
            LoudnessScene::Unavailable(reason) => {
                self.label(
                    "System Capture is unavailable",
                    pad,
                    pad,
                    15.0 * scale,
                    TEXT,
                );
                self.label(reason, pad, pad + 24.0 * scale, 13.0 * scale, DIM);
            }
            LoudnessScene::Live(display) => {
                self.layout_live(gpu, display, scale, width, height - 28.0 * scale)
            }
        }
        for (i, note) in notes.iter().enumerate() {
            let y = height - pad - 18.0 * scale * (notes.len() - i) as f32;
            self.label(note.text(), pad, y, 14.0 * scale, NOTE);
        }

        self.rects.prepare(gpu);
        self.prepare_text(gpu, text);
    }

    fn layout_live(
        &mut self,
        gpu: &Gpu,
        display: &LoudnessDisplay,
        scale: f32,
        width: f32,
        bottom: f32,
    ) {
        let pad = 16.0 * scale;
        let line = 21.0 * scale;
        let size = 15.0 * scale;

        let true_peak = if display.true_peak_oversampled {
            "TP"
        } else {
            "Peak"
        };
        let rows = [
            ("M", display.momentary, "LUFS"),
            ("S", display.short_term, "LUFS"),
            ("I", display.integrated, "LUFS"),
            ("LRA", display.range, "LU"),
            (true_peak, display.true_peak_max, "dBTP"),
        ];
        for (i, (name, level, unit)) in rows.into_iter().enumerate() {
            let y = pad + line * i as f32;
            self.label(name, pad, y, size, DIM);
            self.bold(
                &format!("{:>6}", number(level)),
                pad + 44.0 * scale,
                y,
                size,
                TEXT,
            );
            self.label(unit, pad + 112.0 * scale, y, size, DIM);
        }
        let rate = format!("{:.1} kHz", f64::from(display.sample_rate) / 1000.0);
        self.label(&rate, width - pad - 64.0 * scale, pad, 12.0 * scale, DIM);

        // Bars: L and R levels, then momentary loudness.
        let top = pad + line * rows.len() as f32 + 16.0 * scale;
        let names = 18.0 * scale;
        let bar_bottom = bottom - names;
        let bar_height = (bar_bottom - top).max(0.0);
        let left = pad + 34.0 * scale;
        let gap = 10.0 * scale;
        let bar_width = ((width - pad - left - 3.0 * gap) / 3.0).max(1.0);
        let y_of = |db: f64| {
            let fraction = ((-db as f32) / RANGE_DB).clamp(0.0, 1.0);
            top + fraction * bar_height
        };

        for db in SCALE {
            let y = y_of(f64::from(db));
            self.label(&db.to_string(), pad, y - 7.0 * scale, 11.0 * scale, DIM);
            let tick = Rect {
                x: left - 4.0 * scale,
                y,
                width: width - pad - left + 4.0 * scale,
                height: scale.max(1.0),
            };
            self.rects.push(gpu, tick, TICK);
        }

        let columns = [
            ("L", Some(display.left)),
            ("R", Some(display.right)),
            ("M", None),
        ];
        for (i, (name, channel)) in columns.into_iter().enumerate() {
            let x = left + i as f32 * (bar_width + gap) + if i == 2 { gap } else { 0.0 };
            let track = Rect {
                x,
                y: top,
                width: bar_width,
                height: bar_height,
            };
            self.rects.push(gpu, track, TRACK);
            let mut fill = |level: Level, colour: Colour| {
                if let Some(db) = level.db() {
                    let y = y_of(db);
                    self.rects.push(
                        gpu,
                        Rect {
                            x,
                            y,
                            width: bar_width,
                            height: top + bar_height - y,
                        },
                        colour,
                    );
                }
            };
            match channel {
                Some(levels) => {
                    fill(levels.peak, PEAK);
                    fill(levels.rms, RMS);
                    if let Some(db) = levels.peak_hold.db() {
                        let y = y_of(db);
                        let hold = Rect {
                            x,
                            y: y - scale,
                            width: bar_width,
                            height: 2.0 * scale,
                        };
                        self.rects.push(gpu, hold, HOLD);
                    }
                }
                None => fill(display.momentary, MOMENTARY),
            }
            self.label(
                name,
                x + bar_width / 2.0 - 4.0 * scale,
                bar_bottom + 2.0 * scale,
                12.0 * scale,
                DIM,
            );
        }
    }

    fn label(&mut self, text: &str, x: f32, y: f32, size: f32, colour: Colour) {
        self.labels.push(Label {
            text: text.to_owned(),
            x,
            y,
            size,
            colour,
            bold: false,
        });
    }

    fn bold(&mut self, text: &str, x: f32, y: f32, size: f32, colour: Colour) {
        self.labels.push(Label {
            text: text.to_owned(),
            x,
            y,
            size,
            colour,
            bold: true,
        });
    }

    fn prepare_text(&mut self, gpu: &Gpu, text: &mut Text) {
        self.buffers.truncate(self.labels.len());
        for (i, label) in self.labels.iter().enumerate() {
            let fresh = self.buffers.get(i).is_some_and(|(t, s, b, _)| {
                *t == label.text && *s == label.size && *b == label.bold
            });
            if fresh {
                continue;
            }
            let mut buffer = Buffer::new(
                &mut text.font_system,
                Metrics::new(label.size, label.size * 1.25),
            );
            let attrs = Attrs::new()
                .family(Family::Monospace)
                .weight(if label.bold {
                    Weight::BOLD
                } else {
                    Weight::NORMAL
                });
            buffer.set_size(None, None);
            buffer.set_text(&label.text, &attrs, Shaping::Basic, None);
            buffer.shape_until_scroll(&mut text.font_system, false);
            let entry = (label.text.clone(), label.size, label.bold, buffer);
            if i < self.buffers.len() {
                self.buffers[i] = entry;
            } else {
                self.buffers.push(entry);
            }
        }

        let (width, height) = gpu.size();
        let areas = self
            .labels
            .iter()
            .zip(&self.buffers)
            .map(|(label, (_, _, _, buffer))| TextArea {
                buffer,
                left: label.x,
                top: label.y,
                scale: 1.0,
                bounds: TextBounds {
                    left: 0,
                    top: 0,
                    right: width as i32,
                    bottom: height as i32,
                },
                default_color: label.colour.glyphon(),
                custom_glyphs: &[],
            });
        text.update_viewport(gpu);
        if let Err(error) = self.text.prepare(
            &gpu.device,
            &gpu.queue,
            &mut text.font_system,
            &mut text.atlas,
            &text.viewport,
            areas,
            &mut text.swash_cache,
        ) {
            eprintln!("text: {error}");
        }
    }

    pub fn render(&self, text: &Text, pass: &mut wgpu::RenderPass) {
        self.rects.render(pass);
        if let Err(error) = self.text.render(&text.atlas, &text.viewport, pass) {
            eprintln!("text: {error}");
        }
    }
}

/// A reading as shown: one decimal, or "-inf" for silence.
fn number(level: Level) -> String {
    match level.db() {
        Some(db) => format!("{db:.1}"),
        None => "-inf".to_owned(),
    }
}
