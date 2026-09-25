//! The Stereometer: Polar (a half circle, mono straight up) or Lissajous, the
//! points fading with age as dots or lines, the correlation bar below, an
//! optional balance bar and a mono indication.

use std::f32::consts::{FRAC_PI_4, PI};

use dasmeter_analysis::{StereoReadings, StereoView};
use dasmeter_core::{Role, StereoDrawing, StereometerMeterSettings};

use super::labels::Align;
use super::shapes::Area;
use super::{Canvas, map};

/// Segments used to draw the scope's circle.
const ARC_SEGMENTS: usize = 64;

pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &StereometerMeterSettings,
    readings: &StereoReadings,
    points: &[[f32; 2]],
) {
    let bar_height = c.px(26.0);
    let bars = if settings.show_balance { 2.0 } else { 1.0 };
    let (scope, below) = area.split_bottom(bar_height * bars);
    let (grid, dim) = (c.colour(Role::Grid), c.dim());
    let thin = c.px(1.0).max(1.0);

    // Scope geometry: centre and radius in pixels, and where a point lands.
    let (centre, radius) = match settings.view {
        StereoView::Polar => {
            let radius = (scope.width / 2.0).min(scope.height - c.px(14.0)).max(1.0);
            let centre = [scope.x + scope.width / 2.0, scope.bottom() - c.px(4.0)];
            (centre, radius)
        }
        StereoView::Lissajous => {
            let radius = (scope.width.min(scope.height) / 2.0 - c.px(4.0)).max(1.0);
            let centre = [scope.x + scope.width / 2.0, scope.y + scope.height / 2.0];
            (centre, radius)
        }
    };
    let place = |[x, y]: [f32; 2]| [centre[0] + x * radius, centre[1] - y * radius];

    // Outline and axes: M straight up, L and R at 45°.
    let (start, sweep) = match settings.view {
        StereoView::Polar => (0.0, PI),
        StereoView::Lissajous => (0.0, 2.0 * PI),
    };
    let arc: Vec<[f32; 2]> = (0..=ARC_SEGMENTS)
        .map(|i| {
            let angle = start + sweep * i as f32 / ARC_SEGMENTS as f32;
            place([angle.cos(), angle.sin()])
        })
        .collect();
    c.shapes.polyline(&arc, thin, grid);
    let axes: &[(f32, &str)] = match settings.view {
        StereoView::Polar => &[(PI / 2.0, "M"), (PI - FRAC_PI_4, "L"), (FRAC_PI_4, "R")],
        StereoView::Lissajous => &[
            (PI / 2.0, "M"),
            (PI - FRAC_PI_4, "L"),
            (FRAC_PI_4, "R"),
            (0.0, "+S"),
            (PI, "−S"),
        ],
    };
    for &(angle, name) in axes {
        let end = [angle.cos(), angle.sin()];
        let from = match settings.view {
            StereoView::Polar => [0.0, 0.0],
            StereoView::Lissajous => [-end[0], -end[1]],
        };
        c.shapes
            .line(place(from), place(end), thin, grid.faded(0.7));
        let [x, y] = place([end[0] * 1.06, end[1] * 1.06]);
        c.text(name, x, y - c.px(6.0), 9.0, dim, Align::Centre);
    }

    // Points, oldest faintest.
    let trace = c.colour(Role::StereometerTrace);
    let count = points.len().max(1) as f32;
    let fade = |i: usize| 0.12 + 0.88 * (i as f32 + 1.0) / count;
    match settings.drawing {
        StereoDrawing::Dots => {
            let size = c.px(1.5);
            for (i, &point) in points.iter().enumerate() {
                let [x, y] = place(point);
                let dot = Area {
                    x: x - size / 2.0,
                    y: y - size / 2.0,
                    width: size,
                    height: size,
                };
                c.shapes.rect(dot, trace.faded(fade(i)));
            }
        }
        StereoDrawing::Lines => {
            let width = c.stroke(1.0);
            for (i, pair) in points.windows(2).enumerate() {
                c.shapes.line(
                    place(pair[0]),
                    place(pair[1]),
                    width,
                    trace.faded(fade(i + 1)),
                );
            }
        }
    }
    if readings.mono {
        let text = c.colour(Role::Text);
        c.text("mono", scope.right(), scope.y, 10.0, text, Align::Right);
    }

    // Correlation: −1 to +1, filled from 0, negative-coloured below the threshold.
    let (correlation_area, balance_area) = below.split_top(bar_height);
    let colour = if readings.correlation < settings.correlation_threshold {
        c.colour(Role::CorrelationNegative)
    } else {
        c.colour(Role::CorrelationPositive)
    };
    let colour = if readings.no_signal {
        colour.faded(0.3)
    } else {
        colour
    };
    bar(
        c,
        correlation_area,
        readings.correlation,
        colour,
        ["−1", "0", "+1"],
    );
    // Shape cue (High contrast): correlation below the threshold is marked
    // with "!" by the bar, so it doesn't rely on colour alone.
    let warn = readings.correlation < settings.correlation_threshold && !readings.no_signal;
    if c.styling.shape_cues && warn {
        let text = c.colour(Role::Text);
        let y = correlation_area.y;
        c.bold("!", correlation_area.x, y, 12.0, text, Align::Left);
    }
    if settings.show_balance {
        let accent = c.colour(Role::Accent);
        bar(c, balance_area, readings.balance, accent, ["L", "C", "R"]);
    }
}

/// A horizontal bar from −1 to +1 with `value` filled from the centre and a marker.
fn bar(c: &mut Canvas, area: Area, value: f32, colour: dasmeter_core::Colour, labels: [&str; 3]) {
    let dim = c.dim();
    let track_height = c.px(8.0);
    let track = Area {
        x: area.x + c.px(14.0),
        y: area.y + c.px(4.0),
        width: (area.width - c.px(28.0)).max(1.0),
        height: track_height,
    };
    let background = c.colour(Role::Background);
    c.shapes.rect(track, background);
    let x_of = |v: f32| map(v, (-1.0, 1.0), track.x, track.right());
    let (zero, at) = (x_of(0.0), x_of(value));
    let fill = Area {
        x: zero.min(at),
        width: (at - zero).abs(),
        ..track
    };
    c.shapes.rect(fill, colour.faded(0.6));
    let marker = Area {
        x: at - c.px(1.0),
        y: track.y - c.px(2.0),
        width: c.px(2.0),
        height: track_height + c.px(4.0),
    };
    c.shapes.rect(marker, colour);
    let grid = c.colour(Role::Grid);
    c.shapes.rect(
        Area {
            x: zero - c.px(0.5),
            width: c.px(1.0),
            ..track
        },
        grid,
    );
    let y = track.bottom() + c.px(1.0);
    c.text(labels[0], track.x, y, 9.0, dim, Align::Left);
    c.text(labels[1], zero, y, 9.0, dim, Align::Centre);
    c.text(labels[2], track.right(), y, 9.0, dim, Align::Right);
}
