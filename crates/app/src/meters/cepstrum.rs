//! The Cepstrum: the cepstrum of the mono sum as a filled curve, from the
//! shortest period (highest pitch, left) to the longest, labelled in the pitch
//! each period stands for, and a line on the pitch found with its frequency
//! and note. Under the pointer, the pitch of the period there.

use dasmeter_core::{CepstrumMeterSettings, Colour, CursorReadout, Role};

use super::labels::Align;
use super::shapes::{Area, smooth};
use super::{Canvas, map};

/// Pitches marked on the axis, in Hz.
const PITCH_MARKS: [(f32, &str); 10] = [
    (2_000.0, "2k"),
    (1_000.0, "1k"),
    (500.0, "500"),
    (300.0, "300"),
    (200.0, "200"),
    (150.0, "150"),
    (100.0, "100"),
    (70.0, "70"),
    (50.0, "50"),
    (30.0, "30"),
];

pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &CepstrumMeterSettings,
    values: &[f32],
    (short, long): (f32, f32),
    pitch: Option<&CursorReadout>,
    hover: Option<&CursorReadout>,
) {
    let (grid, dim) = (c.colour(Role::Grid), c.dim());
    let thin = c.px(1.0).max(1.0);
    let (plot, axis) = area.split_bottom(c.px(16.0));
    let x_of_period = |period: f32| map(period, (short, long), plot.x, plot.right());

    // The axis: periods, labelled as the pitch they stand for. A label that
    // would run into the one before it is left out.
    let mut last_label: Option<f32> = None;
    for (frequency, name) in PITCH_MARKS {
        let period = 1.0 / frequency;
        if period < short || period > long {
            continue;
        }
        let x = x_of_period(period);
        c.grid_v(plot, x, grid.faded(0.6), false);
        let half = c.px(9.0 * 0.6) * name.len() as f32 / 2.0;
        if last_label.is_none_or(|last| x - half > last + c.px(4.0)) {
            c.text(name, x, axis.y + c.px(2.0), 9.0, dim, Align::Centre);
            last_label = Some(x + half);
        }
    }

    // The curve, filled down to the baseline.
    let trace = c.colour(Role::CepstrumTrace);
    let base = plot.bottom();
    let top = plot.y + c.px(22.0);
    let y_of = |v: f32| base - v.clamp(0.0, 1.0) * (base - top);
    let step = plot.width / values.len().max(1) as f32;
    let points: Vec<[f32; 2]> = values
        .iter()
        .enumerate()
        .map(|(i, &value)| [plot.x + (i as f32 + 0.5) * step, y_of(value)])
        .collect();
    let curve = smooth(&points, c.px(2.0));
    let fill = trace.faded(0.35);
    c.edge_faded(plot, &curve, |c, piece, strength| {
        let fill = fill.faded(strength);
        c.fill_under(piece, base, fill, fill);
    });
    c.edge_faded(plot, &curve, |c, piece, strength| {
        c.glow(piece, trace.faded(strength));
    });
    c.edge_faded(plot, &curve, |c, piece, strength| {
        c.shapes
            .polyline(piece, c.stroke(1.5), trace.faded(strength));
    });

    if let Some(hover) = hover {
        hover_readout(c, plot, hover);
    }

    // The pitch found: a line at its period, its frequency and note on top.
    let text = c.colour(Role::Text);
    if !settings.show_pitch {
        return;
    }
    let Some(pitch) = pitch else {
        c.text(
            "No pitch",
            plot.x,
            plot.y + c.px(2.0),
            11.0,
            dim,
            Align::Left,
        );
        return;
    };
    let accent = c.colour(Role::Accent);
    let x = plot.x + pitch.x * plot.width;
    c.shapes.rect(
        Area {
            x: x - thin / 2.0,
            width: thin,
            ..plot
        },
        accent,
    );
    let readout = pitch_text(pitch);
    beside(c, plot, x, plot.y + c.px(2.0), 11.0, &readout, text);
}

/// "110.0 Hz  A2 +3¢".
fn pitch_text(at: &CursorReadout) -> String {
    let mut readout = if at.frequency < 1_000.0 {
        format!("{:.1} Hz", at.frequency)
    } else {
        format!("{:.2} kHz", at.frequency / 1_000.0)
    };
    if let Some(note) = at.note {
        readout += &format!("  {note} {:+.0}¢", note.cents);
    }
    readout
}

/// `text` at `y` just right of `x`, or left of it if it would run out of `plot`.
fn beside(c: &mut Canvas, plot: Area, x: f32, y: f32, size: f32, text: &str, colour: Colour) {
    let width = c.px(size * 0.6) * text.chars().count() as f32;
    let start = if x + c.px(4.0) + width <= plot.right() {
        x + c.px(4.0)
    } else {
        (x - c.px(4.0) - width).max(plot.x)
    };
    let pad = c.px(3.0);
    c.backdrop(Area {
        x: start - pad,
        y: y - pad,
        width: width + 2.0 * pad,
        height: c.px(size) * c.styling.text_scale + 2.0 * pad,
    });
    c.text(text, start, y, size, colour, Align::Left);
}

/// A faint line under the pointer, with the pitch of the period there
/// under the found pitch's readout.
fn hover_readout(c: &mut Canvas, plot: Area, hover: &CursorReadout) {
    let x = plot.x + hover.x.clamp(0.0, 1.0) * plot.width;
    let thin = c.px(1.0).max(1.0);
    let text = c.colour(Role::Text);
    c.overlay.rect(
        Area {
            x: x - thin / 2.0,
            width: thin,
            ..plot
        },
        text.faded(0.5),
    );
    let readout = pitch_text(hover);
    beside(c, plot, x, plot.y + c.px(18.0), 10.0, &readout, text);
}
