//! The Spectrum: a log-frequency axis, each trace as a line with soft fill or
//! as bars, the peak-hold curve, the loudest peak's readout pinned to the top
//! with a line to the peak, the box being dragged to zoom into, and the zoom
//! window showing that box larger and finer.

use dasmeter_analysis::{Spectrum, SpectrumStyle};
use dasmeter_core::{Colour, CursorReadout, Role, SpectrumMeterSettings, SpectrumZoom};

use super::labels::Align;
use super::shapes::Area;
use super::{Canvas, map};

/// About how wide `text` is in the monospace labels at `size`, in px.
fn text_width(c: &Canvas, text: &str, size: f32) -> f32 {
    c.px(size * 0.6) * text.chars().count() as f32
}

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

#[allow(clippy::too_many_arguments)]
pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &SpectrumMeterSettings,
    spectrum: &Spectrum,
    range: (f32, f32),
    cursor: Option<&CursorReadout>,
    peak: Option<&CursorReadout>,
    selecting: Option<[f32; 4]>,
    zoom: Option<&SpectrumZoom>,
) {
    // Labels under the zoom window would show through it: leave them out.
    let panel = zoom.map(|zoom| fraction_of(area, zoom.panel));
    c.covered = panel;
    let marks: Vec<(f32, String)> = FREQUENCY_MARKS
        .iter()
        .map(|&(frequency, name)| (frequency, name.to_owned()))
        .collect();
    plot(c, area, settings, spectrum, range, &marks, DB_STEP);

    let (low, high) = (range.0.ln(), range.1.ln());
    let x_of = |f: f32| map(f.max(1.0).ln(), (low, high), area.x, area.right());
    let db_range = spectrum.db_range;
    let y_of = |db: f32| map(db, db_range, area.bottom(), area.y);
    // The loudest peak's readout pinned to the top right, a leader line from
    // the peak to it; the cursor's line and readout under it.
    if let Some(peak) = peak {
        peak_readout(c, area, spectrum, peak, &x_of, &y_of);
    }
    if let Some(cursor) = cursor {
        let text = c.colour(Role::Text);
        marker(
            c,
            area,
            cursor,
            text.faded(0.5),
            usize::from(peak.is_some()),
        );
    }
    c.covered = None;

    // The box being dragged, or the one zoomed into.
    if let Some(selection) = selecting.or(zoom.map(|zoom| zoom.selection)) {
        outline(c, fraction_of(area, selection));
    }
    if let (Some(zoom), Some(panel)) = (zoom, panel) {
        draw_zoom(c, panel, settings, zoom);
    }
}

/// `fractions` (left, top, right, bottom) of `area`.
fn fraction_of(area: Area, [left, top, right, bottom]: [f32; 4]) -> Area {
    Area {
        x: area.x + left * area.width,
        y: area.y + top * area.height,
        width: (right - left) * area.width,
        height: (bottom - top) * area.height,
    }
}

/// A box drawn as a thin accent frame over a faint fill.
fn outline(c: &mut Canvas, area: Area) {
    let accent = c.colour(Role::Accent);
    let thin = c.px(1.0).max(1.0);
    c.shapes.rect(area, accent.faded(0.03));
    for edge in [
        Area {
            height: thin,
            ..area
        },
        Area {
            y: area.bottom() - thin,
            height: thin,
            ..area
        },
        Area {
            width: thin,
            ..area
        },
        Area {
            x: area.right() - thin,
            width: thin,
            ..area
        },
    ] {
        c.shapes.rect(edge, accent.faded(0.8));
    }
}

/// The zoom window: a title bar with the box's ranges and a close button,
/// and the box's Spectrum under it with its own scale.
fn draw_zoom(c: &mut Canvas, panel: Area, settings: &SpectrumMeterSettings, zoom: &SpectrumZoom) {
    let (text, dim) = (c.colour(Role::Text), c.dim());
    let opaque = |colour: Colour| Colour { a: 255, ..colour };
    c.shapes
        .rounded_rect(panel, c.px(6.0), opaque(c.colour(Role::Grid)));
    let inside = panel.inset(c.px(1.0));
    c.shapes
        .rounded_rect(inside, c.px(5.0), opaque(c.colour(Role::Panel)));

    let title_height = c.px(dasmeter_core::meters::ZOOM_TITLE_HEIGHT);
    let (title, body) = inside.split_top(title_height);
    let db_range = zoom.spectrum.db_range;
    let label = format!(
        "{} – {}   {:.0} to {:.0} dB",
        hertz(zoom.range.0),
        hertz(zoom.range.1),
        db_range.0,
        db_range.1
    );
    let close = Area {
        x: title.right() - title_height,
        width: title_height,
        ..title
    };
    let fits = text_width(c, &label, 10.0) < title.width - title_height - c.px(12.0);
    if fits {
        c.text(
            &label,
            title.x + c.px(8.0),
            title.y + c.px(4.0),
            10.0,
            dim,
            Align::Left,
        );
    }
    c.shapes
        .rect(close.inset(c.px(3.0)), c.colour(Role::Grid).faded(0.6));
    c.text(
        "×",
        close.x + close.width / 2.0,
        close.y + c.px(2.0),
        13.0,
        text,
        Align::Centre,
    );

    let plot_area = Area {
        x: body.x + c.px(4.0),
        width: (body.width - c.px(8.0)).max(1.0),
        height: (body.height - c.px(4.0)).max(1.0),
        ..body
    };
    let marks = frequency_marks(zoom.range);
    let span = db_range.1 - db_range.0;
    let step = match span {
        s if s >= 48.0 => 12.0,
        s if s >= 24.0 => 6.0,
        s if s >= 12.0 => 3.0,
        s if s >= 5.0 => 1.0,
        _ => 0.5,
    };
    plot(
        c,
        plot_area,
        settings,
        &zoom.spectrum,
        zoom.range,
        &marks,
        step,
    );
}

