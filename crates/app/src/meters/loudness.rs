//! The Loudness Meter: numbers for M, S, I, LRA, true peak, PLR and PSR; L/R
//! RMS bars with sample-peak lines and hold ticks; a LUFS bar with the target
//! line; and, where there's room, the loudness graph. With the bars off (the
//! default) one loudness reading and the sample peak are shown large, and the
//! other readings as plain rows.

use dasmeter_core::{
    BigReading, Colour, Level, LoudnessDisplay, LoudnessMeterSettings, LufsBar, Role,
};

use super::labels::Align;
use super::shapes::Area;
use super::{Canvas, map};

/// A number tile's height and the narrowest one, in logical px.
const TILE_HEIGHT: f32 = 26.0;
/// The lowest a tile gets before readings are left out.
const MIN_TILE_HEIGHT: f32 = 20.0;
/// The narrowest single tile, beside the bars in a small, wide area.
const SINGLE_TILE_WIDTH: f32 = 110.0;
/// Below this height the tiles shrink to the single reading the bar shows.
const TINY_HEIGHT: f32 = 64.0;
/// The rate line under the tiles.
const RATE_HEIGHT: f32 = 16.0;
const TILE_MIN_WIDTH: f32 = 150.0;
/// A reading's row in the numbers-only layout: name, value and unit.
const ROW_WIDTH: f32 = 160.0;
/// The least width the big numbers keep beside the rows.
const MIN_HEROES_WIDTH: f32 = 150.0;
/// A big number's height with its caption, in multiples of its size.
const BIG_BLOCK: f32 = 1.55;
/// The tallest the loudness graph gets in the numbers-only layout.
const MAX_GRAPH_HEIGHT: f32 = 160.0;
/// The least room between two scale labels.
const LABEL_SPACING: f32 = 11.0;
/// The widest a bar gets.
const MAX_BAR_WIDTH: f32 = 26.0;
/// The smallest loudness graph worth drawing.
const MIN_GRAPH_WIDTH: f32 = 120.0;
const MIN_GRAPH_HEIGHT: f32 = 48.0;

/// Scale marks on the bars, in dB, kept if inside the bar range.
const MARKS: [f32; 10] = [
    0.0, -6.0, -12.0, -18.0, -24.0, -30.0, -36.0, -48.0, -60.0, -72.0,
];

pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &LoudnessMeterSettings,
    display: &LoudnessDisplay,
) {
    let (text, dim) = (c.colour(Role::Text), c.dim());
    let size = 13.0;

    // Numbers, each in its own tile: name, value and unit on one line.
    let true_peak = if display.true_peak_oversampled {
        "TP"
    } else {
        "Peak"
    };
    let rows = [
        Some(("M", display.momentary, "LUFS")),
        Some(("S", display.short_term, "LUFS")),
        Some(("I", display.integrated, "LUFS")),
        settings.show_range.then_some(("LRA", display.range, "LU")),
        settings
            .show_true_peak
            .then_some((true_peak, display.true_peak_max, "dBTP")),
        settings
            .show_peak_to_loudness
            .then_some(("PLR", display.plr, "LU")),
        settings
            .show_peak_to_loudness
            .then_some(("PSR", display.psr, "LU")),
    ];
    let rows: Vec<_> = rows.into_iter().flatten().collect();
    let rate = format!("{:.1} kHz", f64::from(display.sample_rate) / 1000.0);
    if !settings.show_bars {
        draw_numbers(c, area, settings, display, &rows);
        return;
    }

    // The bars take only the width they need: the scale, three bars and gaps.
    let gap = c.px(6.0);
    let scale_width = c.px(28.0);
    let bars_width = scale_width + 3.0 * c.px(MAX_BAR_WIDTH) + 4.0 * gap;
    // Wider than tall (a Bar along the top or bottom): tiles on the left, bars
    // on the right at full height. Otherwise tiles above, bars below.
    let beside = area.width >= area.height && area.width >= c.px(TILE_MIN_WIDTH) + bars_width
        // A short, wide area keeps its bars beside a single narrower tile.
        || (area.width >= 1.2 * area.height
            && area.width >= c.px(SINGLE_TILE_WIDTH) + bars_width);
    let graph_wanted = settings.show_history;
    let mut graph: Option<Area> = None;
    let (tiles, bars) = if beside {
        let tiles = Area {
            width: area.width - bars_width - gap,
            ..area
        };
        let bars = Area {
            x: area.right() - bars_width,
            width: bars_width,
            ..area
        };
        (tiles, bars)
    } else {
        // At most half the height for tiles; the bars keep the rest.
        let columns = tile_columns(c, area.width, rows.len());
        let tile_rows = rows.len().div_ceil(columns) as f32;
        let wanted = tile_rows * (c.px(TILE_HEIGHT) + gap) + c.px(RATE_HEIGHT);
        let (tiles, row) = area.split_top(wanted.min(area.height / 2.0));
        let width = bars_width.min(row.width);
        let graph_width = row.width - width - gap;
        let bars = if graph_wanted
            && graph_width >= c.px(MIN_GRAPH_WIDTH)
            && row.height - gap >= c.px(MIN_GRAPH_HEIGHT)
        {
            // The loudness graph fills the width beside the bars, which go right.
            graph = Some(Area {
                x: row.x,
                y: row.y + gap,
                width: graph_width,
                height: row.height - gap - c.px(16.0),
            });
            Area {
                x: row.right() - width,
                y: row.y + gap,
                width,
                height: (row.height - gap).max(0.0),
            }
        } else {
            // The bar group sits in the middle of the width left under the tiles.
            Area {
                x: row.x + (row.width - width) / 2.0,
                y: row.y + gap,
                width,
                height: (row.height - gap).max(0.0),
            }
        };
        (tiles, bars)
    };

    // As many readings as fit, most important first; shown in their usual order.
    let columns = tile_columns(c, tiles.width, rows.len());
    let min_tile = c.px(MIN_TILE_HEIGHT);
    let fit_rows = ((tiles.height + gap) / (min_tile + gap)).floor().max(0.0) as usize;
    // A very short area (a thin Bar) shows just the one reading the bar shows.
    // So does one too narrow for a tile at its narrowest.
    let capacity = if tiles.height < c.px(TINY_HEIGHT) || tiles.width < c.px(TILE_MIN_WIDTH) {
        fit_rows.min(1)
    } else {
        (fit_rows * columns).min(rows.len())
    };
    let bar_name = match settings.lufs_bar {
        LufsBar::ShortTerm => "S",
        LufsBar::Momentary => "M",
    };
    let mut shown: Vec<usize> = (0..rows.len()).collect();
    shown.sort_by_key(|&i| priority(rows[i].0, bar_name));
    shown.truncate(capacity);
    shown.sort_unstable();
    let tile_rows = shown.len().div_ceil(columns).max(1);
    // The rate line goes under the tiles only where there's room left for it.
    let room = tiles.height - tile_rows as f32 * (min_tile + gap);
    let show_rate = !shown.is_empty() && room >= c.px(RATE_HEIGHT);
    let for_tiles = tiles.height - if show_rate { c.px(RATE_HEIGHT) } else { 0.0 };
    let tile_height = ((for_tiles - gap * (tile_rows as f32 - 1.0)) / tile_rows as f32)
        .clamp(min_tile, c.px(TILE_HEIGHT));
    let tile_width = (tiles.width - gap * (columns as f32 - 1.0)) / columns as f32;
    let tile_fill = c.colour(Role::Grid).faded(0.35);
    let unit_width = c.px(36.0);
    let pad = c.px(8.0);
    for (slot, &i) in shown.iter().enumerate() {
        let (name, level, unit) = rows[i];
        let (row, column) = (slot / columns, slot % columns);
        let tile = Area {
            x: tiles.x + column as f32 * (tile_width + gap),
            y: tiles.y + row as f32 * (tile_height + gap),
            width: tile_width,
            height: tile_height,
        };
        c.shapes.rect(tile, tile_fill);
        let y = tile.y + (tile_height - c.px(16.0)) / 2.0;
        c.text(name, tile.x + pad, y, size, dim, Align::Left);
        let unit_x = tile.right() - pad - unit_width;
        c.bold(
            &number(level),
            unit_x - c.px(4.0),
            y,
            size,
            text,
            Align::Right,
        );
        c.text(unit, unit_x, y + c.px(2.0), 10.0, dim, Align::Left);
    }
    let below = tiles.y + tile_rows as f32 * (tile_height + gap);
    if show_rate {
        c.text(&rate, tiles.x, below, 10.0, dim, Align::Left);
    }
    // Beside the bars (a wide area), the graph takes the room under the tiles.
    if beside && graph_wanted && !shown.is_empty() {
        let top = below + if show_rate { c.px(RATE_HEIGHT) } else { 0.0 };
        let room = Area {
            y: top,
            height: tiles.bottom() - top - c.px(16.0),
            ..tiles
        };
        if room.width >= c.px(MIN_GRAPH_WIDTH) && room.height >= c.px(MIN_GRAPH_HEIGHT) {
            graph = Some(room);
        }
    }
    if let Some(graph) = graph {
        draw_graph(c, graph, settings, display);
    }

    // Bars: L, R, then the LUFS bar.
    let (bars, names) = bars.split_bottom(c.px(16.0));
    let range = (settings.bar_range.0 as f32, settings.bar_range.1 as f32);
    let y_of = |db: f64| map(db as f32, range, bars.bottom(), bars.y);
    let left = bars.x + scale_width;
    let width = ((bars.right() - left - 3.0 * gap) / 3.0)
        .min(c.px(MAX_BAR_WIDTH))
        .max(1.0);
    let grid = c.colour(Role::Grid);
    let thin = c.px(1.0).max(1.0);

    // Labels from the top down, skipping any that would crowd the one above.
    let mut last_label: Option<f32> = None;
    for db in MARKS
        .into_iter()
        .filter(|&db| db >= range.0 && db <= range.1)
    {
        let y = y_of(f64::from(db));
        if last_label.is_some_and(|last| y - last < c.px(LABEL_SPACING)) {
            continue;
        }
        last_label = Some(y);
        // Right-aligned against the bars, so "0" lines up with "-60".
        c.text(
            &format!("{db}"),
            left - c.px(6.0),
            y - c.px(6.0),
            9.0,
            dim,
            Align::Right,
        );
        let tick = Area {
            x: left - c.px(3.0),
            y,
            width: 3.0 * width + 3.0 * gap + c.px(3.0),
            height: thin,
        };
        c.shapes.rect(tick, grid.faded(0.6));
    }

    let (bar, peak) = (c.colour(Role::LoudnessBar), c.colour(Role::LoudnessPeak));
    let track = c.colour(Role::Background);
    let column = |i: usize| left + i as f32 * (width + gap) + if i == 2 { gap } else { 0.0 };
    for (i, (name, levels)) in [("L", display.left), ("R", display.right)]
        .into_iter()
        .enumerate()
    {
        let x = column(i);
        c.shapes.rect(Area { x, width, ..bars }, track);
        if let Some(db) = levels.rms.db() {
            let y = y_of(db);
            let fill = Area {
                x,
                y,
                width,
                height: bars.bottom() - y,
            };
            c.shapes.rect(fill, bar);
        }
        if let Some(db) = levels.peak.db() {
            let y = y_of(db);
            c.shapes
                .line([x, y], [x + width, y], c.stroke(1.5), peak.faded(0.7));
        }
        if let Some(db) = levels.peak_hold.db() {
            let y = y_of(db);
            let tick = Area {
                x,
                y: y - c.px(1.0),
                width,
                height: c.px(2.0),
            };
            c.shapes.rect(tick, peak);
        }
        let centre = x + width / 2.0;
        c.text(name, centre, names.y + c.px(2.0), 10.0, dim, Align::Centre);
    }

    // The LUFS bar turns the over-target colour above the target.
    let x = column(2);
    let lufs = match settings.lufs_bar {
        LufsBar::ShortTerm => display.short_term,
        LufsBar::Momentary => display.momentary,
    };
    let over = settings
        .target
        .zip(lufs.db())
        .is_some_and(|(target, db)| db > target);
    let lufs_colour = if over {
        c.colour(Role::LoudnessOverTarget)
    } else {
        bar
    };
    c.shapes.rect(Area { x, width, ..bars }, track);
    if let Some(db) = lufs.db() {
        let y = y_of(db);
        let fill = Area {
            x,
            y,
            width,
            height: bars.bottom() - y,
        };
        c.shapes.rect(fill, lufs_colour);
        // Shape cue (High contrast): the part over the target is hatched, so
        // it doesn't rely on colour alone.
        if let (true, Some(target)) = (c.styling.shape_cues, settings.target) {
            let top = y_of(target);
            if y < top {
                let stripe = c.px(6.0);
                let back = c.colour(Role::Background);
                let mut offset = -width;
                while offset < top - y {
                    let from = [x, y + offset + width];
                    let to = [x + width, y + offset];
                    let (from, to) = clip_to(from, to, y, top);
                    if let Some((from, to)) = from.zip(to) {
                        c.shapes.line(from, to, c.px(2.0), back);
                    }
                    offset += stripe;
                }
            }
        }
    }
    if let Some(target) = settings.target {
        let y = y_of(target);
        let colour = if over {
            c.colour(Role::LoudnessOverTarget)
        } else {
            text
        };
        c.shapes.line(
            [x - c.px(3.0), y],
            [x + width + c.px(3.0), y],
            c.stroke(2.0),
            colour,
        );
        let label = format!("{target}");
        c.text(&label, x + width, y - c.px(14.0), 9.0, colour, Align::Right);
    }
    let name = match settings.lufs_bar {
        LufsBar::ShortTerm => "S",
        LufsBar::Momentary => "M",
    };
    c.text(
        name,
        x + width / 2.0,
        names.y + c.px(2.0),
        10.0,
        dim,
        Align::Centre,
    );
}

