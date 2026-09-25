//! The Waveform: columns scrolling right to left, newest at the right edge,
//! one lane per trace (L or M above, R or S below), coloured by band energy.

use dasmeter_analysis::{ChannelView, WaveformColumn};
use dasmeter_core::{Colour, Role, WaveformColouring, WaveformMeterSettings};

use super::Canvas;
use super::labels::Align;
use super::shapes::Area;

pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &WaveformMeterSettings,
    traces: &[Vec<WaveformColumn>],
) {
    let names: &[&str] = match settings.analysis.channel_view {
        ChannelView::Mono => &[""],
        ChannelView::LeftRight => &["L", "R"],
        ChannelView::MidSide => &["M", "S"],
    };
    let lanes = traces.len().max(1);
    let lane_height = area.height / lanes as f32;
    let capacity = settings.analysis.columns().max(1);
    let column_width = area.width / capacity as f32;
    let bands = [
        c.colour(Role::WaveformLow),
        c.colour(Role::WaveformMid),
        c.colour(Role::WaveformHigh),
    ];
    let (grid, dim) = (c.colour(Role::Grid), c.dim());
    let thin = c.px(1.0).max(1.0);

    for (lane, columns) in traces.iter().enumerate() {
        let top = area.y + lane_height * lane as f32;
        let centre = top + lane_height / 2.0;
        let half = lane_height / 2.0 - c.px(2.0);
        let axis = Area {
            y: centre - thin / 2.0,
            height: thin,
            ..area
        };
        c.shapes.rect(axis, grid);
        if let Some(name) = names.get(lane).filter(|n| !n.is_empty()) {
            c.text(name, area.x, top, 10.0, dim, Align::Left);
        }

        // Columns narrower than a pixel are merged, so a 30 s span costs no
        // more to draw than the Meter is wide.
        let pixels = area.width.ceil().max(1.0) as usize;
        let per_pixel = (1.0 / column_width).max(1.0);
        let merged = merge(columns, capacity, per_pixel, pixels);
        let width = column_width.max(1.0);
        for (right_edge, column) in merged {
            let x = area.right() - right_edge * column_width;
            let y = |sample: f32| centre - settings.analysis.height(sample) * half;
            match settings.colouring {
                WaveformColouring::Solid => {
                    let span = envelope(x, width, y(column.max), y(column.min));
                    c.shapes.rect(span, bands[1]);
                }
                WaveformColouring::Rgb => {
                    let span = envelope(x, width, y(column.max), y(column.min));
                    c.shapes
                        .rect(span, mix(bands, [column.low, column.mid, column.high]));
                }
                WaveformColouring::Rekordbox => {
                    // Each band's height follows its own level, low widest underneath.
                    let peak = column.max.max(-column.min);
                    let total = column.low + column.mid + column.high;
                    for (band, &colour) in [column.low, column.mid, column.high]
                        .into_iter()
                        .zip(&bands)
                    {
                        if total <= 0.0 {
                            break;
                        }
                        let level = peak * (band / total).sqrt();
                        let span = envelope(x, width, y(level), y(-level));
                        c.shapes.rect(span, colour);
                    }
                }
            }
        }
    }
}

/// A column's vertical span, at least a pixel high so silence still shows a line.
fn envelope(x: f32, width: f32, top: f32, bottom: f32) -> Area {
    let height = (bottom - top).max(1.0);
    Area {
        x,
        y: (top + bottom) / 2.0 - height / 2.0,
        width,
        height,
    }
}

/// Merges columns into groups of `per_pixel`, counted from the newest. Returns
/// each group's distance from the right edge (in columns) and its merged column.
fn merge(
    columns: &[WaveformColumn],
    capacity: usize,
    per_pixel: f32,
    pixels: usize,
) -> Vec<(f32, WaveformColumn)> {
    let shown = columns.len().min(capacity);
    let newest_first = columns.iter().rev().take(shown).enumerate();
    let mut merged: Vec<(f32, WaveformColumn)> = Vec::with_capacity(pixels.min(shown));
    let mut group = usize::MAX;
    for (age, column) in newest_first {
        let this = (age as f32 / per_pixel) as usize;
        if this != group {
            group = this;
            let right_edge = (this as f32 * per_pixel) + per_pixel.max(1.0);
            merged.push((right_edge, *column));
        } else if let Some((_, m)) = merged.last_mut() {
            m.min = m.min.min(column.min);
            m.max = m.max.max(column.max);
            m.low = m.low.max(column.low);
            m.mid = m.mid.max(column.mid);
            m.high = m.high.max(column.high);
        }
    }
    merged
}

/// The band colours mixed by energy, at full brightness.
fn mix(colours: [Colour; 3], energies: [f32; 3]) -> Colour {
    let total: f32 = energies.iter().sum();
    if total <= 0.0 {
        return colours[1];
    }
    let channel = |pick: fn(&Colour) -> u8| {
        colours
            .iter()
            .zip(energies)
            .map(|(colour, energy)| f32::from(pick(colour)) * energy / total)
            .sum::<f32>()
    };
    let (r, g, b) = (channel(|c| c.r), channel(|c| c.g), channel(|c| c.b));
    let brightest = r.max(g).max(b).max(1.0);
    let scale = 255.0 / brightest;
    Colour::rgb(
        (r * scale).min(255.0) as u8,
        (g * scale).min(255.0) as u8,
        (b * scale).min(255.0) as u8,
    )
}
