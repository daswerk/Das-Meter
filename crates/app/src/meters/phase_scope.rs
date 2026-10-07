//! The Phase Scope: the waveform over one Cycle, the newest sharp and the
//! few before it fading behind, with a centre line, the beat lines of a bar,
//! and the tempo it follows. An Overlay Source is drawn over it in its own
//! colour; a phase lane under the Cycle shows
//! where the two push together (green) and where they cancel (red), and the
//! low end's correlation sits large in the top right. The sum of both is
//! drawn as its own waveform on top, and suggestions for a better fit are
//! listed under the readout when asked for.

use dasmeter_core::{
    Colour, PhaseScopeMeterSettings, PhaseScopeView, Role, ScopeOverlay, ScopeTrace,
};

use super::Canvas;
use super::labels::Align;
use super::shapes::Area;

/// Below this height (logical px) the correlation readout is left out, the
/// first thing to go, so the traces keep the room.
const MIN_NUMBER_HEIGHT: f32 = 64.0;
/// Below this height the tempo readout and the phase lane go too.
const MIN_TEXT_HEIGHT: f32 = 40.0;
/// The tempo readout's row at the bottom, in logical px.
const TEXT_ROW: f32 = 14.0;
/// The phase lane's height, and the gap around it, in logical px.
const LANE: f32 = 10.0;
const LANE_GAP: f32 = 3.0;
/// A column quieter than this (after gain) leaves the lane empty there.
const LANE_QUIET: f32 = 0.02;

pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &PhaseScopeMeterSettings,
    scope: &PhaseScopeView,
) {
    let (grid, dim) = (c.colour(Role::PhaseScopeGrid), c.dim());
    let thin = c.px(1.0).max(1.0);
    let roomy = area.height >= c.px(MIN_TEXT_HEIGHT);

    // Room at the bottom for the tempo readout and, with an overlay, the lane.
    let (mut plot, text_row) = if roomy {
        let (plot, row) = area.split_bottom(c.px(TEXT_ROW));
        (plot, Some(row))
    } else {
        (area, None)
    };
    let lane = match (&scope.overlay, roomy) {
        (Some(overlay), true) if overlay.waiting.is_none() => {
            let (rest, lane) = plot.split_bottom(c.px(LANE + 2.0 * LANE_GAP));
            plot = rest;
            Some(Area {
                y: lane.y + c.px(LANE_GAP),
                height: c.px(LANE),
                ..lane
            })
        }
        _ => None,
    };

    // Beat lines across the whole plot, then one band per trace with its centre line.
    for &x in &scope.beat_lines {
        let x = plot.x + x * plot.width;
        c.shapes.rect(
            Area {
                x: x - thin / 2.0,
                width: thin,
                ..plot
            },
            grid.faded(0.6),
        );
    }
    let colour = scope
        .colour
        .unwrap_or_else(|| c.colour(Role::PhaseScopeTrace));
    let bands = scope.traces.len().max(1);
    let band_height = plot.height / bands as f32;
    for (t, trace) in scope.traces.iter().enumerate() {
        let band = Area {
            y: plot.y + t as f32 * band_height,
            height: band_height,
            ..plot
        };
        let centre = band.y + band.height / 2.0;
        c.shapes.rect(
            Area {
                y: centre - thin / 2.0,
                height: thin,
                ..band
            },
            grid,
        );
        // Older Cycles first, fainter the older they are.
        let older = scope.trail.len();
        for (i, cycle) in scope.trail.iter().enumerate() {
            if let Some(old) = cycle.get(t) {
                let fade = 0.12 + 0.2 * (i + 1) as f32 / (older + 1) as f32;
                trace_line(c, band, old, colour.faded(fade), false);
            }
        }
        let overlay = scope.overlay.as_ref().filter(|_| t == 0);
        if let Some(overlay) = overlay {
            overlay_trace(c, band, overlay, colour);
        }
        trace_line(c, band, trace, colour, settings.filled);
        if let Some(overlay) = overlay.filter(|_| settings.show_sum) {
            sum_wave(c, band, overlay);
        }
    }

    if let (Some(lane), Some(overlay), Some(main)) = (lane, &scope.overlay, scope.traces.first()) {
        phase_lane(c, lane, overlay, main);
    }

    let Some(row) = text_row else {
        return;
    };
    let mut readout = format!("{:.1} BPM", scope.tempo);
    if !scope.following_daw {
        readout += " · typed in";
    }
    if let Some(note) = &scope.note {
        readout = note.clone();
    }
    c.text(
        &readout,
        row.x + c.px(2.0),
        row.y + c.px(1.0),
        9.0,
        dim,
        Align::Left,
    );
    if let Some(name) = scope.overlay.as_ref().and_then(|o| o.waiting.as_ref()) {
        c.text(
            &format!("Waiting for {name}"),
            plot.x + c.px(2.0),
            plot.y + c.px(2.0),
            9.0,
            dim,
            Align::Left,
        );
    }
    if area.height >= c.px(MIN_NUMBER_HEIGHT)
        && let Some(correlation) = scope.overlay.as_ref().and_then(|o| o.correlation)
    {
        correlation_readout(c, plot, correlation, settings.cutoff);
        if let Some(overlay) = &scope.overlay {
            advice(c, plot, &overlay.advice);
        }
    }
}