/// The big reading's name (as its tile is called), level, unit and caption.
fn big_reading(
    settings: &LoudnessMeterSettings,
    display: &LoudnessDisplay,
    true_peak: &'static str,
) -> (&'static str, Level, &'static str) {
    match settings.big_reading {
        BigReading::ShortTerm => ("S", display.short_term, "LUFS short-term"),
        BigReading::Momentary => ("M", display.momentary, "LUFS momentary"),
        BigReading::Integrated => ("I", display.integrated, "LUFS integrated"),
        BigReading::TruePeak if true_peak == "TP" => ("TP", display.true_peak_max, "dBTP max"),
        BigReading::TruePeak => ("Peak", display.true_peak_max, "dBFS peak max"),
    }
}

/// Numbers only: the big loudness reading and the sample peak, the other
/// readings as compact rows and, where there's room, the loudness graph.
/// Wider than tall, the big numbers sit left of the rows; otherwise above
/// them. The big numbers grow to fill the room the rest leaves, and the
/// whole group is centred in the Meter.
fn draw_numbers(
    c: &mut Canvas,
    area: Area,
    settings: &LoudnessMeterSettings,
    display: &LoudnessDisplay,
    rows: &[(&'static str, Level, &'static str)],
) {
    let gap = c.px(10.0);
    let true_peak = if display.true_peak_oversampled {
        "TP"
    } else {
        "Peak"
    };
    let (big_name, level, caption) = big_reading(settings, display, true_peak);
    let others: Vec<_> = rows
        .iter()
        .copied()
        .filter(|(name, ..)| *name != big_name)
        .collect();
    let row_height = c.px(TILE_HEIGHT);
    let row_width = c.px(ROW_WIDTH);
    let column_gap = c.px(24.0);
    let area = area.inset(c.px(6.0));

    let wide =
        area.width >= 1.4 * area.height && area.width >= row_width + gap + c.px(MIN_HEROES_WIDTH);
    let (heroes, readings, graph) = if area.width < c.px(TILE_MIN_WIDTH) {
        // Too narrow for the rows: the big numbers alone.
        (fit_pair(c, settings, area), None, None)
    } else if wide {
        // The rows in one column at the right, as many as fit the height.
        let fit = ((area.height + 1.0) / row_height).floor().max(0.0) as usize;
        let count = others.len().min(fit);
        let block = Area {
            x: area.right() - row_width,
            y: area.y + (area.height - count as f32 * row_height) / 2.0,
            width: row_width,
            height: count as f32 * row_height,
        };
        let heroes = Area {
            width: area.width - row_width - gap,
            ..area
        };
        (fit_pair(c, settings, heroes), Some((block, 1, count)), None)
    } else {
        // As many columns of rows as fit across, centred, and as many rows
        // as leave the big numbers some room.
        let columns = (((area.width + column_gap) / (row_width + column_gap)).floor() as usize)
            .clamp(1, others.len().max(1));
        let fit = ((area.height - c.px(64.0) - gap) / row_height)
            .floor()
            .max(0.0) as usize;
        let count = others.len().min(fit * columns);
        let row_count = count.div_ceil(columns);
        let rows_height = row_count as f32 * row_height;
        let rows_width = columns as f32 * row_width + (columns as f32 - 1.0) * column_gap;
        let rows_block = if count > 0 { gap + rows_height } else { 0.0 };
        let room = (area.height - rows_block).max(0.0);
        let heroes = fit_pair(
            c,
            settings,
            Area {
                height: room,
                ..area
            },
        );
        // The graph takes what the big numbers leave, if that's enough.
        let graph_height = (room - heroes.height - gap - c.px(16.0)).min(c.px(MAX_GRAPH_HEIGHT));
        let graph_width = rows_width.max(c.px(MIN_GRAPH_WIDTH)).min(area.width);
        let with_graph = settings.show_history
            && area.width >= c.px(MIN_GRAPH_WIDTH)
            && graph_height >= c.px(MIN_GRAPH_HEIGHT);
        let graph_block = if with_graph {
            gap + graph_height + c.px(16.0)
        } else {
            0.0
        };
        // Centre the whole group: big numbers, rows, graph.
        let total = heroes.height + rows_block + graph_block;
        let top = area.y + ((area.height - total) / 2.0).max(0.0);
        let heroes = Area { y: top, ..heroes };
        let block = Area {
            x: area.x + (area.width - rows_width) / 2.0,
            y: heroes.bottom() + gap,
            width: rows_width,
            height: rows_height,
        };
        let graph = with_graph.then_some(Area {
            x: area.x + (area.width - graph_width) / 2.0,
            y: heroes.bottom() + rows_block + gap,
            width: graph_width,
            height: graph_height,
        });
        (heroes, Some((block, columns, count)), graph)
    };

    draw_pair(c, heroes, settings, display, big_name, level, caption);

    if let Some((block, columns, count)) = readings {
        // Most important first when not all fit, then in their usual order.
        let mut shown: Vec<usize> = (0..others.len()).collect();
        shown.sort_by_key(|&i| priority(others[i].0, big_name));
        shown.truncate(count);
        shown.sort_unstable();
        let (text, dim) = (c.colour(Role::Text), c.dim());
        let unit_width = c.px(36.0);
        for (slot, &i) in shown.iter().enumerate() {
            let (name, level, unit) = others[i];
            let (row, column) = (slot / columns, slot % columns);
            let x = block.x + column as f32 * (row_width + column_gap);
            let y = block.y + row as f32 * row_height + (row_height - c.px(16.0)) / 2.0;
            c.text(name, x, y, 13.0, dim, Align::Left);
            let unit_x = x + row_width - unit_width;
            c.bold(
                &number(level),
                unit_x - c.px(4.0),
                y,
                13.0,
                text,
                Align::Right,
            );
            c.text(unit, unit_x, y + c.px(2.0), 10.0, dim, Align::Left);
        }
    }
    if let Some(graph) = graph {
        draw_graph(c, graph, settings, display);
    }
}

