//! The Phase Scope: the waveform over one Cycle, the newest sharp and the
//! few before it fading behind, with a centre line, the beat lines of a bar,
//! and the tempo it follows. An Overlay Source is drawn over it in its own
//! colour; a phase lane under the Cycle shows where the two push together
//! (green) and where they cancel (red). The low end's correlation and what
//! it means sit in a row of their own above the waves, the sum of both is
//! drawn over them or in its own row under them, and suggestions for a
//! better fit (or word that the lows fit) get rows under the lane. Text
//! never sits on the traces: when the Meter is small the suggestions go
//! first, then the words, then the number.

use dasmeter_core::{
    Colour, PhaseScopeMeterSettings, PhaseScopeView, Role, ScopeOverlay, ScopeTrace,
};

use super::Canvas;
use super::labels::Align;
use super::shapes::Area;

/// Below this height (logical px) the correlation row is left out, so the
/// traces keep the room.
const MIN_NUMBER_HEIGHT: f32 = 64.0;
/// Below this height the tempo readout and the phase lane go too.
const MIN_TEXT_HEIGHT: f32 = 40.0;
/// The traces always keep at least this much height before a suggestion
/// row is added, in logical px.
const MIN_PLOT: f32 = 48.0;
/// The tempo readout's row at the bottom, in logical px.
const TEXT_ROW: f32 = 14.0;
/// The correlation row above the waves, in logical px.
const HEADER_ROW: f32 = 19.0;
/// One suggestion line, in logical px.
const ADVICE_ROW: f32 = 12.0;
/// The phase lane's height, and the gap around it, in logical px.
const LANE: f32 = 10.0;
const LANE_GAP: f32 = 3.0;
/// A column quieter than this (after gain) leaves the lane empty there.
const LANE_QUIET: f32 = 0.02;
/// The newest trail Cycle's strength; older ones fade from it to nothing.
const TRAIL_STRENGTH: f32 = 0.32;
/// The small and large text sizes, in logical px.
const SMALL: f32 = 9.0;
const NUMBER: f32 = 13.0;

pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &PhaseScopeMeterSettings,
    scope: &PhaseScopeView,
) {
    let roomy = area.height >= c.px(MIN_TEXT_HEIGHT);
    let live = scope.overlay.as_ref().filter(|o| o.waiting.is_none());
    let waiting = scope.overlay.as_ref().and_then(|o| o.waiting.as_ref());

    // Rows from the outside in: the tempo at the bottom, the correlation
    // (or what's waited for) at the top, the lane over the tempo, and as
    // many suggestion lines as leave the traces room.
    let mut plot = area;
    let tempo_row = roomy.then(|| take_bottom(&mut plot, c.px(TEXT_ROW)));
    let header = match (waiting, live) {
        (Some(_), _) => roomy,
        (None, Some(o)) => o.correlation.is_some() && area.height >= c.px(MIN_NUMBER_HEIGHT),
        (None, None) => false,
    }
    .then(|| take_top(&mut plot, text_height(c, HEADER_ROW)));
    let lane_height = if live.is_some() && roomy {
        c.px(LANE + 2.0 * LANE_GAP)
    } else {
        0.0
    };
    // Whole suggestions only, while the traces keep their room.
    let groups = match live {
        Some(o) if settings.suggestions && roomy => advice_lines(c, plot.width, o),
        _ => Vec::new(),
    };
    let row = text_height(c, ADVICE_ROW);
    let mut room = ((plot.height - lane_height - c.px(MIN_PLOT)) / row)
        .floor()
        .max(0.0) as usize;
    let mut lines = Vec::new();
    for group in groups {
        if group.len() > room {
            break;
        }
        room -= group.len();
        lines.extend(group);
    }
    let advice_rows = (!lines.is_empty()).then(|| take_bottom(&mut plot, row * lines.len() as f32));
    let lane = match (live, roomy) {
        (Some(_), true) => {
            let lane = take_bottom(&mut plot, c.px(LANE + 2.0 * LANE_GAP));
            Some(Area {
                y: lane.y + c.px(LANE_GAP),
                height: c.px(LANE),
                ..lane
            })
        }
        _ => None,
    };
    // The sum in its own row under the two, when asked.
    let sum_row = match live {
        Some(_) if settings.show_sum && settings.split_sum => {
            let half = plot.height / 2.0;
            Some(take_bottom(&mut plot, half))
        }
        _ => None,
    };

    let colour = scope
        .colour
        .unwrap_or_else(|| c.colour(Role::PhaseScopeTrace));
    beat_lines(c, plot, scope);
    let bands = scope.traces.len().max(1);
    let band_height = plot.height / bands as f32;
    for (t, trace) in scope.traces.iter().enumerate() {
        let band = Area {
            y: plot.y + t as f32 * band_height,
            height: band_height,
            ..plot
        };
        centre_line(c, band);
        // Older Cycles first, fading on as the newest is drawn.
        let older = scope.trail.len();
        for (i, cycle) in scope.trail.iter().enumerate() {
            if let Some(old) = cycle.get(t) {
                let age = (older - i) as f32 + scope.progress;
                let fade = TRAIL_STRENGTH * (1.0 - (age - 1.0) / older as f32);
                if fade > 0.01 {
                    trace_line(c, band, old, colour.faded(fade), false, false);
                }
            }
        }
        let overlay = live.filter(|_| t == 0);
        if let Some(overlay) = overlay {
            overlay_trace(c, band, overlay, colour);
        }
        trace_line(c, band, trace, colour, settings.filled, true);
        if let Some(overlay) = overlay.filter(|_| settings.show_sum && sum_row.is_none()) {
            sum_wave(c, band, overlay, false);
        }
    }
    if let (Some(row), Some(overlay)) = (sum_row, live) {
        beat_lines(c, row, scope);
        centre_line(c, row);
        sum_wave(c, row, overlay, true);
    }

    if let (Some(lane), Some(overlay), Some(main)) = (lane, live, scope.traces.first()) {
        phase_lane(c, lane, overlay, main);
    }

    if let Some(row) = header {
        match (waiting, live.and_then(|o| o.correlation)) {
            (Some(name), _) => {
                let dim = c.dim();
                c.text(
                    &format!("Waiting for {name}"),
                    row.x + c.px(2.0),
                    row.y + c.px(3.0),
                    SMALL,
                    dim,
                    Align::Left,
                );
            }
            (None, Some(correlation)) => {
                correlation_readout(c, row, correlation, settings.cutoff);
            }
            (None, None) => {}
        }
    }
    if let Some(rows) = advice_rows {
        let mut y = rows.y;
        for line in &lines {
            draw_advice(c, rows.x + c.px(2.0), y, line);
            y += row;
        }
    }
    if let Some(row) = tempo_row {
        let mut readout = format!("{:.1} BPM", scope.tempo);
        if !scope.following_daw {
            readout += " · typed in";
        }
        if let Some(note) = &scope.note {
            readout = note.clone();
        }
        let dim = c.dim();
        c.text(
            &readout,
            row.x + c.px(2.0),
            row.y + c.px(1.0),
            SMALL,
            dim,
            Align::Left,
        );
    }
}

/// Cuts `height` off the top of `area`, returning it.
fn take_top(area: &mut Area, height: f32) -> Area {
    let (top, rest) = area.split_top(height.min(area.height));
    *area = rest;
    top
}

/// Cuts `height` off the bottom of `area`, returning it.
fn take_bottom(area: &mut Area, height: f32) -> Area {
    let (rest, bottom) = area.split_bottom(height.min(area.height));
    *area = rest;
    bottom
}

/// A text row's height: `logical` px, grown with the Theme's text size.
fn text_height(c: &Canvas, logical: f32) -> f32 {
    c.px(logical) * c.styling.text_scale.max(1.0)
}

/// About how wide `text` is at `size` (the labels are monospace).
fn text_width(c: &Canvas, text: &str, size: f32) -> f32 {
    0.6 * c.px(size) * c.styling.text_scale * text.chars().count() as f32
}

/// The beat lines inside a Cycle, across `area`.
fn beat_lines(c: &mut Canvas, area: Area, scope: &PhaseScopeView) {
    let grid = c.colour(Role::PhaseScopeGrid);
    for &x in &scope.beat_lines {
        let x = area.x + x * area.width;
        c.grid_v(area, x, grid.faded(0.6), false);
    }
}

/// The centre line across a band.
fn centre_line(c: &mut Canvas, band: Area) {
    let grid = c.colour(Role::PhaseScopeGrid);
    c.grid_h(band, band.y + band.height / 2.0, grid, true);
}

/// One suggestion line, as drawn.
enum Line {
    /// Plain words.
    Words(String),
    /// "Lows fit" and the correlation, with a tick, in green.
    Fits(f32),
}