/// "440 Hz" or "2.35 kHz".
fn hertz(frequency: f32) -> String {
    if frequency < 1_000.0 {
        format!("{frequency:.0} Hz")
    } else {
        format!("{:.2} kHz", frequency / 1_000.0)
    }
}

/// Grid marks across a zoomed frequency range: 1-2-5 per decade over a wide
/// range, every whole number per decade over a narrower one, and evenly
/// spaced round steps over a narrow one.
fn frequency_marks((low, high): (f32, f32)) -> Vec<(f32, String)> {
    let label = |f: f32, step: f32| {
        if f >= 1_000.0 {
            let decimals = if step >= 1_000.0 {
                0
            } else if step >= 100.0 {
                1
            } else {
                2
            };
            format!("{:.*}k", decimals, f / 1_000.0)
        } else if step >= 1.0 {
            format!("{f:.0}")
        } else {
            format!("{f:.1}")
        }
    };
    let mut marks = Vec::new();
    if high / low >= 3.0 {
        let multiples: &[f32] = if high / low >= 30.0 {
            &[1.0, 2.0, 5.0]
        } else {
            &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0]
        };
        let mut decade = 10f32.powf(low.log10().floor());
        while decade <= high {
            for &m in multiples {
                let f = m * decade;
                if (low..=high).contains(&f) {
                    marks.push((f, label(f, decade)));
                }
            }
            decade *= 10.0;
        }
    } else {
        // About six round steps across.
        let rough = (high - low) / 6.0;
        let power = 10f32.powf(rough.log10().floor());
        let step = [1.0, 2.0, 5.0, 10.0]
            .into_iter()
            .map(|m| m * power)
            .find(|&s| s >= rough)
            .unwrap_or(10.0 * power);
        let mut f = (low / step).ceil() * step;
        while f <= high {
            marks.push((f, label(f, step)));
            f += step;
        }
    }
    marks
}

