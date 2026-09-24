//! A Meter's text: short labels placed in physical pixels, shaped with glyphon.
//! Shaped buffers are reused while a label's text and style stay the same.

use dasmeter_core::Colour;
use glyphon::{
    Attrs, Buffer, Family, Metrics, Shaping, TextArea, TextBounds, TextRenderer, Weight,
};

use super::shapes::Area;
use crate::gpu::{Gpu, Text, text_colour};

/// Where a label's position anchors it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Centre,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Style {
    pub size: f32,
    pub colour: Colour,
    pub bold: bool,
    pub align: Align,
}

struct Label {
    text: String,
    x: f32,
    y: f32,
    style: Style,
}

struct Shaped {
    text: String,
    size: f32,
    bold: bool,
    buffer: Buffer,
    width: f32,
}

pub struct Labels {
    renderer: TextRenderer,
    labels: Vec<Label>,
    shaped: Vec<Shaped>,
}

impl Labels {
    pub fn new(gpu: &Gpu, text: &mut Text) -> Labels {
        Labels {
            renderer: TextRenderer::new(
                &mut text.atlas,
                &gpu.device,
                wgpu::MultisampleState::default(),
                None,
            ),
            labels: Vec::new(),
            shaped: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.labels.clear();
    }

    /// A line of text whose top edge is at `y`, anchored at `x` per `style.align`.
    pub fn add(&mut self, text: &str, x: f32, y: f32, style: Style) {
        self.labels.push(Label {
            text: text.to_owned(),
            x,
            y,
            style,
        });
    }

    /// Shapes and uploads the labels, clipped to `clip`.
    pub fn prepare(&mut self, gpu: &Gpu, text: &mut Text, clip: Area) {
        self.shaped.truncate(self.labels.len());
        for (i, label) in self.labels.iter().enumerate() {
            let style = label.style;
            let fresh = self.shaped.get(i).is_some_and(|s| {
                s.text == label.text && s.size == style.size && s.bold == style.bold
            });
            if fresh {
                continue;
            }
            let mut buffer = Buffer::new(
                &mut text.font_system,
                Metrics::new(style.size, style.size * 1.25),
            );
            let weight = if style.bold {
                Weight::BOLD
            } else {
                Weight::NORMAL
            };
            buffer.set_size(None, None);
            buffer.set_text(
                &label.text,
                &Attrs::new().family(Family::Monospace).weight(weight),
                Shaping::Advanced,
                None,
            );
            buffer.shape_until_scroll(&mut text.font_system, false);
            let width = buffer
                .layout_runs()
                .map(|run| run.line_w)
                .fold(0.0, f32::max);
            let shaped = Shaped {
                text: label.text.clone(),
                size: style.size,
                bold: style.bold,
                buffer,
                width,
            };
            if i < self.shaped.len() {
                self.shaped[i] = shaped;
            } else {
                self.shaped.push(shaped);
            }
        }

        let bounds = TextBounds {
            left: clip.x as i32,
            top: clip.y as i32,
            right: clip.right() as i32,
            bottom: clip.bottom() as i32,
        };
        let areas = self.labels.iter().zip(&self.shaped).map(|(label, shaped)| {
            let left = match label.style.align {
                Align::Left => label.x,
                Align::Centre => label.x - shaped.width / 2.0,
                Align::Right => label.x - shaped.width,
            };
            TextArea {
                buffer: &shaped.buffer,
                left: left.round(),
                top: label.y.round(),
                scale: 1.0,
                bounds,
                default_color: text_colour(label.style.colour),
                custom_glyphs: &[],
            }
        });
        if let Err(error) = self.renderer.prepare(
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
        if let Err(error) = self.renderer.render(&text.atlas, &text.viewport, pass) {
            eprintln!("text: {error}");
        }
    }
}
