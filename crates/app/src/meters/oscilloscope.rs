//! The Oscilloscope: the wave itself across a few milliseconds, held still on
//! its rising edge. One trace (mono), or two drawn over each other (L/R or
//! M/S, the second in its own colour), over a centre line, half-height lines
//! and time divisions labelled in milliseconds. With auto gain on, the gain
//! it adds is shown in the corner.

use dasmeter_analysis::Oscilloscope;
use dasmeter_core::{OscilloscopeMeterSettings, Role};

use super::Canvas;
use super::labels::Align;
use super::shapes::Area;

/// Time divisions to choose from, in milliseconds.
const DIVISIONS_MS: [f32; 12] = [
    0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0,
];

/// Divisions across the window, at most.
const MOST_DIVISIONS: f32 = 8.0;

pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &OscilloscopeMeterSettings,
    scope: &Oscilloscope,
) {
    let (grid, dim) = (c.colour(Role::Grid), c.dim());
    let (plot, axis) = area.split_bottom(c.px(16.0));
    let mid = plot.y + plot.height / 2.0;
    let half = plot.height / 2.0 - c.px(4.0);

    // Centre line, and lines at half height.
    c.grid_h(plot, mid, grid, true);
    for y in [mid - half / 2.0, mid + half / 2.0] {
        c.grid_h(plot, y, grid.faded(0.5), false);
    }

    // Time divisions: the smallest step that keeps to a handful across.
    let window_ms = scope.window.as_secs_f32() * 1_000.0;
    if window_ms > 0.0 {
        let step = DIVISIONS_MS
            .into_iter()
            .find(|&step| window_ms / step <= MOST_DIVISIONS)
            .unwrap_or(DIVISIONS_MS[DIVISIONS_MS.len() - 1]);
        let mut last_label: Option<f32> = None;
        let mut t = step;
        while t < window_ms - step * 0.01 {
            let x = plot.x + t / window_ms * plot.width;
            c.grid_v(plot, x, grid.faded(0.6), false);
            let label = ms_label(t);
            let half_width = c.px(9.0 * 0.6) * label.len() as f32 / 2.0;
            if x + half_width <= plot.right()
                && last_label.is_none_or(|last| x - half_width > last + c.px(4.0))
            {
                c.text(&label, x, axis.y + c.px(2.0), 9.0, dim, Align::Centre);
                last_label = Some(x + half_width);
            }
            t += step;
        }
    }

    // The traces, the second behind the first.
    let roles = [Role::OscilloscopeTrace, Role::OscilloscopeSecond];
    for (values, role) in scope.traces.iter().zip(roles).rev() {
        if values.len() < 2 {
            continue;
        }
        let colour = c.colour(role);
        let step = plot.width / (values.len() - 1) as f32;
        let points: Vec<[f32; 2]> = values
            .iter()
            .enumerate()
            .map(|(i, &v)| [plot.x + i as f32 * step, mid - v.clamp(-1.0, 1.0) * half])
            .collect();
        c.glow_ribbon(&points, colour);
        c.shapes.polyline(&points, c.stroke(1.5), colour);
    }

    // The gain auto gain adds, and a note when nothing holds it still.
    let mut corner = Vec::new();
    if settings.analysis.auto_gain && scope.gain_db >= 0.05 {
        corner.push(format!("+{:.1} dB", scope.gain_db));
    }
    if settings.analysis.trigger && !scope.triggered {
        corner.push("Free".to_owned());
    }
    if !corner.is_empty() {
        c.text(
            &corner.join("  "),
            plot.right() - c.px(4.0),
            plot.y + c.px(2.0),
            10.0,
            dim,
            Align::Right,
        );
    }
}

/// "0.5 ms", "5 ms".
fn ms_label(ms: f32) -> String {
    if ms < 1.0 {
        format!("{ms:.1} ms")
    } else {
        format!("{ms:.0} ms")
    }
}