/// The grid, its labels and the traces of `spectrum` in `area`.
fn plot(
    c: &mut Canvas,
    area: Area,
    settings: &SpectrumMeterSettings,
    spectrum: &Spectrum,
    range: (f32, f32),
    marks: &[(f32, String)],
    db_step: f32,
) {
    let (grid, dim) = (c.colour(Role::Grid), c.dim());
    let thin = c.px(1.0).max(1.0);
    let (low, high) = (range.0.ln(), range.1.ln());
    let x_of = |f: f32| map(f.max(1.0).ln(), (low, high), area.x, area.right());
    let db_range = spectrum.db_range;
    let y_of = |db: f32| map(db, db_range, area.bottom(), area.y);

    // Grid. A label that would run into the one before it is left out.
    let mut last_label: Option<f32> = None;
    for (frequency, name) in marks {
        let frequency = *frequency;
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
        let half = text_width(c, name, 9.0) / 2.0;
        let clear = last_label.is_none_or(|last| x - half > last + c.px(4.0));
        let inside = x - half > area.x + c.px(26.0) && x + half < area.right();
        if !name.is_empty() && clear && inside {
            c.text(name, x, area.bottom() - c.px(13.0), 9.0, dim, Align::Centre);
            last_label = Some(x + half);
        }
    }
    let mut last_db_label: Option<f32> = None;
    let mut db = (db_range.1 / db_step).floor() * db_step;
    while db > db_range.0 {
        let y = y_of(db);
        let line = Area {
            y: y - thin / 2.0,
            height: thin,
            ..area
        };
        c.shapes.rect(line, grid.faded(0.6));
        // Right-aligned so "0" lines up with "-12"; none so low that it runs
        // into the frequency labels.
        let crowded = last_db_label.is_some_and(|last| y - last < c.px(11.0));
        if y + c.px(14.0) < area.bottom() - c.px(13.0) && !crowded {
            last_db_label = Some(y);
            c.text(
                &format!("{db}"),
                area.x + c.px(22.0),
                y + c.px(1.0),
                9.0,
                dim,
                Align::Right,
            );
        }
        db -= db_step;
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
                let fill_top = if spectrum.traces.len() > 1 {
                    line
                } else {
                    fill
                };
                c.shapes.fill_under(
                    &curve,
                    area.bottom(),
                    fill_top.faded(0.45),
                    fill_top.faded(0.05),
                );
                c.shapes.polyline(&curve, c.stroke(1.5), line);
                if settings.show_peak_hold {
                    c.shapes
                        .polyline(&points(&trace.peak_hold), c.stroke(1.0), hold.faded(0.6));
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
}

/// "−15.5 dBFS @ 40.0 Hz  D#1 +12¢": fixed widths, so the digits change in
/// place instead of the text shifting as the peak moves.
fn peak_text(peak: &CursorReadout) -> String {
    let level = peak
        .level
        .map_or_else(|| "    --".to_owned(), |db| format!("{db:6.1}"));
    let frequency = if peak.frequency < 1_000.0 {
        format!("{:6.1} Hz ", peak.frequency)
    } else {
        format!("{:6.2} kHz", peak.frequency / 1_000.0)
    };
    let note = peak.note.map_or_else(String::new, |note| {
        format!("  {:<3} {:+3.0}¢", note.to_string(), note.cents)
    });
    format!("{level} dBFS @ {frequency}{note}")
}

/// The peak's readout at the top right, and a thin line from the top of the
/// peak (as drawn) to the readout.
fn peak_readout(
    c: &mut Canvas,
    area: Area,
    spectrum: &Spectrum,
    peak: &CursorReadout,
    x_of: &impl Fn(f32) -> f32,
    y_of: &impl Fn(f32) -> f32,
) {
    let accent = c.colour(Role::Accent);
    let text = c.colour(Role::Text);
    // The whole readout, else shorter ones, as the Meter's width allows.
    let full = peak_text(peak);
    let short = full.split("  ").next().unwrap_or(&full).to_owned();
    let frequency = full.split(" @ ").nth(1).unwrap_or(&full).trim().to_owned();
    let room = area.width - c.px(12.0);
    let fitting = [full, short, frequency]
        .into_iter()
        .map(|t| (text_width(c, &t, 11.0) * c.styling.text_scale, t))
        .find(|(width, _)| *width <= room);
    let right = area.right() - c.px(6.0);
    let top = area.y + c.px(4.0);
    let width = match fitting {
        Some((width, readout)) => {
            c.text(&readout, right, top, 11.0, text, Align::Right);
            width
        }
        None => 0.0,
    };

    // The peak as drawn: the loudest trace at the drawn point nearest the
    // peak's frequency, so the line meets the bar or curve, not a raw bin.
    let nearest = spectrum
        .frequencies
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            let distance = |f: f32| (f.max(1.0) / peak.frequency.max(1.0)).ln().abs();
            distance(**a).total_cmp(&distance(**b))
        })
        .map(|(i, _)| i);
    let drawn = nearest.and_then(|i| {
        spectrum
            .traces
            .iter()
            .filter_map(|t| t.levels.get(i).copied())
            .reduce(f32::max)
    });
    let level = drawn.or(peak.level).unwrap_or(spectrum.db_range.0);
    let from = [
        x_of(peak.frequency),
        y_of(level.clamp(spectrum.db_range.0, spectrum.db_range.1)),
    ];
    // To the middle of the readout's underside, kept clear of the text.
    let to = [
        (right - width / 2.0).max(area.x),
        top + c.px(11.0) * c.styling.text_scale + c.px(4.0),
    ];
    let thin = c.px(1.0).max(1.0);
    if width > 0.0 {
        c.shapes.line(from, to, thin, text.faded(0.6));
    }
    // A tick on the peak in the accent colour.
    let tick = Area {
        x: from[0] - c.px(5.0),
        y: from[1] - c.px(1.0),
        width: c.px(10.0),
        height: c.px(2.0),
    };
    c.shapes.rect(tick, accent);
}

/// A vertical line at a frequency, with its frequency and note on text row
/// `row` from the top, kept inside the Meter.
fn marker(c: &mut Canvas, area: Area, at: &CursorReadout, line_colour: Colour, row: usize) {
    let thin = c.px(1.0).max(1.0);
    let x = area.x + at.x * area.width;
    let line = Area {
        x: x - thin / 2.0,
        width: thin,
        ..area
    };
    c.shapes.rect(line, line_colour);
    let mut readout = if at.frequency < 1_000.0 {
        format!("{:.0} Hz", at.frequency)
    } else {
        format!("{:.2} kHz", at.frequency / 1_000.0)
    };
    if let Some(note) = at.note {
        readout += &format!("  {note} {:+.0}¢", note.cents);
    }
    // Right of the line, else left of it, else against whichever edge it would cross.
    let width = text_width(c, &readout, 11.0);
    let right = x + c.px(4.0);
    let left = x - c.px(4.0) - width;
    let start = if right + width <= area.right() {
        right
    } else if left >= area.x {
        left
    } else {
        (area.right() - width).max(area.x)
    };
    // Under the peak readout when there is one.
    let text = c.colour(Role::Text);
    let y = area.y + c.px(12.0) + row as f32 * c.px(16.0);
    c.text(&readout, start, y, 11.0, text, Align::Left);
}
