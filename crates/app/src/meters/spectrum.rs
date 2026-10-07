//! The Spectrum: a log-frequency axis, each trace as a line with soft fill or
//! as bars (steady harmonics glowing brighter, if it's coloured so), the
//! peak-hold curve, the loudest standing peak's readout pinned to the top
//! with a line to the peak, the held peaks marked while the pointer is over
//! it, the box being dragged to zoom into, and the zoom window showing that
//! box larger and finer, with the pitch under the pointer.

use dasmeter_analysis::{Spectrum, SpectrumStyle, note_name};
use dasmeter_core::{Colour, CursorReadout, Role, SpectrumMeterSettings, SpectrumZoom};

use super::labels::Align;
use super::shapes::{Area, smooth};
use super::{Canvas, map, mix};

/// The readouts and marks over a Spectrum.
pub struct Marks<'a> {
    pub cursor: Option<&'a CursorReadout>,
    pub peak: Option<&'a CursorReadout>,
    /// Marked while the pointer is over the Spectrum.
    pub held_peaks: &'a [CursorReadout],
    /// Where the pointer is, as fractions of the Meter's frame.
    pub pointer: Option<[f32; 2]>,
}

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

/// `meter` is the Meter's whole frame (where `marks.pointer` is measured),
/// `area` the plot inside it.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    c: &mut Canvas,
    meter: Area,
    area: Area,
    settings: &SpectrumMeterSettings,
    spectrum: &Spectrum,
    range: (f32, f32),
    marks: Marks,
    selecting: Option<[f32; 4]>,
    zoom: Option<&SpectrumZoom>,
) {
    let Marks {
        cursor,
        peak,
        held_peaks,
        pointer,
    } = marks;
    let pointer = pointer.map(|[x, y]| [meter.x + x * meter.width, meter.y + y * meter.height]);
    // Labels under the zoom window would show through it: leave them out.
    let panel = zoom.map(|zoom| fraction_of(area, zoom.panel));
    c.covered = panel;
    let grid_marks: Vec<(f32, String)> = FREQUENCY_MARKS
        .iter()
        .map(|&(frequency, name)| (frequency, name.to_owned()))
        .collect();
    // While it holds its peaks (under the pointer), the held curve shows.
    let show_hold = settings.show_peak_hold || !held_peaks.is_empty();
    plot(
        c,
        area,
        settings,
        spectrum,
        range,
        &grid_marks,
        DB_STEP,
        show_hold,
    );

    let (low, high) = (range.0.ln(), range.1.ln());
    let x_of = |f: f32| map(f.max(1.0).ln(), (low, high), area.x, area.right());
    let db_range = spectrum.db_range;
    let y_of = |db: f32| map(db, db_range, area.bottom(), area.y);
    // The loudest peak's readout pinned to the top right, a leader line from
    // the peak to it; the cursor's line and readout under it.
    if let Some(peak) = peak {
        peak_readout(c, area, spectrum, peak, &x_of, &y_of);
    }
    if !held_peaks.is_empty() {
        let colour = c.colour(Role::SpectrumPeakDots);
        held_marks(c, area, held_peaks, pointer, colour, &x_of, &y_of);
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
        draw_zoom(c, panel, settings, zoom, pointer);
    }
}

