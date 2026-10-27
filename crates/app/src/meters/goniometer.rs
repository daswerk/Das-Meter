//! The Goniometer, as on the old studio meters: L against R drawn as the
//! beam of a scope, turned so mono stands upright, in a square with the L
//! and R axes faintly dotted across it. Each sample is joined to the next
//! (smoothly, where they're far apart), and the trace leaves trails: older
//! passes thinner and dimmer, the newest bright and hot (under a soft glow
//! when the Theme has Glow), so the figure reads as a shape with depth. The scale it's drawn at is
//! read out on top, the correlation bar below.

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
    let axis = dim.faded(0.25);

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

    // The trace: oldest faintest and thinnest, under a soft glow (as much
    // as the Theme's Glow, Smooth only).
    let trace = c.colour(Role::StereometerTrace);
    let hot = mix(trace, Colour::rgb(0xff, 0xff, 0xff), 0.4);
    let segments = trail(
        &points.iter().map(|&p| place(p)).collect::<Vec<_>>(),
        c.px(3.0),
    );
    let count = segments.len().max(1) as f32;
    let (glow, thin, thick) = (c.px(4.0), c.px(0.6), c.px(1.8));
    let strength = if c.smooth() { c.styling.glow } else { 0.0 };
    if strength > 0.0 {
        for (i, &(from, to)) in segments.iter().enumerate() {
            let age = (i as f32 + 1.0) / count;
            c.shapes.glow_line(
                from,
                to,
                glow * (0.5 + 0.5 * age),
                trace.faded(0.05 * age * strength),
            );
        }
    }
    for (i, &(from, to)) in segments.iter().enumerate() {
        let age = (i as f32 + 1.0) / count;
        let colour = mix(trace, hot, age * age).faded(0.12 + 0.88 * age);
        c.shapes.line(from, to, thin + (thick - thin) * age, colour);
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

/// The trace through `points` (in px, oldest first) as segments: straight
/// where samples are close, a Catmull-Rom curve through them where they're
/// more than `step` apart, so a slow, wide figure stays round.
fn trail(points: &[[f32; 2]], step: f32) -> Vec<([f32; 2], [f32; 2])> {
    let mut segments = Vec::with_capacity(points.len());
    for i in 1..points.len() {
        let (a, b) = (points[i - 1], points[i]);
        let before = points[i.saturating_sub(2)];
        let after = points[(i + 1).min(points.len() - 1)];
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        let pieces = (length / step.max(0.5)).ceil().clamp(1.0, 8.0) as usize;
        let mut from = a;
        for k in 1..=pieces {
            let t = k as f32 / pieces as f32;
            let to = if k == pieces {
                b
            } else {
                catmull_rom(before, a, b, after, t)
            };
            segments.push((from, to));
            from = to;
        }
    }
    segments
}

/// The Catmull-Rom curve from `p1` to `p2` at `t` (0–1).
fn catmull_rom(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], t: f32) -> [f32; 2] {
    let (t2, t3) = (t * t, t * t * t);
    let axis = |i: usize| {
        0.5 * (2.0 * p1[i]
            + (p2[i] - p0[i]) * t
            + (2.0 * p0[i] - 5.0 * p1[i] + 4.0 * p2[i] - p3[i]) * t2
            + (3.0 * p1[i] - p0[i] - 3.0 * p2[i] + p3[i]) * t3)
    };
    [axis(0), axis(1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn far_apart_samples_are_joined_by_a_curve_through_them() {
        let points = [[0.0, 0.0], [10.0, 0.0], [20.0, 10.0], [30.0, 10.0]];
        let segments = trail(&points, 3.0);
        assert!(segments.len() > 3, "subdivided: {}", segments.len());
        // It starts and ends on the samples and passes through each one.
        assert_eq!(segments[0].0, points[0]);
        assert_eq!(segments.last().unwrap().1, points[3]);
        for p in &points[1..3] {
            assert!(segments.iter().any(|s| s.1 == *p), "through {p:?}");
        }
        // Joined end to end.
        for pair in segments.windows(2) {
            assert_eq!(pair[0].1, pair[1].0);
        }
    }

    #[test]
    fn close_samples_stay_straight() {
        let points = [[0.0, 0.0], [1.0, 1.0], [2.0, 1.5]];
        assert_eq!(trail(&points, 3.0).len(), 2);
    }
}