/// The size a big number gets in `width` × `height` (in text points), and
/// the height its number and caption then take, in px.
fn big_size(c: &Canvas, settings: &LoudnessMeterSettings, width: f32, height: f32) -> (f32, f32) {
    let scale = c.scale * c.styling.text_scale;
    let bar = if settings.show_thin_bars && width >= c.px(170.0) {
        c.px(46.0)
    } else {
        0.0
    };
    // Monospace: "-10.8" is five characters at 0.6 of the size wide.
    let size = ((width - bar - c.px(8.0)) / (6.0 * 0.6) / scale)
        .min(height / BIG_BLOCK / scale)
        .clamp(14.0, 96.0);
    (size, size * BIG_BLOCK * scale)
}

/// Whether the two big numbers sit side by side in `area` (else one over
/// the other): whichever lets them be larger.
fn side_by_side(c: &Canvas, settings: &LoudnessMeterSettings, area: Area) -> bool {
    let (side, _) = big_size(c, settings, area.width / 2.0, area.height);
    let (stacked, _) = big_size(c, settings, area.width, area.height / 2.0);
    side >= stacked
}

/// The part of `area` the two big numbers need, centred in it.
fn fit_pair(c: &Canvas, settings: &LoudnessMeterSettings, area: Area) -> Area {
    let height = if side_by_side(c, settings, area) {
        big_size(c, settings, area.width / 2.0, area.height).1
    } else {
        2.0 * big_size(c, settings, area.width, area.height / 2.0).1
    }
    .min(area.height);
    Area {
        y: area.y + (area.height - height) / 2.0,
        height,
        ..area
    }
}