/// What goes under the lane, one group of lines per suggestion: each on
/// one line or broken after its colon when that's too wide (dropped if
/// still too wide), or word that the lows fit, or that nothing simple helps.
fn advice_lines(c: &Canvas, width: f32, overlay: &ScopeOverlay) -> Vec<Vec<Line>> {
    let room = width - c.px(4.0);
    let fits = |line: &str| text_width(c, line, SMALL) <= room;
    match overlay.lows_fit {
        Some(true) => {
            // The tick takes about two characters.
            return overlay
                .correlation
                .filter(|r| fits(&format!("  Lows fit: {r:+.2}")))
                .map(|r| vec![Line::Fits(r)])
                .into_iter()
                .collect();
        }
        Some(false) if overlay.advice.is_empty() => {
            let words = "No flip, move or pitch would help";
            return [words, "No simple fix"]
                .into_iter()
                .find(|w| fits(w))
                .map(|w| vec![Line::Words(w.to_owned())])
                .into_iter()
                .collect();
        }
        _ => {}
    }
    let mut groups = Vec::new();
    for suggestion in &overlay.advice {
        let parts: Vec<&str> = if fits(suggestion) {
            vec![suggestion.as_str()]
        } else {
            match suggestion.split_once(": ") {
                Some((what, then)) => vec![what, then],
                None => vec![suggestion.as_str()],
            }
        };
        if parts.iter().all(|part| fits(part)) {
            groups.push(
                parts
                    .into_iter()
                    .map(|p| Line::Words(p.to_owned()))
                    .collect(),
            );
        }
    }
    groups
}

/// One line under the lane, its top-left at (`x`, `y`).
fn draw_advice(c: &mut Canvas, x: f32, y: f32, line: &Line) {
    match line {
        Line::Words(words) => {
            let text = c.colour(Role::Text);
            c.text(words, x, y, SMALL, text, Align::Left);
        }
        Line::Fits(correlation) => {
            let green = c.colour(Role::CorrelationPositive);
            // A tick drawn, not a glyph: every font has it then.
            let size = c.px(SMALL) * c.styling.text_scale;
            let stroke = c.stroke(1.5);
            let (left, top) = (x + c.px(1.0), y + size * 0.15);
            let low = [left + size * 0.3, top + size * 0.75];
            c.shapes.line([left, top + size * 0.45], low, stroke, green);
            c.shapes.line(low, [left + size * 0.85, top], stroke, green);
            c.text(
                &format!("Lows fit: {correlation:+.2}"),
                x + size * 1.2,
                y,
                SMALL,
                green,
                Align::Left,
            );
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
fn trace_line(
    c: &mut Canvas,
    band: Area,
    trace: &ScopeTrace,
    colour: Colour,
    filled: bool,
    glow: bool,
) {
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
        c.fill_under(&mid, centre, fill, fill);
    }
    // The space between the highest and lowest values, solid.
    for i in 0..columns - 1 {
        c.shapes
            .quad([high[i], high[i + 1], low[i], low[i + 1]], colour, colour);
    }
    if glow {
        c.glow(&high, colour);
        c.glow(&low, colour);
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
    trace_line(c, band, &overlay.trace, colour.faded(0.85), false, false);
}

/// The sum of both, what's actually left in the mix, as a waveform of its
/// own: over the two as a faint band with a thin outline, so they still
/// show through, or full strength in its own row.
fn sum_wave(c: &mut Canvas, band: Area, overlay: &ScopeOverlay, own_row: bool) {
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
    let (fill, line, stroke) = if own_row {
        (colour.faded(0.35), colour, c.stroke(1.5))
    } else {
        (colour.faded(0.12), colour.faded(0.7), c.stroke(1.0))
    };
    let layer = if own_row {
        &mut *c.shapes
    } else {
        &mut *c.overlay
    };
    for i in 0..columns - 1 {
        layer.quad([high[i], high[i + 1], low[i], low[i + 1]], fill, fill);
    }
    layer.polyline(&high, stroke, line);
    layer.polyline(&low, stroke, line);
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

/// The low end's correlation in its row above the waves: the number at the
/// right, what it means to its left when there's room.
fn correlation_readout(c: &mut Canvas, row: Area, correlation: f32, cutoff: f32) {
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
    let right = row.right() - c.px(4.0);
    let number = format!("{correlation:+.2}");
    c.bold(
        &number,
        right,
        row.y + c.px(2.0),
        NUMBER,
        colour,
        Align::Right,
    );
    let gap = c.px(8.0);
    let room = row.width - text_width(c, &number, NUMBER) - gap - c.px(8.0);
    let long = format!("{meaning} · below {cutoff:.0} Hz");
    let Some(words) = [long.as_str(), meaning]
        .into_iter()
        .find(|w| text_width(c, w, SMALL) <= room)
    else {
        return;
    };
    let dim = c.dim();
    let x = right - text_width(c, &number, NUMBER) - gap;
    c.text(words, x, row.y + c.px(5.0), SMALL, dim, Align::Right);
}
