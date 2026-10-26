//! The Goniometer, as on the old studio meters: a square with the L and R
//! axes dotted across it, and L against R as a cloud of glowing points,
//! turned so mono stands upright. Each point is a faint wide dot under a
//! small bright one: where many land, the cloud burns towards white, so the
//! densest part of the stereo image glows hottest. The scale it's drawn at
//! is read out on top, the correlation bar below.

use dasmeter_analysis::StereoReadings;
use dasmeter_core::{Colour, GoniometerMeterSettings, Role};

use super::labels::Align;
use super::shapes::Area;
use super::{Canvas, mix, stereometer};

/// The correlation bar's height, in logical px.
const BAR: f32 = 26.0;
/// Room above the square for the scale and the L and R, in logical px.
const TOP: f32 = 16.0;
/// Dots along the axes, this far apart, in logical px.
const DOT_STEP: f32 = 6.0;

pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &GoniometerMeterSettings,
    readings: &StereoReadings,
    points: &[[f32; 2]],
    scale_db: f32,
) {
    let (scope, bar) = if settings.show_correlation {
        let (scope, bar) = area.split_bottom(c.px(BAR));
        (scope, Some(bar))
    } else {
        (area, None)
    };
    let (_, room) = scope.split_top(c.px(TOP));
    let side = room.width.min(room.height).max(1.0);
    let square = Area {
        x: room.x + (room.width - side) / 2.0,
        y: room.y + (room.height - side) / 2.0,
        width: side,
        height: side,
    };
    let centre = [square.x + side / 2.0, square.y + side / 2.0];
    let half = side / 2.0;
    let place = |[x, y]: [f32; 2]| [centre[0] + x * half, centre[1] - y * half];
    let dim = c.dim();
    let axis = dim.faded(0.45);

    // The L and R axes, dotted corner to corner.
    let dot = c.px(1.5).max(1.0);
    let steps = (side * std::f32::consts::SQRT_2 / c.px(DOT_STEP)).max(1.0) as usize;
    for (from, to) in [([-1.0, 1.0], [1.0, -1.0]), ([1.0, 1.0], [-1.0, -1.0])] {
        let (a, b) = (place(from), place(to));
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let at = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            c.shapes.rect(
                Area {
                    x: at[0] - dot / 2.0,
                    y: at[1] - dot / 2.0,
                    width: dot,
                    height: dot,
                },
                axis,
            );
        }
    }
    let label_y = square.y - c.px(TOP) + c.px(2.0);
    c.text("L", square.x, label_y, 10.0, dim, Align::Left);
    c.text("R", square.right(), label_y, 10.0, dim, Align::Right);
    let scale = format!("Scale {scale_db:+.1} dB");
    c.text(&scale, centre[0], label_y, 10.0, dim, Align::Centre);

    // The cloud: oldest faintest; a wide faint glow under a small hot core.
    let trace = c.colour(Role::StereometerTrace);
    let hot = mix(trace, Colour::rgb(0xff, 0xff, 0xff), 0.55);
    let count = points.len().max(1) as f32;
    let (glow, core) = (c.px(4.5), c.px(1.3).max(1.0));
    let square_dot = |at: [f32; 2], size: f32| Area {
        x: at[0] - size / 2.0,
        y: at[1] - size / 2.0,
        width: size,
        height: size,
    };
    for (i, &point) in points.iter().enumerate() {
        let age = (i as f32 + 1.0) / count;
        let at = place(point);
        c.shapes
            .rect(square_dot(at, glow), trace.faded(0.03 + 0.09 * age));
        c.shapes
            .rect(square_dot(at, core), hot.faded(0.08 + 0.32 * age));
    }
    if readings.mono {
        let text = c.colour(Role::Text);
        c.text(
            "mono",
            square.right(),
            square.bottom() - c.px(12.0),
            10.0,
            text,
            Align::Right,
        );
    }

    if let Some(bar) = bar {
        let colour = if readings.correlation < 0.0 {
            c.colour(Role::CorrelationNegative)
        } else {
            c.colour(Role::CorrelationPositive)
        };
        let colour = if readings.no_signal {
            colour.faded(0.3)
        } else {
            colour
        };
        stereometer::bar(c, bar, readings.correlation, colour, ["−1", "0", "+1"]);
    }
}