/// The held peaks, each a dot with its note; the one nearest the pointer
/// also with its frequency, cents and level.
fn held_marks(
    c: &mut Canvas,
    area: Area,
    held: &[CursorReadout],
    pointer: Option<[f32; 2]>,
    dot_colour: Colour,
    x_of: &impl Fn(f32) -> f32,
    y_of: &impl Fn(f32) -> f32,
) {
    let (text, panel) = (c.colour(Role::Text), c.colour(Role::Panel));
    let nearest = pointer.and_then(|[px, _]| {
        held.iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                (x_of(a.frequency) - px)
                    .abs()
                    .total_cmp(&(x_of(b.frequency) - px).abs())
            })
            .map(|(i, _)| i)
    });
    // Dots first; then the labels, the nearest's first, each above its dot
    // (or a row higher, or below near the top) where it runs into none
    // placed before it, else left out.
    let mut order: Vec<usize> = (0..held.len()).collect();
    order.sort_by_key(|&i| (Some(i) != nearest, i));
    let mut placed: Vec<Area> = Vec::new();
    for &i in &order {
        let peak = &held[i];
        let level = peak.level.unwrap_or(-200.0);
        let (x, y) = (x_of(peak.frequency), y_of(level));
        let size = c.px(if Some(i) == nearest { 7.0 } else { 5.0 });
        let dot = Area {
            x: x - size / 2.0,
            y: y - size / 2.0,
            width: size,
            height: size,
        };
        c.shapes.rounded_rect(dot.inset(-c.px(1.0)), size, panel);
        c.shapes.rounded_rect(dot, size / 2.0, dot_colour);
        let label = if Some(i) == nearest {
            let mut label = hertz(peak.frequency);
            if let Some(note) = peak.note {
                label += &format!("  {note} {:+.0}¢", note.cents);
            }
            label + &format!("  {level:.1} dB")
        } else {
            peak.note
                .map_or_else(|| hertz(peak.frequency), |n| n.to_string())
        };
        let width = text_width(c, &label, 10.0) * c.styling.text_scale;
        let height = c.px(12.0) * c.styling.text_scale;
        // Clear of the dB labels at the left.
        let left = (x - width / 2.0)
            .max(area.x + c.px(26.0))
            .min(area.right() - width - c.px(2.0));
        let rows = [
            y - c.px(18.0),
            y - c.px(18.0) - height - c.px(2.0),
            y + c.px(6.0),
        ];
        let spot = rows
            .into_iter()
            .filter(|&top| top > area.y + c.px(30.0) && top + height < area.bottom())
            .map(|top| Area {
                x: left - c.px(3.0),
                y: top,
                width: width + c.px(6.0),
                height,
            })
            .find(|spot| {
                placed.iter().all(|other| {
                    spot.right() <= other.x
                        || spot.x >= other.right()
                        || spot.bottom() <= other.y
                        || spot.y >= other.bottom()
                })
            });
        let Some(spot) = spot else { continue };
        placed.push(spot);
        let colour = if Some(i) == nearest {
            text
        } else {
            text.faded(0.75)
        };
        c.text(&label, left, spot.y, 10.0, colour, Align::Left);
    }
}

/// `fractions` (left, top, right, bottom) of `area`.
pub(super) fn fraction_of(area: Area, [left, top, right, bottom]: [f32; 4]) -> Area {
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
fn draw_zoom(
    c: &mut Canvas,
    panel: Area,
    settings: &SpectrumMeterSettings,
    zoom: &SpectrumZoom,
    pointer: Option<[f32; 2]>,
) {
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
        settings.show_peak_hold,
    );

    let inside = pointer.filter(|&[x, y]| {
        x >= plot_area.x && x <= plot_area.right() && y >= plot_area.y && y <= plot_area.bottom()
    });
    // Its steadiest peaks, marked as on the Spectrum under the pointer.
    if !zoom.peaks.is_empty() {
        let (low, high) = (zoom.range.0.ln(), zoom.range.1.ln());
        let x_of = |f: f32| map(f.max(1.0).ln(), (low, high), plot_area.x, plot_area.right());
        let y_of = |db: f32| map(db, db_range, plot_area.bottom(), plot_area.y);
        let colour = c.colour(Role::SpectrumPeakDots);
        held_marks(c, plot_area, &zoom.peaks, inside, colour, &x_of, &y_of);
    }

    // The pitch under the pointer: a line, and its frequency, note and level.
    let Some([x, y]) = inside else { return };
    let along = (x - plot_area.x) / plot_area.width;
    let frequency = zoom.range.0 * (zoom.range.1 / zoom.range.0).powf(along);
    let level = map(y, (plot_area.bottom(), plot_area.y), db_range.0, db_range.1);
    let thin = c.px(1.0).max(1.0);
    c.shapes.rect(
        Area {
            x: x - thin / 2.0,
            width: thin,
            ..plot_area
        },
        text.faded(0.5),
    );
    let mut readout = format!("{:.1} Hz", frequency);
    if frequency >= 1_000.0 {
        readout = format!("{:.3} kHz", frequency / 1_000.0);
    }
    if let Some(note) = note_name(frequency) {
        readout += &format!("  {note} {:+.0}¢", note.cents);
    }
    readout += &format!("  {level:.1} dB");
    let width = text_width(c, &readout, 11.0) * c.styling.text_scale;
    let right_of = x + c.px(6.0);
    let start = if right_of + width <= plot_area.right() {
        right_of
    } else {
        (x - c.px(6.0) - width).max(plot_area.x)
    };
    let back = Area {
        x: start - c.px(3.0),
        y: plot_area.y + c.px(3.0),
        width: width + c.px(6.0),
        height: c.px(15.0) * c.styling.text_scale,
    };
    let opaque = Colour {
        a: 230,
        ..c.colour(Role::Panel)
    };
    c.shapes.rounded_rect(back, c.px(3.0), opaque);
    c.text(
        &readout,
        start,
        plot_area.y + c.px(4.0),
        11.0,
        text,
        Align::Left,
    );
}