/// The two big numbers in `area`, laid out as [`fit_pair`] chose.
fn draw_pair(
    c: &mut Canvas,
    area: Area,
    settings: &LoudnessMeterSettings,
    display: &LoudnessDisplay,
    big_name: &str,
    level: Level,
    caption: &str,
) {
    let (first, second) = if side_by_side(c, settings, area) {
        let width = area.width / 2.0;
        (
            Area { width, ..area },
            Area {
                x: area.x + width,
                width,
                ..area
            },
        )
    } else {
        area.split_top(area.height / 2.0)
    };
    let lufs = matches!(big_name, "S" | "M" | "I");
    let over_target = settings
        .target
        .zip(level.db())
        .is_some_and(|(target, db)| db > target);
    let loudness_colour = if lufs && over_target {
        c.colour(Role::LoudnessOverTarget)
    } else {
        c.colour(Role::LoudnessBar)
    };
    draw_big(
        c,
        first,
        settings,
        BigNumber {
            level,
            caption,
            colour: loudness_colour,
            target: if lufs { settings.target } else { None },
        },
    );
    let peak = display.left.peak_hold.max(display.right.peak_hold);
    let clipping = peak.db().is_some_and(|db| db >= 0.0);
    let peak_colour = if clipping {
        c.colour(Role::LoudnessOverTarget)
    } else {
        c.colour(Role::LoudnessPeak)
    };
    draw_big(
        c,
        second,
        settings,
        BigNumber {
            level: peak,
            caption: "dBFS peak",
            colour: peak_colour,
            target: None,
        },
    );
}

