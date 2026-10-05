//! The Spectrogram: the Spectrum over time, newest at the right edge,
//! log-frequency from the bottom up, each level a colour from the Theme's
//! Spectrum fill, line and accent up to its text colour at the top of the range.

use dasmeter_core::{Colour, Role, SpectrogramMeterSettings};

use super::labels::Align;
use super::shapes::Area;
use super::{Canvas, map};

/// How many colours the levels fall into: neighbouring rows of one colour
/// are drawn as one rectangle.
const STEPS: u8 = 48;
/// Levels below this step stay the panel's colour.
const FIRST_STEP: u8 = 2;

const FREQUENCY_MARKS: [(f32, &str); 8] = [
    (50.0, "50"),
    (100.0, "100"),
    (200.0, "200"),
    (500.0, "500"),
    (1_000.0, "1k"),
    (2_000.0, "2k"),
    (5_000.0, "5k"),
    (10_000.0, "10k"),
];

fn mix(a: Colour, b: Colour, t: f32) -> Colour {
    let channel = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Colour {
        r: channel(a.r, b.r),
        g: channel(a.g, b.g),
        b: channel(a.b, b.b),
        a: channel(a.a, b.a),
    }
}

/// The colour of each step, from the quietest (faint fill) to the loudest.
fn colours(c: &Canvas) -> Vec<Colour> {
    let stops = [
        (0.0, c.colour(Role::SpectrumFill).faded(0.15)),
        (0.35, c.colour(Role::SpectrumFill)),
        (0.65, c.colour(Role::SpectrumLine)),
        (0.88, c.colour(Role::Accent)),
        (1.0, c.colour(Role::Text)),
    ];
    (0..=STEPS)
        .map(|step| {
            let t = f32::from(step) / f32::from(STEPS);
            let upper = stops
                .iter()
                .position(|(at, _)| *at >= t)
                .unwrap_or(stops.len() - 1);
            let lower = upper.saturating_sub(1);
            let ((a_at, a), (b_at, b)) = (stops[lower], stops[upper]);
            let span = (b_at - a_at).max(f32::EPSILON);
            mix(a, b, ((t - a_at) / span).clamp(0.0, 1.0))
        })
        .collect()
}

pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &SpectrogramMeterSettings,
    columns: &[Vec<u8>],
    range: (f32, f32),
) {
    let (grid, dim) = (c.colour(Role::Grid), c.dim());
    let scale_width = if settings.show_scale { c.px(30.0) } else { 0.0 };
    let plot = Area {
        x: area.x + scale_width,
        width: (area.width - scale_width).max(1.0),
        ..area
    };
    let (low, high) = (range.0.max(1.0).ln(), range.1.max(1.0).ln());
    let y_of = |f: f32| map(f.max(1.0).ln(), (low, high), plot.bottom(), plot.y);

    // The picture: one column per entry, newest against the right edge.
    let palette = colours(c);
    let column_width = plot.width / dasmeter_analysis::spectrogram::COLUMNS as f32;
    for (age, column) in columns.iter().rev().enumerate() {
        let right = plot.right() - age as f32 * column_width;
        let x = right - column_width;
        if right <= plot.x {
            break;
        }
        let rows = column.len().max(1);
        let row_height = plot.height / rows as f32;
        let step_of = |level: u8| ((u32::from(level) * u32::from(STEPS) + 127) / 255) as u8;
        // Runs of rows in one colour, from the bottom up.
        let mut row = 0;
        while row < column.len() {
            let step = step_of(column[row]);
            let start = row;
            while row < column.len() && step_of(column[row]) == step {
                row += 1;
            }
            if step < FIRST_STEP {
                continue;
            }
            let top = plot.bottom() - row as f32 * row_height;
            let bottom = plot.bottom() - start as f32 * row_height;
            c.shapes.rect(
                Area {
                    x: x.max(plot.x),
                    y: top,
                    // A hair wider, so columns never leave seams between them.
                    width: (right - x.max(plot.x)) + c.px(0.5),
                    height: bottom - top,
                },
                palette[usize::from(step)],
            );
        }
    }

    if !settings.show_scale {
        return;
    }
    // The frequency scale: faint lines across, labels right-aligned on the left.
    let thin = c.px(1.0).max(1.0);
    let mut last_label: Option<f32> = None;
    for (frequency, name) in FREQUENCY_MARKS {
        if frequency < range.0 || frequency > range.1 {
            continue;
        }
        let y = y_of(frequency);
        c.shapes.rect(
            Area {
                y: y - thin / 2.0,
                height: thin,
                ..plot
            },
            grid.faded(0.35),
        );
        let clear = last_label.is_none_or(|last| last - y > c.px(12.0));
        if clear && y - c.px(6.0) > area.y && y + c.px(6.0) < area.bottom() {
            c.text(
                name,
                plot.x - c.px(4.0),
                y - c.px(6.0),
                9.0,
                dim,
                Align::Right,
            );
            last_label = Some(y);
        }
    }
}
