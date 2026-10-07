//! The Stereometer: Polar (a half circle, mono straight up) or Lissajous, the
//! points fading with age as dots or lines, the correlation bar below, an
//! optional balance bar and a mono indication. The scope stretches to fill
//! whatever shape the Meter has, and in a wide, low Meter the bars stand
//! upright beside it.

use std::f32::consts::{FRAC_PI_4, PI};

use dasmeter_analysis::{StereoReadings, StereoView};
use dasmeter_core::{Role, StereoDrawing, StereometerMeterSettings};

use super::labels::Align;
use super::shapes::Area;
use super::{Canvas, map};

/// Segments used to draw the scope's circle.
const ARC_SEGMENTS: usize = 64;
/// A bar's height below the scope, or its width standing beside it, in
/// logical px.
const BAR: f32 = 26.0;
const UPRIGHT_BAR: f32 = 22.0;
/// How far the scope may be stretched to its Meter's shape: one radius at
/// most this many times the other.
const MAX_STRETCH: f32 = 6.0;

/// Where everything goes in a Meter of some shape.
#[derive(Debug)]
struct Layout {
    scope: Area,
    /// The correlation bar, then the balance bar if shown.
    bars: Vec<Area>,
    /// Whether the bars stand upright beside the scope.
    vertical: bool,
    centre: [f32; 2],
    /// Across and up: an ellipse when the scope's area isn't square.
    radius: [f32; 2],
}

/// The layout of `area` at `scale` physical px a logical one, with `bars` bars.
fn layout(area: Area, scale: f32, view: StereoView, bars: usize) -> Layout {
    let px = |v: f32| v * scale;
    // Upright beside the scope only when height is what's short: a narrow
    // Meter needs all its width for the scope.
    let vertical = area.width > area.height * 2.0;
    let (scope, bars) = if vertical {
        // The correlation bar at the right edge, the balance bar inside it.
        let (scope, mut side) = area.split_right(px(UPRIGHT_BAR) * bars as f32);
        let bars = (0..bars)
            .map(|_| {
                let (rest, bar) = side.split_right(px(UPRIGHT_BAR));
                side = rest;
                bar
            })
            .collect();
        (scope, bars)
    } else {
        let (scope, mut below) = area.split_bottom(px(BAR) * bars as f32);
        let bars = (0..bars)
            .map(|_| {
                let (bar, rest) = below.split_top(px(BAR));
                below = rest;
                bar
            })
            .collect();
        (scope, bars)
    };
    // Room for the label above the half circle, or round the full one.
    let (top, bottom, side) = match view {
        StereoView::Polar => (px(14.0), px(4.0), px(8.0)),
        StereoView::Lissajous => (px(4.0), px(4.0), px(4.0)),
    };
    let halves = match view {
        StereoView::Polar => 1.0,
        StereoView::Lissajous => 2.0,
    };
    let across = (scope.width / 2.0 - side).max(1.0);
    let up = ((scope.height - top - bottom) / halves).max(1.0);
    // Stretched to the Meter's shape, but no more than this either way, so
    // the scope still reads as one.
    let radius = [across.min(up * MAX_STRETCH), up.min(across * MAX_STRETCH)];
    // Centred in the room it has.
    let height = radius[1] * halves + top + bottom;
    let first = scope.y + (scope.height - height) / 2.0 + top;
    let centre = [scope.x + scope.width / 2.0, first + radius[1]];
    Layout {
        scope,
        bars,
        vertical,
        centre,
        radius,
    }
}

pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &StereometerMeterSettings,
    readings: &StereoReadings,
    points: &[[f32; 2]],
) {
    let bars = if settings.show_balance { 2 } else { 1 };
    let layout = layout(area, c.px(1.0), settings.view, bars);
    let scope = layout.scope;
    let (grid, dim) = (c.colour(Role::Grid), c.dim());
    let thin = c.px(1.0).max(1.0);

    // Where a point lands.
    let (centre, [across, up]) = (layout.centre, layout.radius);
    let place = |[x, y]: [f32; 2]| [centre[0] + x * across, centre[1] - y * up];

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
        // L and R only when there's room between them.
        let side = (place([end[0], 0.0])[0] - centre[0]).abs();
        if name != "M" && side < c.px(8.0) {
            continue;
        }
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
    let correlation_area = layout.bars[0];
    let bar = |c: &mut Canvas, area, value, colour, labels| {
        if layout.vertical {
            upright_bar(c, area, value, colour, labels);
        } else {
            bar(c, area, value, colour, labels);
        }
    };
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
    if let Some(&balance_area) = layout.bars.get(1) {
        let accent = c.colour(Role::Accent);
        bar(c, balance_area, readings.balance, accent, ["L", "C", "R"]);
    }
}

