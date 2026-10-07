//! Meter renderers: one per Meter, each its own small shape buffer and text,
//! drawing that Meter's part of the scene with the palette's colour roles.

mod cepstrum;
mod heatmap;
mod labels;
mod loudness;
mod phase_scope;
mod shapes;
mod spectrogram;
mod spectrum;
mod stereometer;
mod waveform;

use dasmeter_core::{
    Colour, MeterScene, MeterState, MeterView, Note, Palette, Role, SourceItem, Styling,
};

use crate::gpu::{Gpu, Text};
use heatmap::Heatmap;
use labels::{Align, Labels, Style};
pub use shapes::Area;
use shapes::Shapes;

/// What a Meter's drawing code draws with: shapes, labels, colours and the scale factor.
pub struct Canvas<'a> {
    pub shapes: &'a mut Shapes,
    /// Shapes drawn over the heat map (and under the labels).
    pub overlay: &'a mut Shapes,
    pub heatmap: &'a mut Heatmap,
    labels: &'a mut Labels,
    palette: &'a Palette,
    /// Physical pixels per logical pixel.
    pub scale: f32,
    /// The Theme's styling: line weight, text size, shape cues.
    pub styling: Styling,
    /// Text that would fall in this area is left out: something drawn over
    /// it (labels are drawn after all shapes) covers it.
    pub covered: Option<Area>,
}

impl Canvas<'_> {
    /// Logical pixels to physical ones.
    pub fn px(&self, logical: f32) -> f32 {
        logical * self.scale
    }

    /// A line width in logical pixels, in physical ones at the Theme's line weight.
    pub fn stroke(&self, logical: f32) -> f32 {
        self.px(logical) * self.styling.line.factor()
    }

    pub fn colour(&self, role: Role) -> Colour {
        self.palette[role]
    }

    /// Secondary text: labels, units, scales.
    pub fn dim(&self) -> Colour {
        self.palette[Role::Text].faded(0.55)
    }

    /// Whether text at `x`, `y` would run into the covered area.
    fn hidden(&self, text: &str, x: f32, y: f32, size: f32, align: Align) -> bool {
        let Some(covered) = self.covered else {
            return false;
        };
        let height = self.px(size) * self.styling.text_scale;
        let width = 0.6 * height * text.chars().count() as f32;
        let left = match align {
            Align::Left => x,
            Align::Centre => x - width / 2.0,
            Align::Right => x - width,
        };
        left < covered.right()
            && left + width > covered.x
            && y < covered.bottom()
            && y + height > covered.y
    }

    /// Text whose top edge is at `y`, `size` logical pixels high.
    pub fn text(&mut self, text: &str, x: f32, y: f32, size: f32, colour: Colour, align: Align) {
        if self.hidden(text, x, y, size, align) {
            return;
        }
        let style = Style {
            size: self.px(size) * self.styling.text_scale,
            colour,
            bold: false,
            align,
        };
        self.labels.add(text, x, y, style);
    }

    pub fn bold(&mut self, text: &str, x: f32, y: f32, size: f32, colour: Colour, align: Align) {
        if self.hidden(text, x, y, size, align) {
            return;
        }
        let style = Style {
            size: self.px(size) * self.styling.text_scale,
            colour,
            bold: true,
            align,
        };
        self.labels.add(text, x, y, style);
    }
}

/// How a window is drawn: its colours, the Theme's styling and the scale.
#[derive(Clone, Copy)]
pub struct Look<'a> {
    pub palette: &'a Palette,
    pub styling: Styling,
    /// Physical pixels per logical pixel.
    pub scale: f32,
}

/// Draws one Meter (or the notes over a window) into its area.
pub struct MeterRenderer {
    shapes: Shapes,
    heatmap: Heatmap,
    overlay: Shapes,
    labels: Labels,
}

impl MeterRenderer {
    pub fn new(gpu: &Gpu, text: &mut Text) -> MeterRenderer {
        MeterRenderer {
            shapes: Shapes::new(gpu),
            heatmap: Heatmap::new(gpu),
            overlay: Shapes::new(gpu),
            labels: Labels::new(gpu, text),
        }
    }

    fn canvas<'a>(&'a mut self, palette: &'a Palette, scale: f32, styling: Styling) -> Canvas<'a> {
        Canvas {
            shapes: &mut self.shapes,
            overlay: &mut self.overlay,
            heatmap: &mut self.heatmap,
            labels: &mut self.labels,
            palette,
            scale,
            styling,
            covered: None,
        }
    }