/// One big number: its level, the caption under it, its colour and, for a
/// loudness reading, the target to mark on its thin bar.
struct BigNumber<'a> {
    level: Level,
    caption: &'a str,
    colour: Colour,
    target: Option<f64>,
}

/// A big number as large as fits, its caption under it and, with thin bars
/// on, a thin bar beside it on the bar range.
fn draw_big(c: &mut Canvas, area: Area, settings: &LoudnessMeterSettings, big: BigNumber) {
    let dim = c.dim();
    let BigNumber {
        level,
        caption,
        colour,
        target,
    } = big;

    // The thin bar at the right, the number centred in the room left of it.
    // A narrow area leaves the bar out.
    let with_bar = settings.show_thin_bars && area.width >= c.px(170.0);
    let bar_width = c.px(6.0);
    let pad = c.px(10.0);
    let labels_width = c.px(30.0);
    let bar = Area {
        x: area.right() - labels_width - bar_width,
        y: area.y + pad,
        width: bar_width,
        height: (area.height - 2.0 * pad).max(0.0),
    };
    let room = if with_bar {
        Area {
            width: (bar.x - area.x - pad).max(0.0),
            ..area
        }
    } else {
        area.inset(c.px(4.0))
    };
    let scale = c.scale * c.styling.text_scale;
    let (size, block) = big_size(c, settings, area.width, area.height);
    let caption_size = (size * 0.22).clamp(9.0, 16.0);
    let top = area.y + ((area.height - block) / 2.0).max(0.0);
    let centre = room.x + room.width / 2.0;
    c.text(&number(level), centre, top, size, colour, Align::Centre);
    // Only the unit when the whole caption doesn't fit.
    let fits = |text: &str| text.chars().count() as f32 * 0.6 * caption_size * scale <= room.width;
    let caption = if fits(caption) {
        caption
    } else {
        caption.split(' ').next().unwrap_or(caption)
    };
    c.text(
        caption,
        centre,
        top + size * 1.15 * scale,
        caption_size,
        dim,
        Align::Centre,
    );

    if !with_bar || bar.height < c.px(24.0) {
        return;
    }
    let range = (settings.bar_range.0 as f32, settings.bar_range.1 as f32);
    let y_of = |db: f64| map(db as f32, range, bar.bottom(), bar.y).clamp(bar.y, bar.bottom());
    c.shapes.rect(bar, c.colour(Role::Background));
    if let Some(db) = level.db() {
        let y = y_of(db);
        c.shapes.rect(
            Area {
                y,
                height: bar.bottom() - y,
                ..bar
            },
            colour,
        );
    }
    // The top and bottom of the range, and the target for a loudness reading,
    // right-aligned so "0" lines up with "-60".
    let label_x = bar.right() + c.px(28.0);
    c.text(
        &format!("{}", range.1),
        label_x,
        bar.y - c.px(5.0),
        9.0,
        dim,
        Align::Right,
    );
    c.text(
        &format!("{}", range.0),
        label_x,
        bar.bottom() - c.px(7.0),
        9.0,
        dim,
        Align::Right,
    );
    if let Some(target) = target {
        let y = y_of(target);
        let text = c.colour(Role::Text);
        c.shapes.line(
            [bar.x - c.px(3.0), y],
            [bar.right() + c.px(3.0), y],
            c.stroke(1.5),
            text,
        );
        c.text(
            &format!("{target}"),
            label_x,
            y - c.px(6.0),
            9.0,
            text,
            Align::Right,
        );
    }
}