/// The suggestions under the correlation readout, as many as fit: each on
/// one line, or broken after its colon when that's too wide.
fn advice(c: &mut Canvas, plot: Area, suggestions: &[String]) {
    let text = c.colour(Role::Text);
    let right = plot.right() - c.px(4.0);
    let room = plot.width - c.px(8.0);
    // About how wide the monospace labels are.
    let fits = |c: &Canvas, line: &str| c.px(9.0 * 0.6) * line.chars().count() as f32 <= room;
    let mut y = plot.y + c.px(37.0);
    for suggestion in suggestions {
        let lines: Vec<&str> = if fits(c, suggestion) {
            vec![suggestion.as_str()]
        } else {
            match suggestion.split_once(": ") {
                Some((what, then)) => vec![what, then],
                None => vec![suggestion.as_str()],
            }
        };
        if lines.iter().any(|line| !fits(c, line))
            || y + c.px(12.0) * lines.len() as f32 > plot.bottom()
        {
            break;
        }
        for line in lines {
            c.text(line, right, y, 9.0, text, Align::Right);
            y += c.px(12.0);
        }
    }
}

/// The x of column `i`'s centre, and the y of value `v`, in `band`.
fn place(band: Area, columns: usize) -> impl Fn(usize, f32) -> [f32; 2] {
    let step = band.width / columns.max(1) as f32;
    let half = band.height / 2.0 * 0.95;
    let centre = band.y + band.height / 2.0;
    move |i, v| [band.x + (i as f32 + 0.5) * step, centre - v * half]
}

/// One trace in its band as a smooth line: through each column's value
/// where the column holds one, and as a band from its lowest to highest
/// value where it holds many (dense, fast content). Filled to the centre
/// line if asked.
fn trace_line(c: &mut Canvas, band: Area, trace: &ScopeTrace, colour: Colour, filled: bool) {
    let columns = trace.max.len();
    if columns < 2 {
        return;
    }
    let at = place(band, columns);
    let high: Vec<[f32; 2]> = trace
        .max
        .iter()
        .enumerate()
        .map(|(i, &v)| at(i, v))
        .collect();
    let low: Vec<[f32; 2]> = trace
        .min
        .iter()
        .enumerate()
        .map(|(i, &v)| at(i, v))
        .collect();
    let centre = band.y + band.height / 2.0;
    if filled {
        let mid: Vec<[f32; 2]> = high
            .iter()
            .zip(&low)
            .map(|(h, l)| [h[0], (h[1] + l[1]) / 2.0])
            .collect();
        let fill = colour.faded(0.3);
        c.shapes.fill_under(&mid, centre, fill, fill);
    }
    // The space between the highest and lowest values, solid.
    for i in 0..columns - 1 {
        c.shapes
            .quad([high[i], high[i + 1], low[i], low[i + 1]], colour, colour);
    }
    let stroke = c.stroke(1.5);
    c.shapes.polyline(&high, stroke, colour);
    c.shapes.polyline(&low, stroke, colour);
}