    fn begin(&mut self, gpu: &Gpu, area: Area) {
        self.shapes.begin(gpu, area);
        self.heatmap.begin();
        self.overlay.begin(gpu, area);
        self.labels.clear();
    }

    fn finish(&mut self, gpu: &Gpu, text: &mut Text, area: Area) {
        self.shapes.prepare(gpu);
        self.heatmap.prepare(gpu);
        self.overlay.prepare(gpu);
        self.labels.prepare(gpu, text, area);
    }

    /// Lays out one Meter in `area` and uploads it for [`MeterRenderer::render`].
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        text: &mut Text,
        area: Area,
        look: Look,
        meter: &MeterScene,
    ) {
        self.begin(gpu, area);
        let (scale, styling) = (look.scale, look.styling);
        // The Meter's own colour overrides go over the Theme's.
        let palette = look.palette.with(&meter.overrides);
        let mut c = self.canvas(&palette, scale, styling);
        let panel = c.colour(Role::Panel).faded(styling.background_opacity);
        c.shapes
            .rounded_rect(area, c.px(styling.corner_radius), panel);
        let inner = area.inset(c.px(10.0));
        match &meter.state {
            MeterState::Starting => {
                let dim = c.dim();
                c.text(
                    "Starting System Capture…",
                    inner.x,
                    inner.y,
                    13.0,
                    dim,
                    Align::Left,
                );
            }
            MeterState::Unavailable(reason) => {
                let (text_colour, dim) = (c.colour(Role::Text), c.dim());
                c.text(
                    "System Capture is unavailable",
                    inner.x,
                    inner.y,
                    13.0,
                    text_colour,
                    Align::Left,
                );
                let y = inner.y + c.px(20.0);
                c.text(reason, inner.x, y, 11.0, dim, Align::Left);
            }
            MeterState::WaitingFor(name) => {
                // Dimmed: the Meter has nothing to show until its Send Plugin is back.
                let dim = c.dim().faded(0.7);
                let text = MeterState::waiting_text(name);
                c.text(&text, inner.x, inner.y, 13.0, dim, Align::Left);
            }
            MeterState::NoSendPlugins => {
                let (text_colour, dim) = (c.colour(Role::Text), c.dim());
                c.text(
                    MeterState::NO_SEND_PLUGINS,
                    inner.x,
                    inner.y,
                    13.0,
                    text_colour,
                    Align::Left,
                );
                let y = inner.y + c.px(20.0);
                c.text(
                    MeterState::NO_SEND_PLUGINS_HINT,
                    inner.x,
                    y,
                    11.0,
                    dim,
                    Align::Left,
                );
            }
            MeterState::NotListening => {
                // A small button: a click anywhere on the Meter starts listening.
                let accent = c.colour(Role::Accent);
                let text_colour = c.colour(Role::Text);
                let button = Area {
                    x: inner.x,
                    y: inner.y,
                    width: c.px(116.0).min(inner.width),
                    height: c.px(24.0).min(inner.height),
                };
                c.shapes.rounded_rect(button, c.px(6.0), accent.faded(0.35));
                let text_y = button.y + (button.height - c.px(13.0)) / 2.0;
                c.text(
                    MeterState::START_LISTENING,
                    button.x + button.width / 2.0,
                    text_y,
                    12.0,
                    text_colour,
                    Align::Centre,
                );
            }
            MeterState::PickSendPlugin(items) => {
                let text_colour = c.colour(Role::Text);
                c.text(
                    MeterState::PICK_TITLE,
                    inner.x,
                    inner.y,
                    13.0,
                    text_colour,
                    Align::Left,
                );
                for item in items {
                    draw_item(&mut c, area, meter, item);
                }
            }
            MeterState::Live(view) => match view {
                MeterView::Waveform {
                    settings,
                    traces,
                    completed,
                    lag,
                } => waveform::draw(&mut c, inner, settings, traces, *completed, *lag),
                MeterView::Spectrum {
                    settings,
                    spectrum,
                    range,
                    cursor,
                    peak,
                    selecting,
                    zoom,
                    pointer,
                    held_peaks,
                } => spectrum::draw(
                    &mut c,
                    area,
                    inner,
                    settings,
                    spectrum,
                    *range,
                    spectrum::Marks {
                        cursor: cursor.as_ref(),
                        peak: peak.as_ref(),
                        held_peaks,
                        pointer: *pointer,
                    },
                    *selecting,
                    zoom.as_deref(),
                ),
                MeterView::Loudness { settings, display } => {
                    loudness::draw(&mut c, inner, settings, display)
                }
                MeterView::Stereometer {
                    settings,
                    readings,
                    points,
                } => stereometer::draw(&mut c, inner, settings, readings, points),
                MeterView::Cepstrum {
                    settings,
                    values,
                    quefrency_range,
                    pitch,
                    hover,
                } => cepstrum::draw(
                    &mut c,
                    inner,
                    settings,
                    values,
                    *quefrency_range,
                    pitch.as_ref(),
                    hover.as_ref(),
                ),
                MeterView::Spectrogram {
                    settings,
                    columns,
                    range,
                    lag,
                    selecting,
                    zoom,
                    ..
                } => spectrogram::draw(
                    &mut c,
                    inner,
                    settings,
                    columns,
                    *range,
                    *lag,
                    *selecting,
                    zoom.as_ref(),
                ),
                MeterView::PhaseScope { settings, scope } => {
                    phase_scope::draw(&mut c, inner, settings, scope)
                }
            },
        }
        if let Some(source) = &meter.source {
            let dim = c.dim();
            let size = c.px(6.0);
            let (x, y) = (area.right() - c.px(8.0), area.y + c.px(6.0));
            let width = if let Some(colour) = source.colour {
                c.shapes.rect(
                    Area {
                        x: x - size,
                        y: y + c.px(3.0),
                        width: size,
                        height: size,
                    },
                    colour,
                );
                size + c.px(4.0)
            } else {
                0.0
            };
            c.text(&source.name, x - width, y, 10.0, dim, Align::Right);
        }
        self.finish(gpu, text, area);
    }

    /// Lays out the notes along the bottom of a window.
    pub fn prepare_notes(
        &mut self,
        gpu: &Gpu,
        text: &mut Text,
        window: Area,
        look: Look,
        notes: &[Note],
    ) {
        self.begin(gpu, window);
        let mut c = self.canvas(look.palette, look.scale, look.styling);
        let line = c.px(24.0);
        for (i, note) in notes.iter().rev().enumerate() {
            let y = window.bottom() - line * (i + 1) as f32;
            let back = Area {
                x: window.x,
                y,
                width: window.width,
                height: line,
            };
            let (background, accent) = (c.colour(Role::Background), c.colour(Role::Accent));
            c.shapes.rect(back, background.faded(0.85));
            let x = window.x + window.width / 2.0;
            c.text(note.text(), x, y + c.px(4.0), 13.0, accent, Align::Centre);
        }
        self.finish(gpu, text, window);
    }

    pub fn render(&self, text: &Text, pass: &mut wgpu::RenderPass) {
        // The shapes set the Meter's scissor, which clips the rest too.
        self.shapes.render(pass);
        self.heatmap.render(pass);
        self.overlay.render(pass);
        self.labels.render(text, pass);
    }
}

