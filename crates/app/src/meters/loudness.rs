//! The Loudness Meter: numbers for M, S, I, LRA and true peak; L/R RMS bars
//! with sample-peak lines and hold ticks; a LUFS bar with the target line.

use dasmeter_core::{Level, LoudnessDisplay, LoudnessMeterSettings, LufsBar, Role};

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
/// The least room between two scale labels.
const LABEL_SPACING: f32 = 11.0;
/// The widest a bar gets.
const MAX_BAR_WIDTH: f32 = 26.0;

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
    ];
    let rows: Vec<_> = rows.into_iter().flatten().collect();
    let rate = format!("{:.1} kHz", f64::from(display.sample_rate) / 1000.0);

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
        let (tiles, bars) = area.split_top(wanted.min(area.height / 2.0));
        // The bar group sits in the middle of the width left under the tiles.
        let width = bars_width.min(bars.width);
        let bars = Area {
            x: bars.x + (bars.width - width) / 2.0,
            y: bars.y + gap,
            width,
            height: (bars.height - gap).max(0.0),
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
    if show_rate {
        let y = tiles.y + tile_rows as f32 * (tile_height + gap);
        c.text(&rate, tiles.x, y, 10.0, dim, Align::Left);
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
                .line([x, y], [x + width, y], c.px(1.5), peak.faded(0.7));
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
            c.px(2.0),
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
        _ => 4,
    }
}

/// A reading as shown: one decimal, or "-inf" for silence.
fn number(level: Level) -> String {
    match level.db() {
        Some(db) => format!("{db:.1}"),
        None => "-inf".to_owned(),
    }
}
