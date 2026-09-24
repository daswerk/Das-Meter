//! The Spectrum: a log-frequency axis, each trace as a line with soft fill or
//! as bars, the peak-hold curve, and the frequency and note under the cursor.

use dasmeter_analysis::{Spectrum, SpectrumStyle};
use dasmeter_core::{CursorReadout, Role, SpectrumMeterSettings};

use super::labels::Align;
use super::shapes::Area;
use super::{Canvas, map};

const FREQUENCY_MARKS: [(f32, &str); 10] = [
    (20.0, ""),
    (50.0, "50"),
    (100.0, "100"),
    (200.0, "200"),
    (500.0, "500"),
    (1_000.0, "1k"),
    (2_000.0, "2k"),
    (5_000.0, "5k"),
    (10_000.0, "10k"),
    (20_000.0, ""),
];

/// Horizontal grid lines every this many dB.
const DB_STEP: f32 = 12.0;

pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &SpectrumMeterSettings,
    spectrum: &Spectrum,
    range: (f32, f32),
    cursor: Option<&CursorReadout>,
) {
    let (grid, dim) = (c.colour(Role::Grid), c.dim());
    let thin = c.px(1.0).max(1.0);
    let (low, high) = (range.0.ln(), range.1.ln());
    let x_of = |f: f32| map(f.max(1.0).ln(), (low, high), area.x, area.right());
    let db_range = spectrum.db_range;
    let y_of = |db: f32| map(db, db_range, area.bottom(), area.y);

    // Grid.
    for (frequency, name) in FREQUENCY_MARKS {
        if frequency < range.0 || frequency > range.1 {
            continue;
        }
        let x = x_of(frequency);
        let line = Area {
            x: x - thin / 2.0,
            width: thin,
            ..area
        };
        c.shapes.rect(line, grid.faded(0.6));
        if !name.is_empty() {
            c.text(name, x, area.bottom() - c.px(13.0), 9.0, dim, Align::Centre);
        }
    }
    let mut db = (db_range.1 / DB_STEP).floor() * DB_STEP;
    while db > db_range.0 {
        let y = y_of(db);
        let line = Area {
            y: y - thin / 2.0,
            height: thin,
            ..area
        };
        c.shapes.rect(line, grid.faded(0.6));
        c.text(&format!("{db}"), area.x + c.px(2.0), y + c.px(1.0), 9.0, dim, Align::Left);
        db -= DB_STEP;
    }

    // Traces: the first in the line colour, a second (R or S) in the accent colour.
    let colours = [c.colour(Role::SpectrumLine), c.colour(Role::Accent)];
    let fill = c.colour(Role::SpectrumFill);
    let hold = c.colour(Role::SpectrumPeakHold);
    let frequencies = &spectrum.frequencies;
    for (trace, line) in spectrum.traces.iter().zip(colours) {
        let points = |levels: &[f32]| -> Vec<[f32; 2]> {
            frequencies
                .iter()
                .zip(levels)
                .map(|(&f, &db)| [x_of(f), y_of(db)])
                .collect()
        };
        match settings.analysis.style {
            SpectrumStyle::Line { .. } => {
                let curve = points(&trace.levels);
                let fill_top = if spectrum.traces.len() > 1 { line } else { fill };
                c.shapes
                    .fill_under(&curve, area.bottom(), fill_top.faded(0.45), fill_top.faded(0.05));
                c.shapes.polyline(&curve, c.px(1.5), line);
                if settings.show_peak_hold {
                    c.shapes
                        .polyline(&points(&trace.peak_hold), c.px(1.0), hold.faded(0.6));
                }
            }
            SpectrumStyle::Bars { .. } => {
                for (i, &f) in frequencies.iter().enumerate() {
                    // Each band spans halfway (in log frequency) to its neighbours.
                    let edge = |other: Option<&f32>| other.map_or(f, |&o| (f * o).sqrt());
                    let left = x_of(edge(i.checked_sub(1).and_then(|j| frequencies.get(j))));
                    let right = x_of(edge(frequencies.get(i + 1)));
                    let gap = c.px(1.0);
                    let (x, width) = (left + gap / 2.0, (right - left - gap).max(1.0));
                    let y = y_of(trace.levels[i]);
                    let bar = Area {
                        x,
                        y,
                        width,
                        height: area.bottom() - y,
                    };
                    c.shapes.gradient(bar, line.faded(0.8), line.faded(0.25));
                    if settings.show_peak_hold {
                        let y = y_of(trace.peak_hold[i]);
                        let tick = Area {
                            x,
                            y: y - c.px(1.0),
                            width,
                            height: c.px(1.5),
                        };
                        c.shapes.rect(tick, hold.faded(0.8));
                    }
                }
            }
        }
    }

    if let Some(cursor) = cursor {
        let x = area.x + cursor.x * area.width;
        let text = c.colour(Role::Text);
        let line = Area {
            x: x - thin / 2.0,
            width: thin,
            ..area
        };
        c.shapes.rect(line, text.faded(0.5));
        let mut readout = if cursor.frequency < 1_000.0 {
            format!("{:.0} Hz", cursor.frequency)
        } else {
            format!("{:.2} kHz", cursor.frequency / 1_000.0)
        };
        if let Some(note) = cursor.note {
            readout += &format!("  {note} {:+.0}¢", note.cents);
        }
        // Keep the readout inside the Meter: right of the line, or left near the edge.
        let (anchor, align) = if cursor.x < 0.6 {
            (x + c.px(4.0), Align::Left)
        } else {
            (x - c.px(4.0), Align::Right)
        };
        c.text(&readout, anchor, area.y, 11.0, text, align);
    }
}