/// The Overlay Source's trace, in its own colour.
fn overlay_trace(c: &mut Canvas, band: Area, overlay: &ScopeOverlay, main: Colour) {
    let colour = overlay
        .colour
        .unwrap_or_else(|| super::mix(main, c.colour(Role::Text), 0.6));
    trace_line(c, band, &overlay.trace, colour.faded(0.85), false);
}

/// The sum of both, what's actually left in the mix, as a waveform of its
/// own drawn over them: a soft band with a bright outline.
fn sum_wave(c: &mut Canvas, band: Area, overlay: &ScopeOverlay) {
    let colour = c.colour(Role::PhaseScopeSum);
    let columns = overlay.sum.max.len();
    if columns < 2 {
        return;
    }
    let at = place(band, columns);
    let edge = |values: &[f32]| -> Vec<[f32; 2]> {
        values.iter().enumerate().map(|(i, &v)| at(i, v)).collect()
    };
    let (high, low) = (edge(&overlay.sum.max), edge(&overlay.sum.min));
    let fill = colour.faded(0.35);
    for i in 0..columns - 1 {
        c.overlay
            .quad([high[i], high[i + 1], low[i], low[i + 1]], fill, fill);
    }
    let stroke = c.stroke(1.5);
    c.overlay.polyline(&high, stroke, colour);
    c.overlay.polyline(&low, stroke, colour);
}

/// A strip under the Cycle, one cell per column: green where the two push
/// the same way, red where they cancel, stronger the louder they are there,
/// empty where both are quiet.
fn phase_lane(c: &mut Canvas, lane: Area, overlay: &ScopeOverlay, main: &ScopeTrace) {
    let (together, against) = (
        c.colour(Role::CorrelationPositive),
        c.colour(Role::PhaseScopeCancel),
    );
    c.shapes
        .rect(lane, c.colour(Role::PhaseScopeGrid).faded(0.35));
    let columns = overlay.phase.len().max(1);
    let step = lane.width / columns as f32;
    let peak = |t: &ScopeTrace, i: usize| {
        let low = t.min.get(i).copied().unwrap_or(0.0);
        let high = t.max.get(i).copied().unwrap_or(0.0);
        low.abs().max(high.abs())
    };
    for (i, &phase) in overlay.phase.iter().enumerate() {
        let loud = peak(main, i).max(peak(&overlay.trace, i));
        if loud < LANE_QUIET || phase == 0.0 {
            continue;
        }
        let colour = if phase > 0.0 { together } else { against };
        let strength = phase.abs() * (0.35 + 0.65 * loud.min(1.0));
        c.shapes.rect(
            Area {
                x: lane.x + i as f32 * step,
                width: step.max(1.0),
                ..lane
            },
            colour.faded(strength),
        );
    }
}

/// The low end's correlation, large in the top right, with what it means
/// in words under it.
fn correlation_readout(c: &mut Canvas, plot: Area, correlation: f32, cutoff: f32) {
    let colour = if correlation < 0.0 {
        c.colour(Role::PhaseScopeCancel)
    } else if correlation < 0.5 {
        c.dim()
    } else {
        c.colour(Role::CorrelationPositive)
    };
    let meaning = if correlation < 0.0 {
        "lows cancel"
    } else if correlation < 0.5 {
        "lows partly apart"
    } else {
        "lows in phase"
    };
    let right = plot.right() - c.px(4.0);
    c.bold(
        &format!("{correlation:+.2}"),
        right,
        plot.y + c.px(3.0),
        16.0,
        colour,
        Align::Right,
    );
    let dim = c.dim();
    c.text(
        &format!("{meaning} · below {cutoff:.0} Hz"),
        right,
        plot.y + c.px(23.0),
        9.0,
        dim,
        Align::Right,
    );
}