/// A standing bar from −1 (bottom) to +1 (top) with `value` filled from the
/// centre and a marker; the end labels above and below it, the middle one
/// beside it.
fn upright_bar(
    c: &mut Canvas,
    area: Area,
    value: f32,
    colour: dasmeter_core::Colour,
    labels: [&str; 3],
) {
    let dim = c.dim();
    let track_width = c.px(8.0);
    let track = Area {
        x: area.x + c.px(3.0),
        y: area.y + c.px(14.0),
        width: track_width,
        height: (area.height - c.px(28.0)).max(1.0),
    };
    let background = c.colour(Role::Background);
    c.shapes.rect(track, background);
    let y_of = |v: f32| map(v, (-1.0, 1.0), track.bottom(), track.y);
    let (zero, at) = (y_of(0.0), y_of(value));
    let fill = Area {
        y: zero.min(at),
        height: (at - zero).abs(),
        ..track
    };
    c.shapes.rect(fill, colour.faded(0.6));
    let marker = Area {
        x: track.x - c.px(2.0),
        y: at - c.px(1.0),
        width: track_width + c.px(4.0),
        height: c.px(2.0),
    };
    c.shapes.rect(marker, colour);
    let grid = c.colour(Role::Grid);
    c.shapes.rect(
        Area {
            y: zero - c.px(0.5),
            height: c.px(1.0),
            ..track
        },
        grid,
    );
    let middle = track.x + track_width / 2.0;
    c.text(labels[2], middle, area.y + c.px(1.0), 9.0, dim, Align::Centre);
    c.text(labels[0], middle, track.bottom() + c.px(2.0), 9.0, dim, Align::Centre);
    c.text(
        labels[1],
        track.right() + c.px(4.0),
        zero - c.px(6.0),
        9.0,
        dim,
        Align::Left,
    );
}

/// A horizontal bar from −1 to +1 with `value` filled from the centre and a marker.
fn bar(c: &mut Canvas, area: Area, value: f32, colour: dasmeter_core::Colour, labels: [&str; 3]) {
    let dim = c.dim();
    let track_height = c.px(8.0);
    // Less inset when narrow, so the track keeps most of the width.
    let inset = c.px(14.0).min(area.width * 0.1);
    let track = Area {
        x: area.x + inset,
        y: area.y + c.px(4.0),
        width: (area.width - 2.0 * inset).max(1.0),
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
    // The end labels only when they clear the middle one.
    let y = track.bottom() + c.px(1.0);
    let label = c.px(9.0 * 0.6 * 2.0);
    if track.width >= label * 4.0 {
        c.text(labels[0], track.x, y, 9.0, dim, Align::Left);
        c.text(labels[2], track.right(), y, 9.0, dim, Align::Right);
    }
    c.text(labels[1], zero, y, 9.0, dim, Align::Centre);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(width: f32, height: f32) -> Area {
        Area {
            x: 0.0,
            y: 0.0,
            width,
            height,
        }
    }

    fn inside(outer: Area, inner: Area) -> bool {
        inner.x >= outer.x
            && inner.y >= outer.y
            && inner.right() <= outer.right() + 1e-3
            && inner.bottom() <= outer.bottom() + 1e-3
    }

    #[test]
    fn a_tall_narrow_meter_stretches_the_scope_up() {
        let block = area(70.0, 300.0);
        let l = layout(block, 1.0, StereoView::Polar, 2);
        assert!(!l.vertical, "all the width for the scope");
        assert_eq!(l.scope.width, block.width);
        for bar in &l.bars {
            assert!(inside(block, *bar));
            assert!(bar.y >= l.scope.bottom());
        }
        // The half circle grows upward into the room it has, as far as it
        // still reads as one, and sits in the middle of it.
        assert!((l.radius[1] / l.radius[0] - MAX_STRETCH).abs() < 1e-3, "{l:?}");
        assert!(l.radius[0] * 2.0 <= l.scope.width);
        let middle = l.centre[1] - l.radius[1] / 2.0;
        assert!((middle - l.scope.height / 2.0).abs() < 10.0, "{l:?}");
    }

    #[test]
    fn a_wide_low_meter_puts_the_bars_beside() {
        let block = area(900.0, 100.0);
        let l = layout(block, 1.0, StereoView::Lissajous, 2);
        assert!(l.vertical);
        assert_eq!(l.scope.height, block.height);
        for bar in &l.bars {
            assert!(inside(block, *bar));
            assert!(bar.height > bar.width, "upright: {bar:?}");
            assert!(bar.x >= l.scope.right());
        }
        // Wider than high, as far as it still reads as one.
        assert!((l.radius[0] / l.radius[1] - MAX_STRETCH).abs() < 1e-3, "{l:?}");
    }

    #[test]
    fn a_wide_meter_keeps_the_bars_below() {
        let block = area(400.0, 200.0);
        let l = layout(block, 1.0, StereoView::Lissajous, 1);
        assert!(!l.vertical);
        assert!(l.bars[0].width > l.bars[0].height);
        assert!(l.bars[0].y >= l.scope.bottom());
        // The scope uses the room it has each way.
        assert!(l.radius[0] > l.radius[1]);
        assert!(l.radius[0] * 2.0 <= l.scope.width);
        assert!(l.radius[1] * 2.0 <= l.scope.height);
    }

    #[test]
    fn a_square_meter_keeps_the_circle_about_round() {
        let l = layout(area(300.0, 340.0), 1.0, StereoView::Lissajous, 1);
        assert!((l.radius[0] / l.radius[1] - 1.0).abs() < 0.05, "{:?}", l.radius);
    }
}
