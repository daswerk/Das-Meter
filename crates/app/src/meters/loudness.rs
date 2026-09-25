//! The Loudness Meter: numbers for M, S, I, LRA and true peak; L/R RMS bars
//! with sample-peak lines and hold ticks; a LUFS bar with the target line.

use dasmeter_core::{Level, LoudnessDisplay, LoudnessMeterSettings, LufsBar, Role};

use super::labels::Align;
use super::shapes::Area;
use super::{Canvas, map};

/// The numbers' column in the side-by-side layout, and the least the bars keep.
const NUMBERS_WIDTH: f32 = 170.0;
const MIN_BARS_WIDTH: f32 = 90.0;
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

    // Numbers.
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

    // Numbers above the bars; in an area wider than it is tall (a Bar along
    // the top or bottom), numbers on the left and the bars beside them at
    // full height.
    let numbers_height = c.px(18.0) * rows.len() as f32 + c.px(10.0);
    let beside = area.width >= area.height && area.width >= c.px(NUMBERS_WIDTH + MIN_BARS_WIDTH);
    let (numbers, bars) = if beside {
        let width = c.px(NUMBERS_WIDTH);
        let numbers = Area { width, ..area };
        let bars = Area {
            x: area.x + width,
            width: area.width - width,
            ..area
        };
        (numbers, bars)
    } else {
        area.split_top(numbers_height)
    };
    // Rows shrink to fit a short area, the rate line included when beside.
    let lines = rows.len() as f32 + if beside { 1.0 } else { 0.0 };
    let line = c.px(18.0).min(numbers.height / lines.max(1.0));
    let value_right = numbers.x + c.px(96.0);
    for (i, &(name, level, unit)) in rows.iter().enumerate() {
        let y = numbers.y + line * i as f32;
        c.text(name, numbers.x, y, size, dim, Align::Left);
        c.bold(&number(level), value_right, y, size, text, Align::Right);
        c.text(unit, value_right + c.px(6.0), y, size, dim, Align::Left);
    }
    if beside {
        let y = numbers.y + line * rows.len() as f32;
        c.text(&rate, numbers.x, y, 10.0, dim, Align::Left);
    } else {
        c.text(&rate, area.right(), area.y, 10.0, dim, Align::Right);
    }

    // Bars: L, R, then the LUFS bar.
    let (bars, names) = bars.split_bottom(c.px(16.0));
    let range = (settings.bar_range.0 as f32, settings.bar_range.1 as f32);
    let y_of = |db: f64| map(db as f32, range, bars.bottom(), bars.y);
    let left = bars.x + c.px(28.0);
    let gap = c.px(6.0);
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
        c.text(
            &format!("{db}"),
            bars.x,
            y - c.px(6.0),
            9.0,
            dim,
            Align::Left,
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

/// A reading as shown: one decimal, or "-inf" for silence.
fn number(level: Level) -> String {
    match level.db() {
        Some(db) => format!("{db:.1}"),
        None => "-inf".to_owned(),
    }
}
