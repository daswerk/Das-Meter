//! The Spectrogram: the Spectrum over time, newest at the right edge,
//! log-frequency from the bottom up, each level a colour from the Theme's
//! Spectrum fill, line and accent up to its text colour at the top of the range.

use dasmeter_core::{Colour, Role, SpectrogramMeterSettings};

use super::labels::Align;
use super::shapes::Area;
use super::{Canvas, map, mix};

/// Levels up to this one stay the panel's colour; the colours fade in over
/// the next [`FADE_IN`] levels, so quiet parts have no hard edge.
const FIRST_LEVEL: usize = 8;
const FADE_IN: usize = 16;

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

/// The colour of each level, from the quietest (faint fill) to the loudest.
fn colours(c: &Canvas) -> [Colour; 256] {
    let stops = [
        (0.0, c.colour(Role::SpectrumFill).faded(0.15)),
        (0.35, c.colour(Role::SpectrumFill)),
        (0.65, c.colour(Role::SpectrumLine)),
        (0.88, c.colour(Role::Accent)),
        (1.0, c.colour(Role::Text)),
    ];
    std::array::from_fn(|level| {
        let t = level as f32 / 255.0;
        let upper = stops
            .iter()
            .position(|(at, _)| *at >= t)
            .unwrap_or(stops.len() - 1);
        let lower = upper.saturating_sub(1);
        let ((a_at, a), (b_at, b)) = (stops[lower], stops[upper]);
        let span = (b_at - a_at).max(f32::EPSILON);
        let colour = mix(a, b, ((t - a_at) / span).clamp(0.0, 1.0));
        let fade = (level.saturating_sub(FIRST_LEVEL) as f32 / FADE_IN as f32).min(1.0);
        Colour {
            a: (f32::from(colour.a) * fade).round() as u8,
            ..colour
        }
    })
}

pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &SpectrogramMeterSettings,
    columns: &[Vec<u8>],
    range: (f32, f32),
    lag: f32,
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

    // The picture, on the GPU: newest against the right edge, scrolling
    // smoothly by the columns still to come in.
    let palette = colours(c);
    let rows = columns.first().map_or(0, Vec::len);
    c.heatmap.draw(
        plot,
        columns,
        dasmeter_analysis::spectrogram::COLUMNS,
        rows,
        lag,
        palette,
    );

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
        c.overlay.rect(
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
