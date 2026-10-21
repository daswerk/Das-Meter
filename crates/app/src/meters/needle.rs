//! The Needle Meter: a classic dial per needle (Left and Right side by side,
//! or one on the mono sum), its scale's numbers around an arc, the red zone
//! over 0 VU or the PPM's permitted maximum, the needle swinging from a pivot
//! under the dial, and an overload lamp in the corner.

use dasmeter_core::{NeedleDisplay, NeedleMeterSettings, Role};

use super::Canvas;
use super::labels::Align;
use super::shapes::Area;

/// Half the dial's sweep, in radians, either side of upright.
const SWEEP: f32 = 0.84;

pub fn draw(c: &mut Canvas, area: Area, settings: &NeedleMeterSettings, needles: &[NeedleDisplay]) {
    if needles.is_empty() {
        return;
    }
    let names: &[&str] = if needles.len() == 1 {
        &["Mono"]
    } else {
        &["L", "R"]
    };
    let n = needles.len() as f32;
    let gap = c.px(8.0);
    // Side by side or stacked, whichever gives the bigger dials (a dial's
    // arc is about 1.5 times as wide as it is tall).
    let radius = |width: f32, height: f32| (width / (2.0 * SWEEP.sin())).min(height);
    let side_by_side = radius((area.width - gap * (n - 1.0)) / n, area.height)
        >= radius(area.width, (area.height - gap * (n - 1.0)) / n);
    for (i, needle) in needles.iter().enumerate() {
        let dial = if side_by_side {
            let width = (area.width - gap * (n - 1.0)) / n;
            Area {
                x: area.x + i as f32 * (width + gap),
                width,
                ..area
            }
        } else {
            let height = (area.height - gap * (n - 1.0)) / n;
            Area {
                y: area.y + i as f32 * (height + gap),
                height,
                ..area
            }
        };
        draw_dial(
            c,
            dial,
            settings,
            needle,
            names.get(i).copied().unwrap_or(""),
        );
    }
}

fn draw_dial(
    c: &mut Canvas,
    area: Area,
    settings: &NeedleMeterSettings,
    needle: &NeedleDisplay,
    name: &str,
) {
    let scale = settings.scale;
    let reference = settings.vu_reference;
    let (text, dim, red) = (
        c.colour(Role::Text),
        c.dim(),
        c.colour(Role::LoudnessCounter),
    );
    // The face.
    c.shapes
        .rounded_rect(area, c.px(6.0), c.colour(Role::Grid).faded(0.18));
    let inner = area.inset(c.px(8.0));
    let label_room = c.px(14.0) * c.styling.text_scale;
    // The arc as large as fits, with its numbers over it; the pivot under
    // it, the two together centred up and down.
    let radius = (inner.height - label_room - c.px(4.0))
        .min((inner.width / 2.0 - c.px(6.0)) / SWEEP.sin())
        .max(c.px(10.0));
    let total = radius + label_room + c.px(4.0);
    let pivot = [
        inner.x + inner.width / 2.0,
        inner.y + (inner.height - total) / 2.0 + total,
    ];
    let point = |t: f32, r: f32| {
        let angle = (t * 2.0 - 1.0) * SWEEP;
        [pivot[0] + r * angle.sin(), pivot[1] - r * angle.cos()]
    };
    let position = |db: f64| scale.dial_position(db, reference) as f32;

    // The red zone, as a thick arc just inside the ticks.
    let red_from = position(scale.red_from(reference));
    let steps = 24;
    for k in 0..steps {
        let (a, b) = (
            red_from + (1.0 - red_from) * k as f32 / steps as f32,
            red_from + (1.0 - red_from) * (k + 1) as f32 / steps as f32,
        );
        c.shapes.line(
            point(a, radius * 0.93),
            point(b, radius * 0.93),
            c.px(3.0),
            red.faded(0.8),
        );
    }
    // The arc and its numbers.
    let steps = 48;
    for k in 0..steps {
        let (a, b) = (k as f32 / steps as f32, (k + 1) as f32 / steps as f32);
        c.shapes
            .line(point(a, radius), point(b, radius), c.px(1.0), dim);
    }
    let mut last_x: Option<f32> = None;
    for (db, label) in scale.marks(reference) {
        let t = position(db);
        let colour = if t >= red_from - 1e-4 && db > scale.red_from(reference) - 1e-6 {
            red
        } else {
            dim
        };
        c.shapes
            .line(point(t, radius * 0.88), point(t, radius), c.px(1.5), colour);
        let [x, y] = point(t, radius + c.px(4.0));
        // Numbers that would crowd the one before are left out.
        if last_x.is_some_and(|last| (x - last).abs() < c.px(16.0)) {
            continue;
        }
        last_x = Some(x);
        c.text(&label, x, y - label_room, 9.0, colour, Align::Centre);
    }
    // Its name and the scale's, at the bottom corners.
    let bottom = inner.bottom() - c.px(12.0) * c.styling.text_scale;
    c.text(scale.name(), inner.x, bottom, 9.0, dim, Align::Left);
    c.text(name, inner.right(), bottom, 9.0, dim, Align::Right);

    // The overload lamp.
    let lamp = c.px(7.0);
    let lamp_area = Area {
        x: inner.right() - lamp,
        y: inner.y,
        width: lamp,
        height: lamp,
    };
    let lamp_colour = if needle.over {
        red
    } else {
        c.colour(Role::Grid)
    };
    c.shapes.rounded_rect(lamp_area, lamp / 2.0, lamp_colour);

    // The needle, at rest at the bottom of the scale in silence.
    let t = needle.level.db().map_or(0.0, position);
    c.shapes
        .line(pivot, point(t, radius * 1.02), c.stroke(1.8), text);
    let hub = c.px(8.0);
    c.shapes.rounded_rect(
        Area {
            x: pivot[0] - hub / 2.0,
            y: pivot[1] - hub / 2.0,
            width: hub,
            height: hub,
        },
        hub / 2.0,
        text,
    );
}
