//! Meter renderers: one per Meter, each its own small shape buffer and text,
//! drawing that Meter's part of the scene with the palette's colour roles.

mod labels;
mod loudness;
mod shapes;
mod spectrum;
mod stereometer;
mod waveform;

use dasmeter_core::{Colour, MeterState, MeterView, Note, Palette, Role};

use crate::gpu::{Gpu, Text};
use labels::{Align, Labels, Style};
pub use shapes::Area;
use shapes::Shapes;

/// What a Meter's drawing code draws with: shapes, labels, colours and the scale factor.
pub struct Canvas<'a> {
    pub shapes: &'a mut Shapes,
    labels: &'a mut Labels,
    palette: &'a Palette,
    /// Physical pixels per logical pixel.
    pub scale: f32,
}

impl Canvas<'_> {
    /// Logical pixels to physical ones.
    pub fn px(&self, logical: f32) -> f32 {
        logical * self.scale
    }

    pub fn colour(&self, role: Role) -> Colour {
        self.palette[role]
    }

    /// Secondary text: labels, units, scales.
    pub fn dim(&self) -> Colour {
        self.palette[Role::Text].faded(0.55)
    }

    /// Text whose top edge is at `y`, `size` logical pixels high.
    pub fn text(&mut self, text: &str, x: f32, y: f32, size: f32, colour: Colour, align: Align) {
        let style = Style {
            size: self.px(size),
            colour,
            bold: false,
            align,
        };
        self.labels.add(text, x, y, style);
    }

    pub fn bold(&mut self, text: &str, x: f32, y: f32, size: f32, colour: Colour, align: Align) {
        let style = Style {
            size: self.px(size),
            colour,
            bold: true,
            align,
        };
        self.labels.add(text, x, y, style);
    }
}

/// Draws one Meter (or the notes over a window) into its area.
pub struct MeterRenderer {
    shapes: Shapes,
    labels: Labels,
}

impl MeterRenderer {
    pub fn new(gpu: &Gpu, text: &mut Text) -> MeterRenderer {
        MeterRenderer {
            shapes: Shapes::new(gpu),
            labels: Labels::new(gpu, text),
        }
    }

    fn canvas<'a>(&'a mut self, palette: &'a Palette, scale: f32) -> Canvas<'a> {
        Canvas {
            shapes: &mut self.shapes,
            labels: &mut self.labels,
            palette,
            scale,
        }
    }

    fn begin(&mut self, gpu: &Gpu, area: Area) {
        self.shapes.begin(gpu, area);
        self.labels.clear();
    }

    fn finish(&mut self, gpu: &Gpu, text: &mut Text, area: Area) {
        self.shapes.prepare(gpu);
        self.labels.prepare(gpu, text, area);
    }

    /// Lays out one Meter in `area` and uploads it for [`MeterRenderer::render`].
    pub fn prepare(
        &mut self,
        gpu: &Gpu,
        text: &mut Text,
        area: Area,
        scale: f32,
        palette: &Palette,
        state: &MeterState,
    ) {
        self.begin(gpu, area);
        let mut c = self.canvas(palette, scale);
        let panel = c.colour(Role::Panel);
        c.shapes.rect(area, panel);
        let inner = area.inset(c.px(10.0));
        match state {
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
            MeterState::Live(view) => match view {
                MeterView::Waveform { settings, traces } => {
                    waveform::draw(&mut c, inner, settings, traces)
                }
                MeterView::Spectrum {
                    settings,
                    spectrum,
                    range,
                    cursor,
                } => spectrum::draw(&mut c, inner, settings, spectrum, *range, cursor.as_ref()),
                MeterView::Loudness { settings, display } => {
                    loudness::draw(&mut c, inner, settings, display)
                }
                MeterView::Stereometer {
                    settings,
                    readings,
                    points,
                } => stereometer::draw(&mut c, inner, settings, readings, points),
            },
        }
        self.finish(gpu, text, area);
    }

    /// Lays out the notes along the bottom of a window.
    pub fn prepare_notes(
        &mut self,
        gpu: &Gpu,
        text: &mut Text,
        window: Area,
        scale: f32,
        palette: &Palette,
        notes: &[Note],
    ) {
        self.begin(gpu, window);
        let mut c = self.canvas(palette, scale);
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
        self.shapes.render(pass);
        self.labels.render(text, pass);
    }
}

/// Maps `value` from `range` onto `from`..`to` (either direction), clamped.
pub fn map(value: f32, range: (f32, f32), from: f32, to: f32) -> f32 {
    let t = ((value - range.0) / (range.1 - range.0)).clamp(0.0, 1.0);
    from + t * (to - from)
}
