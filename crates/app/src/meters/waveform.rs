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
    completed: u64,
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
        let merged = merge(columns, completed, capacity, per_pixel, pixels);
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

/// Merges columns into groups of `per_pixel` by their number in time (not by
/// age), so a group keeps the same columns, and so the same height, while it
/// scrolls; grouping by age moved every boundary with each new column and made
/// the whole Waveform shimmer. Groups move a whole group (a pixel) at a time.
/// Returns each group's distance from the right edge (in columns) and its
/// merged column. `completed` is the newest column's number plus one.
fn merge(
    columns: &[WaveformColumn],
    completed: u64,
    capacity: usize,
    per_pixel: f32,
    pixels: usize,
) -> Vec<(f32, WaveformColumn)> {
    let shown = columns.len().min(capacity);
    if shown == 0 || completed == 0 {
        return Vec::new();
    }
    let per_pixel = f64::from(per_pixel.max(1.0));
    let group_of = |number: u64| (number as f64 / per_pixel).floor() as u64;
    let newest_group = group_of(completed - 1);
    let mut merged: Vec<(f32, WaveformColumn)> = Vec::with_capacity(pixels.min(shown));
    let mut group = u64::MAX;
    for (age, column) in columns.iter().rev().take(shown).enumerate() {
        let number = completed - 1 - age as u64;
        let this = group_of(number);
        if this != group {
            group = this;
            let right_edge = ((newest_group - this + 1) as f64 * per_pixel) as f32;
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

/// The band colours mixed by energy, as bright as the brightest band colour
/// (full brightness on Dark; darker on a light Theme so it stays visible).
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
    let ceiling = colours
        .iter()
        .map(|c| f32::from(c.r.max(c.g).max(c.b)))
        .fold(1.0, f32::max);
    let scale = ceiling / brightest;
    Colour::rgb(
        (r * scale).min(255.0) as u8,
        (g * scale).min(255.0) as u8,
        (b * scale).min(255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(n: u64) -> WaveformColumn {
        // Each column's height is its number, so merged content is easy to follow.
        let x = (n % 97) as f32 / 97.0;
        WaveformColumn {
            min: -x,
            max: x,
            low: x,
            mid: x,
            high: x,
        }
    }

    /// The last `capacity` columns when `completed` have been made.
    fn history(completed: u64, capacity: usize) -> Vec<WaveformColumn> {
        let first = completed.saturating_sub(capacity as u64);
        (first..completed).map(column).collect()
    }

    /// The merged groups by their number in time, without the newest (still
    /// filling) and the oldest (cut off at the left edge).
    fn by_group(
        completed: u64,
        capacity: usize,
        per_pixel: f32,
    ) -> std::collections::BTreeMap<u64, (u32, u32)> {
        let merged = merge(
            &history(completed, capacity),
            completed,
            capacity,
            per_pixel,
            437,
        );
        let newest_group = ((completed - 1) as f64 / f64::from(per_pixel)).floor() as u64;
        merged[1..merged.len() - 1]
            .iter()
            .map(|(right_edge, c)| {
                let back = (f64::from(*right_edge) / f64::from(per_pixel)).round() as u64;
                (newest_group + 1 - back, (c.max.to_bits(), c.min.to_bits()))
            })
            .collect()
    }

    #[test]
    fn merged_pixels_keep_their_content_as_they_scroll() {
        let (capacity, per_pixel) = (800, 800.0 / 437.0);
        let before = by_group(5_000, capacity, per_pixel);
        for step in 1..=5u64 {
            let after = by_group(5_000 + step, capacity, per_pixel);
            let shared: Vec<_> = before.keys().filter(|g| after.contains_key(g)).collect();
            assert!(shared.len() > 400, "step {step}: {} shared", shared.len());
            for g in shared {
                assert_eq!(before[g], after[g], "step {step}: group {g} changed");
            }
        }
    }

    #[test]
    fn groups_scroll_a_whole_group_at_a_time() {
        let (capacity, per_pixel) = (800, 2.0);
        for completed in [4_000u64, 4_001, 4_002] {
            let merged = merge(
                &history(completed, capacity),
                completed,
                capacity,
                per_pixel,
                400,
            );
            for (right_edge, _) in merged {
                let groups = right_edge / per_pixel;
                assert_eq!(groups, groups.round(), "{completed}: {right_edge}");
            }
        }
    }

    #[test]
    fn one_column_per_pixel_or_wider_merges_nothing() {
        let merged = merge(&history(300, 200), 300, 200, 1.0, 400);
        assert_eq!(merged.len(), 200);
        assert_eq!(merged[0].0, 1.0, "the newest touches the right edge");
    }
}
