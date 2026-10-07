//! The Phase Scope: the waveform over one Cycle, the newest sharp and the
//! few before it fading behind, with a centre line, the beat lines of a bar,
//! and the tempo it follows. An Overlay Source is drawn over it in its own
//! colour, with the dotted sum of both and shading where they cancel.

use dasmeter_core::{
    Colour, PhaseScopeMeterSettings, PhaseScopeView, Role, ScopeOverlay, ScopeTrace,
};

use super::Canvas;
use super::labels::Align;
use super::shapes::Area;

/// Below this height (logical px) the tempo and correlation readouts are left
/// out, so the traces keep the room.
const MIN_TEXT_HEIGHT: f32 = 48.0;

pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &PhaseScopeMeterSettings,
    scope: &PhaseScopeView,
) {
    let (grid, dim) = (c.colour(Role::Grid), c.dim());
    let thin = c.px(1.0).max(1.0);
    let roomy = area.height >= c.px(MIN_TEXT_HEIGHT);
    let plot = area;

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
                envelope(c, band, old, colour.faded(fade), false);
            }
        }
        if t == 0
            && let Some(overlay) = &scope.overlay
        {
            draw_overlay(c, band, overlay, colour);
        }
        envelope(c, band, trace, colour, settings.filled);
    }

    if !roomy {
        return;
    }
    let mut readout = format!("{:.1} BPM", scope.tempo);
    if !scope.following_daw {
        readout += " · typed in";
    }
    if let Some(note) = &scope.note {
        readout = note.clone();
    }
    c.text(
        &readout,
        plot.x + c.px(2.0),
        plot.bottom() - c.px(13.0),
        9.0,
        dim,
        Align::Left,
    );
    if let Some(correlation) = scope.overlay.as_ref().and_then(|o| o.correlation) {
        let good = c.colour(Role::CorrelationPositive);
        let bad = c.colour(Role::CorrelationNegative);
        let colour = if correlation < 0.0 { bad } else { good };
        c.text(
            &format!("{correlation:+.2}"),
            plot.right() - c.px(2.0),
            plot.bottom() - c.px(15.0),
            11.0,
            colour,
            Align::Right,
        );
    }
}

/// One trace in its band: each column a bar from its lowest to its highest
/// value (at least a line thick), filled to the centre line if asked.
fn envelope(c: &mut Canvas, band: Area, trace: &ScopeTrace, colour: Colour, filled: bool) {
    let columns = trace.max.len().max(1);
    let step = band.width / columns as f32;
    let half = band.height / 2.0 * 0.95;
    let centre = band.y + band.height / 2.0;
    let line = c.stroke(1.5);
    for (i, (&low, &high)) in trace.min.iter().zip(&trace.max).enumerate() {
        let x = band.x + i as f32 * step;
        let (top, bottom) = (centre - high * half, centre - low * half);
        if filled {
            let (from, to) = (top.min(centre), bottom.max(centre));
            c.shapes.rect(
                Area {
                    x,
                    y: from,
                    width: step.max(1.0),
                    height: to - from,
                },
                colour.faded(0.3),
            );
        }
        let height = (bottom - top).max(line);
        let mid = (top + bottom) / 2.0;
        c.shapes.rect(
            Area {
                x,
                y: mid - height / 2.0,
                width: step.max(1.0),
                height,
            },
            colour,
        );
    }
}

/// The Overlay Source in its band: shading where the two cancel, its trace,
/// and the dotted sum.
fn draw_overlay(c: &mut Canvas, band: Area, overlay: &ScopeOverlay, main: Colour) {
    let columns = overlay.trace.max.len().max(1);
    let step = band.width / columns as f32;
    let half = band.height / 2.0 * 0.95;
    let centre = band.y + band.height / 2.0;
    let cancel = c.colour(Role::PhaseScopeCancel);
    for (i, &amount) in overlay.cancel.iter().enumerate() {
        if amount <= 0.0 {
            continue;
        }
        c.shapes.rect(
            Area {
                x: band.x + i as f32 * step,
                y: band.y,
                width: step.max(1.0),
                height: band.height,
            },
            cancel.faded(0.15 + 0.45 * amount.min(1.0)),
        );
    }
    let colour = overlay
        .colour
        .unwrap_or_else(|| super::mix(main, c.colour(Role::Text), 0.6));
    envelope(c, band, &overlay.trace, colour.faded(0.85), false);
    // The sum, dotted: every other few columns.
    let sum = c.colour(Role::PhaseScopeSum);
    let dot = c.stroke(2.0);
    for (i, (&low, &high)) in overlay.sum.min.iter().zip(&overlay.sum.max).enumerate() {
        if i % 6 >= 3 {
            continue;
        }
        let y = centre - (low + high) / 2.0 * half;
        c.overlay.rect(
            Area {
                x: band.x + i as f32 * step,
                y: y - dot / 2.0,
                width: step.max(1.0),
                height: dot,
            },
            sum,
        );
    }
}