/// The loudness graph: the LUFS bar's reading over the span, newest at the
/// right, on the bars' scale, with the target and integrated lines.
fn draw_graph(
    c: &mut Canvas,
    area: Area,
    settings: &LoudnessMeterSettings,
    display: &LoudnessDisplay,
) {
    let dim = c.dim();
    c.shapes.rect(area, c.colour(Role::Grid).faded(0.2));
    let range = (settings.bar_range.0 as f32, settings.bar_range.1 as f32);
    let y_of = |db: f64| map(db as f32, range, area.bottom(), area.y).clamp(area.y, area.bottom());
    let thin = c.px(1.0).max(1.0);
    // A grid line every 12 dB, labelled at the left.
    let grid = c.colour(Role::Grid).faded(0.6);
    let mut db = (range.1 / 12.0).floor() * 12.0;
    while db > range.0 {
        let y = y_of(f64::from(db));
        if y - area.y > c.px(10.0) && area.bottom() - y > c.px(4.0) {
            c.shapes.rect(
                Area {
                    y,
                    height: thin,
                    ..area
                },
                grid,
            );
            c.text(
                &format!("{db}"),
                area.x + c.px(3.0),
                y - c.px(11.0),
                9.0,
                dim,
                Align::Left,
            );
        }
        db -= 12.0;
    }
    if let Some(target) = settings.target {
        let y = y_of(target);
        c.shapes.line(
            [area.x, y],
            [area.right(), y],
            c.stroke(1.0),
            c.colour(Role::Text).faded(0.6),
        );
    }
    if let Some(integrated) = display.integrated.db() {
        let y = y_of(integrated);
        c.shapes
            .line([area.x, y], [area.right(), y], c.stroke(1.0), dim);
        c.text(
            "I",
            area.right() - c.px(10.0),
            y - c.px(12.0),
            9.0,
            dim,
            Align::Left,
        );
    }
    let steps = (settings.history_span.as_secs_f32() * 10.0).max(1.0);
    let dx = area.width / steps;
    let (normal, over) = (
        c.colour(Role::LoudnessBar),
        c.colour(Role::LoudnessOverTarget),
    );
    let stroke = c.stroke(1.5);
    let newest = display.history.len();
    let x_of = |i: usize| area.right() - (newest - 1 - i) as f32 * dx;
    for (i, pair) in display.history.windows(2).enumerate() {
        let (Some(a), Some(b)) = (pair[0].db(), pair[1].db()) else {
            continue;
        };
        let colour = if settings.target.is_some_and(|t| a.max(b) > t) {
            over
        } else {
            normal
        };
        c.shapes
            .line([x_of(i), y_of(a)], [x_of(i + 1), y_of(b)], stroke, colour);
    }
    let name = match settings.lufs_bar {
        LufsBar::ShortTerm => "S",
        LufsBar::Momentary => "M",
    };
    let label = format!("{name} · {} s", settings.history_span.as_secs());
    c.text(
        &label,
        area.x,
        area.bottom() + c.px(2.0),
        10.0,
        dim,
        Align::Left,
    );
}

