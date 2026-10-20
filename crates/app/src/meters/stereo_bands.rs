//! Stereo Bands: a column per octave band, its band correlation as a bar up
//! (toward +1, the positive colour) or down (toward −1, the negative colour)
//! from the middle, and under it, when shown, its width as a bar from the
//! bottom (0, mono) to the top (1, opposite polarity), half-way meaning
//! unrelated channels. Silent bands are left empty.

use dasmeter_core::{BandDisplay, Role, StereoBandsMeterSettings};

use super::Canvas;
use super::labels::Align;
use super::shapes::Area;

/// Shorter than this (logical px), the width row is left out.
const WIDTH_ROW_MIN_HEIGHT: f32 = 120.0;

fn label(centre: f32) -> String {
    if centre >= 1_000.0 {
        format!("{}k", (centre / 1_000.0).round())
    } else {
        format!("{}", centre.round())
    }
}

pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &StereoBandsMeterSettings,
    bands: &[BandDisplay],
) {
    if bands.is_empty() {
        return;
    }
    let dim = c.dim();
    let label_height = c.px(13.0) * c.styling.text_scale;
    let (body, labels) = area.split_bottom(label_height);
    let with_width = settings.show_width && area.height >= c.px(WIDTH_ROW_MIN_HEIGHT);
    let (correlation_row, width_row) = if with_width {
        let (top, bottom) = body.split_top(body.height * 0.62);
        (top, Some(bottom.split_top(c.px(8.0)).1))
    } else {
        (body, None)
    };
    let scale_width = c.px(26.0);
    let columns = Area {
        x: area.x + scale_width,
        width: area.width - scale_width,
        ..area
    };
    let gap = c.px(4.0).min(columns.width / bands.len() as f32 / 4.0);
    let width = (columns.width - gap * (bands.len() as f32 - 1.0)) / bands.len() as f32;
    let column_x = |i: usize| columns.x + i as f32 * (width + gap);

    // The correlation row: +1 at the top, −1 at the bottom, 0 across the middle.
    let grid = c.colour(Role::Grid);
    let mid = correlation_row.y + correlation_row.height / 2.0;
    let y_of = |v: f32| mid - v.clamp(-1.0, 1.0) * correlation_row.height / 2.0;
    for (value, text) in [(1.0, "+1"), (0.0, "0"), (-1.0, "−1")] {
        let y = y_of(value);
        c.grid_h(columns_row(columns, correlation_row), y, grid, value == 0.0);
        c.text(
            text,
            area.x + scale_width - c.px(4.0),
            (y - c.px(6.0)).clamp(correlation_row.y, correlation_row.bottom() - c.px(12.0)),
            9.0,
            dim,
            Align::Right,
        );
    }
    let (positive, negative) = (
        c.colour(Role::CorrelationPositive),
        c.colour(Role::CorrelationNegative),
    );
    let trace = c.colour(Role::StereometerTrace);
    for (i, band) in bands.iter().enumerate() {
        let x = column_x(i);
        c.text(
            &label(band.centre),
            x + width / 2.0,
            labels.y + c.px(1.0),
            9.0,
            dim,
            Align::Centre,
        );
        if let Some(value) = band.correlation {
            let (top, bottom) = (y_of(value.max(0.0)), y_of(value.min(0.0)));
            let colour = if value >= 0.0 { positive } else { negative };
            c.shapes.rect(
                Area {
                    x,
                    y: top,
                    width,
                    height: (bottom - top).max(c.px(2.0)),
                },
                colour,
            );
            if correlation_row.height > c.px(60.0) && width > c.px(26.0) {
                let text_y = if value >= 0.0 {
                    (top - c.px(13.0)).max(correlation_row.y)
                } else {
                    (bottom + c.px(2.0)).min(correlation_row.bottom() - c.px(12.0))
                };
                c.text(
                    &format!("{value:+.2}"),
                    x + width / 2.0,
                    text_y,
                    9.0,
                    dim,
                    Align::Centre,
                );
            }
        }
        if let (Some(row), Some(value)) = (width_row, band.width) {
            c.shapes.rect(Area { x, width, ..row }, grid.faded(0.25));
            let height = row.height * value.clamp(0.0, 1.0);
            c.shapes.rect(
                Area {
                    x,
                    y: row.bottom() - height,
                    width,
                    height: height.max(c.px(1.0)),
                },
                trace.faded(0.85),
            );
        }
    }
    if let Some(row) = width_row {
        c.text(
            "Width",
            area.x + scale_width - c.px(4.0),
            row.y,
            9.0,
            dim,
            Align::Right,
        );
        // Half-way: unrelated channels.
        let y = row.y + row.height / 2.0;
        c.grid_h(columns_row(columns, row), y, grid, false);
    }
}

/// The columns' span across `row`.
fn columns_row(columns: Area, row: Area) -> Area {
    Area {
        y: row.y,
        height: row.height,
        ..columns
    }
}