/// `a` blended toward `b` by `t` (0 to 1), alpha too.
pub fn mix(a: Colour, b: Colour, t: f32) -> Colour {
    let channel = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Colour {
        r: channel(a.r, b.r),
        g: channel(a.g, b.g),
        b: channel(a.b, b.b),
        a: channel(a.a, b.a),
    }
}

/// Maps `value` from `range` onto `from`..`to` (either direction), clamped.
pub fn map(value: f32, range: (f32, f32), from: f32, to: f32) -> f32 {
    let t = ((value - range.0) / (range.1 - range.0)).clamp(0.0, 1.0);
    from + t * (to - from)
}

/// One row of a "Pick a Send Plugin" list: a colour swatch and the name,
/// where the core put it (the core hit-tests clicks against the same frame).
fn draw_item(c: &mut Canvas, area: Area, meter: &MeterScene, item: &SourceItem) {
    let top = (item.frame.y - meter.frame.y) / meter.frame.height;
    let height = item.frame.height / meter.frame.height;
    let row = Area {
        x: area.x,
        y: area.y + top * area.height,
        width: area.width,
        height: height * area.height,
    };
    let inner = row.inset(c.px(2.0));
    let swatch = c.px(10.0);
    let y = inner.y + (inner.height - swatch) / 2.0;
    let x = area.x + c.px(10.0);
    let colour = if item.pickable {
        item.colour
    } else {
        item.colour.faded(0.4)
    };
    c.shapes.rect(
        Area {
            x,
            y,
            width: swatch,
            height: swatch,
        },
        colour,
    );
    let text_colour = if item.pickable {
        c.colour(Role::Text)
    } else {
        c.dim()
    };
    let text_y = inner.y + (inner.height - c.px(13.0)) / 2.0;
    c.text(
        &item.label,
        x + swatch + c.px(8.0),
        text_y,
        12.0,
        text_colour,
        Align::Left,
    );
}