/// How many tile columns fit `width`: as many as fit at their narrowest, at most one per reading.
fn tile_columns(c: &Canvas, width: f32, count: usize) -> usize {
    let fit = ((width + c.px(6.0)) / (c.px(TILE_MIN_WIDTH) + c.px(6.0))).floor() as usize;
    fit.clamp(1, count.max(1))
}

/// Which readings stay when not all fit, lowest first: the LUFS the bar
/// shows, integrated, true peak, the other of M and S, then LRA.
fn priority(name: &str, bar: &str) -> u8 {
    match name {
        _ if name == bar => 0,
        "I" => 1,
        "TP" | "Peak" => 2,
        "M" | "S" => 3,
        "LRA" => 4,
        _ => 5,
    }
}

/// The part of the line from `from` to `to` between the heights `top` and
/// `bottom`, if any: for hatching inside a band.
fn clip_to(
    from: [f32; 2],
    to: [f32; 2],
    top: f32,
    bottom: f32,
) -> (Option<[f32; 2]>, Option<[f32; 2]>) {
    let at = |y: f32| {
        let t = (y - from[1]) / (to[1] - from[1]);
        [from[0] + t * (to[0] - from[0]), y]
    };
    let clamp = |p: [f32; 2]| {
        if p[1] < top {
            at(top)
        } else if p[1] > bottom {
            at(bottom)
        } else {
            p
        }
    };
    let (a, b) = (clamp(from), clamp(to));
    if (a[1] - b[1]).abs() < 0.5 {
        (None, None)
    } else {
        (Some(a), Some(b))
    }
}

/// A reading as shown: one decimal, or "-inf" for silence.
fn number(level: Level) -> String {
    match level.db() {
        Some(db) => format!("{db:.1}"),
        None => "-inf".to_owned(),
    }
}
