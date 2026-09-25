//! The Cepstrum: the cepstrum of the mono sum as a filled curve, from the
//! shortest period (highest pitch, left) to the longest, labelled in the pitch
//! each period stands for, and a line on the pitch found with its frequency
//! and note.

use dasmeter_core::{CepstrumMeterSettings, CursorReadout, Role};

use super::labels::Align;
use super::shapes::Area;
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
        c.shapes.rect(
            Area {
                x: x - thin / 2.0,
                width: thin,
                ..plot
            },
            grid.faded(0.6),
        );
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
    for (i, &value) in values.iter().enumerate() {
        let y = y_of(value);
        let column = Area {
            x: plot.x + i as f32 * step,
            y,
            width: step.max(thin),
            height: (base - y).max(0.0),
        };
        c.shapes.rect(column, trace.faded(0.35));
    }
    let stroke = c.stroke(1.5);
    for (i, pair) in values.windows(2).enumerate() {
        let x = plot.x + (i as f32 + 0.5) * step;
        c.shapes
            .line([x, y_of(pair[0])], [x + step, y_of(pair[1])], stroke, trace);
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
    let mut readout = if pitch.frequency < 1_000.0 {
        format!("{:.1} Hz", pitch.frequency)
    } else {
        format!("{:.2} kHz", pitch.frequency / 1_000.0)
    };
    if let Some(note) = pitch.note {
        readout += &format!("  {note} {:+.0}¢", note.cents);
    }
    let width = c.px(11.0 * 0.6) * readout.chars().count() as f32;
    let start = if x + c.px(4.0) + width <= plot.right() {
        x + c.px(4.0)
    } else {
        (x - c.px(4.0) - width).max(plot.x)
    };
    c.text(&readout, start, plot.y + c.px(2.0), 11.0, text, Align::Left);
}
