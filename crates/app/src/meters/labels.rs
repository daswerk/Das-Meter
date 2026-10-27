//! A Meter's text: short labels placed in physical pixels, shaped with glyphon.
//! Shaped text is kept by its text, size and weight, and reused next frame
//! wherever it's drawn, so only text that changed is shaped again.

use std::collections::HashMap;

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
    buffer: Buffer,
    width: f32,
}

/// What shaping depends on: the text, its size (as bits) and its weight.
type Key = (String, u32, bool);

fn key(label: &Label) -> Key {
    (
        label.text.clone(),
        label.style.size.to_bits(),
        label.style.bold,
    )
}

pub struct Labels {
    renderer: TextRenderer,
    labels: Vec<Label>,
    /// The text shaped for the last frame.
    shaped: HashMap<Key, Shaped>,
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
            shaped: HashMap::new(),
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
        // Kept from the last frame where it's the same, shaped where it's new;
        // what this frame doesn't show is let go.
        let mut last = std::mem::take(&mut self.shaped);
        for label in &self.labels {
            let key = key(label);
            if self.shaped.contains_key(&key) {
                continue;
            }
            let shaped = last
                .remove(&key)
                .unwrap_or_else(|| shape(text, &label.text, label.style));
            self.shaped.insert(key, shaped);
        }

        let bounds = TextBounds {
            left: clip.x as i32,
            top: clip.y as i32,
            right: clip.right() as i32,
            bottom: clip.bottom() as i32,
        };
        let areas = self.labels.iter().map(|label| {
            let shaped = &self.shaped[&key(label)];
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

/// `text` shaped at `style`'s size and weight.
fn shape(text: &mut Text, line: &str, style: Style) -> Shaped {
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
        line,
        &Attrs::new().family(Family::Monospace).weight(weight),
        Shaping::Advanced,
        None,
    );
    buffer.shape_until_scroll(&mut text.font_system, false);
    let width = buffer
        .layout_runs()
        .map(|run| run.line_w)
        .fold(0.0, f32::max);
    Shaped { buffer, width }
}