/// "440 Hz" or "2.35 kHz".
pub(super) fn hertz(frequency: f32) -> String {
    if frequency < 1_000.0 {
        format!("{frequency:.0} Hz")
    } else {
        format!("{:.2} kHz", frequency / 1_000.0)
    }
}

/// Grid marks across a zoomed frequency range: 1-2-5 per decade over a wide
/// range, every whole number per decade over a narrower one, and evenly
/// spaced round steps over a narrow one.
pub(super) fn frequency_marks((low, high): (f32, f32)) -> Vec<(f32, String)> {
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
#[allow(clippy::too_many_arguments)]
fn plot(
    c: &mut Canvas,
    area: Area,
    settings: &SpectrumMeterSettings,
    spectrum: &Spectrum,
    range: (f32, f32),
    marks: &[(f32, String)],
    db_step: f32,
    show_hold: bool,
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

    // Traces: the first in the line colour, a second (R or S) in its own.
    let colours = [c.colour(Role::SpectrumLine), c.colour(Role::SpectrumSecond)];
    let fill = c.colour(Role::SpectrumFill);
    let hold = c.colour(Role::SpectrumPeakHold);
    // Steady harmonics glow in their own colour.
    let glow = c.colour(Role::SpectrumHarmonics);
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
                let raw = points(&trace.levels);
                let curve = smooth(&raw, c.px(2.0));
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
                if trace.steadiness.len() == raw.len() {
                    let stroke = c.stroke(2.5);
                    for (i, pair) in raw.windows(2).enumerate() {
                        let steady = (trace.steadiness[i] + trace.steadiness[i + 1]) / 2.0;
                        if steady < 0.05 {
                            continue;
                        }
                        let ([x0, y0], [x1, y1]) = (pair[0], pair[1]);
                        c.shapes.quad(
                            [[x0, y0], [x1, y1], [x0, area.bottom()], [x1, area.bottom()]],
                            glow.faded(0.7 * steady),
                            glow.faded(0.08 * steady),
                        );
                        c.shapes
                            .line(pair[0], pair[1], stroke, mix(line, glow, steady));
                    }
                }
                if show_hold {
                    let held = smooth(&points(&trace.peak_hold), c.px(2.0));
                    c.shapes.polyline(&held, c.stroke(1.0), hold.faded(0.6));
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
                    let colour = trace
                        .steadiness
                        .get(i)
                        .map_or(line, |&steady| mix(line, glow, steady));
                    c.shapes
                        .gradient(bar, colour.faded(0.8), colour.faded(0.25));
                    if show_hold {
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
