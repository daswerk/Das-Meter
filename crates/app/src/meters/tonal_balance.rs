//! The Tonal Balance Meter: the mix's long-term curve, shape only, over its
//! Target's range. Fine draws the curves across 20 Hz–20 kHz (or the low end
//! alone, with its notes); Broad draws five regions, each a bar inside its
//! range. Under them, the low end's figures: the Sub and Bass notes, how mono
//! it is and its crest against the Target's.

use dasmeter_analysis::tonal_balance::{Band, FINE_BELOW_HZ, Region, Summary, tonal_grid};
use dasmeter_core::{Colour, Role, TargetScene, TonalBalanceMeterSettings, TonalView};

use super::labels::Align;
use super::shapes::{Area, smooth};
use super::{Canvas, map};

/// What a Tonal Balance Meter draws besides its settings.
pub struct Drawn<'a> {
    pub mix: Option<&'a Summary>,
    pub frozen: Option<&'a [f32]>,
    pub learning: Option<u32>,
    /// The file compared in place of the live Source.
    pub comparing: Option<&'a str>,
    pub target: Option<&'a TargetScene>,
}

/// The low end zoom's range.
const LOW_END: (f32, f32) = (20.0, FINE_BELOW_HZ);
/// The whole range.
const FULL: (f32, f32) = (20.0, 20_000.0);
/// How many dB the Fine view spans from top to bottom, at least and at most.
const FINE_SPAN_DB: (f32, f32) = (36.0, 66.0);
/// The verdict (inside the range, too much or too little) is judged on
/// curves smoothed over this many octaves, as the ear hears balance.
const VERDICT_OCTAVES: f32 = 1.0 / 3.0;
/// How far the Broad view reaches either side of the Target's average.
const BROAD_SPAN_DB: f32 = 12.0;
/// Below this the low end counts as not mono enough.
const MONO_WARNING: f32 = 0.8;
/// The Target's band is drawn smoothed over this many octaves, so it reads
/// as a shape rather than every point's wobble.
const TARGET_OCTAVES: f32 = 1.0 / 6.0;
/// Taller than this (logical px), a Meter shows its readouts and controls.
const ROOMY_HEIGHT: f32 = 110.0;
/// Narrower than this (logical px), the controls are left out.
const CONTROLS_MIN_WIDTH: f32 = 260.0;
/// The on-Meter controls' sizes, in logical px.
const TARGET_PICKER_WIDTH: f32 = 150.0;
const LOW_END_WIDTH: f32 = 64.0;
const CONTROLS_HEIGHT: f32 = 18.0;
const CONTROLS_GAP: f32 = 8.0;

/// Where the controls over a Tonal Balance Meter go (egui draws them on
/// the title line): the Target picker at its left, Low end zoom at its
/// right while the curves are shown.
#[derive(Clone, Copy, Debug)]
pub struct Controls {
    pub target: Area,
    pub low_end: Option<Area>,
}

/// The controls for a Meter drawn in `inner` (its area inside the padding),
/// whose units are `scale` per logical px; `None` when there's no room.
pub fn controls(
    inner: Area,
    scale: f32,
    settings: &TonalBalanceMeterSettings,
    bar: bool,
) -> Option<Controls> {
    if inner.height <= ROOMY_HEIGHT * scale || inner.width < CONTROLS_MIN_WIDTH * scale {
        return None;
    }
    let height = CONTROLS_HEIGHT * scale;
    let y = inner.y - scale;
    let target = Area {
        x: inner.x,
        y,
        width: TARGET_PICKER_WIDTH * scale,
        height,
    };
    let low_end = (!is_broad(settings, bar)).then(|| Area {
        x: inner.right() - LOW_END_WIDTH * scale,
        y,
        width: LOW_END_WIDTH * scale,
        height,
    });
    Some(Controls { target, low_end })
}

fn is_broad(settings: &TonalBalanceMeterSettings, bar: bool) -> bool {
    match settings.view {
        TonalView::Broad => true,
        TonalView::Fine => false,
        TonalView::Auto => bar,
    }
}

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

fn seconds_text(s: u32) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}

/// Above, below or inside: the colour for a value `outside` a band by
/// this many dB, `inside` when it's in range.
fn verdict_colour(c: &Canvas, outside: f32, inside: Role) -> Colour {
    if outside > 0.0 {
        c.colour(Role::TonalBalanceOver)
    } else if outside < 0.0 {
        c.colour(Role::TonalBalanceUnder)
    } else {
        c.colour(inside)
    }
}

/// `values` at the grid's `frequencies`, each the average of those within
/// half of `octaves` either side.
fn smoothed(values: &[f32], frequencies: &[f32], octaves: f32) -> Vec<f32> {
    frequencies
        .iter()
        .map(|&f| {
            let (sum, count) = frequencies
                .iter()
                .zip(values)
                .filter(|&(&g, _)| (g / f).log2().abs() <= octaves / 2.0)
                .fold((0.0, 0), |(sum, count), (_, &v)| (sum + v, count + 1));
            sum / count.max(1) as f32
        })
        .collect()
}

/// `bar` says whether it's drawn in the Bar, where Auto shows Broad.
pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &TonalBalanceMeterSettings,
    drawn: Drawn,
    bar: bool,
) {
    let line = c.px(14.0) * c.styling.text_scale;
    let roomy = area.height > c.px(ROOMY_HEIGHT);
    let controls = controls(area, c.scale, settings, bar);
    let (title, rest) = area.split_top(line.max(c.px(CONTROLS_HEIGHT)) + c.px(4.0));
    title_line(c, title, &drawn, controls);
    let (plot, readouts) = if roomy {
        rest.split_bottom(line + c.px(4.0))
    } else {
        (
            rest,
            Area {
                height: 0.0,
                ..rest
            },
        )
    };
    let broad = is_broad(settings, bar);
    if drawn.mix.is_none() && drawn.target.is_none_or(|t| t.curve.is_empty()) {
        let dim = c.dim();
        let text = match (drawn.target.is_some(), controls.is_some()) {
            (true, _) => "Play something to compare with the Target",
            (false, true) => "Play something, then pick a Target above or teach one in the menu",
            (false, false) => "Play something, then pick or teach a Target in the menu",
        };
        c.text(text, plot.x, plot.y + c.px(4.0), 11.0, dim, Align::Left);
        return;
    }
    if broad {
        draw_broad(c, plot, &drawn);
    } else {
        draw_fine(c, plot, settings, &drawn);
    }
    if roomy {
        readout_line(c, readouts, &drawn);
    }
}

/// The Target's name and key (or what's missing), the mix's key, and
/// whether a song is being learned. With `controls`, the Target picker
/// shows the name, and the rest sits between the controls.
fn title_line(c: &mut Canvas, area: Area, drawn: &Drawn, controls: Option<Controls>) {
    let (text, dim, accent) = (c.colour(Role::Text), c.dim(), c.colour(Role::Accent));
    let mut parts: Vec<String> = Vec::new();
    if let Some(file) = drawn.comparing {
        parts.push(format!("{file} vs"));
    }
    match (drawn.target, controls.is_some()) {
        (None, false) => parts.push("No Target".to_owned()),
        (None, true) => {}
        (Some(t), with_picker) => {
            let state = if t.missing {
                Some("(missing)".to_owned())
            } else if t.songs.is_empty() {
                Some("(no songs yet)".to_owned())
            } else {
                t.key.map(|key| key.name())
            };
            match (with_picker, state) {
                (false, Some(state)) if t.missing || t.songs.is_empty() => {
                    parts.push(format!("{} {state}", t.name));
                }
                (false, Some(key)) => parts.push(format!("{} · {key}", t.name)),
                (false, None) => parts.push(t.name.clone()),
                (true, Some(state)) => parts.push(state),
                (true, None) => {}
            }
        }
    }
    if let Some(seconds) = drawn.learning {
        parts.insert(0, format!("Learning {}", seconds_text(seconds)));
    }
    let left = parts.join("  ·  ");
    let colour = if drawn.learning.is_some() {
        accent
    } else {
        text
    };
    let y = area.y + (c.px(CONTROLS_HEIGHT) - c.px(14.0) * c.styling.text_scale).max(0.0) / 2.0;
    let x = controls.map_or(area.x, |k| k.target.right() + c.px(CONTROLS_GAP));
    c.text(&left, x, y, 11.0, colour, Align::Left);
    let right_x = controls
        .and_then(|k| k.low_end)
        .map_or(area.right(), |l| l.x - c.px(CONTROLS_GAP));
    if let Some(key) = drawn.target.and_then(|t| t.suggest_shift) {
        let right = format!("Shift Target to {}?", key.name());
        c.text(&right, right_x, y, 10.0, accent, Align::Right);
        return;
    }
    if let Some(key) = drawn
        .mix
        .and_then(|m| m.key)
        .filter(|k| k.confidence >= dasmeter_analysis::tonal_balance::KEY_CONFIDENCE)
    {
        let right = format!("Mix: {}", key.key.name());
        c.text(&right, right_x, y, 10.0, dim, Align::Right);
    }
}

/// The curves across the frequency range.
fn draw_fine(c: &mut Canvas, area: Area, settings: &TonalBalanceMeterSettings, drawn: &Drawn) {
    let range = if settings.low_end_zoom { LOW_END } else { FULL };
    let grid_frequencies = tonal_grid();
    let shown: Vec<usize> = (0..grid_frequencies.len())
        .filter(|&i| (range.0..=range.1 * 1.001).contains(&grid_frequencies[i]))
        .collect();
    let target = drawn
        .target
        .filter(|t| t.curve.len() == grid_frequencies.len());
    // The top: the loudest of what's drawn, rounded up to 6 dB; the bottom
    // takes in all but the quietest few points.
    let mut levels: Vec<f32> = shown
        .iter()
        .flat_map(|&i| {
            let mix = drawn.mix.and_then(|m| m.curve.get(i).copied());
            let band = target.map(|t| t.curve[i]);
            let high = band.map(|b| b.average + b.range);
            let low = band.map(|b| b.average - b.range);
            mix.into_iter().chain(high).chain(low)
        })
        .filter(|l| l.is_finite())
        .collect();
    levels.sort_by(f32::total_cmp);
    let (top, bottom) = match (levels.first(), levels.last()) {
        (Some(_), Some(&loudest)) => {
            let top = (loudest / 6.0).ceil() * 6.0 + 3.0;
            let quiet = levels[levels.len() / 20];
            let span = (top - quiet + 3.0).clamp(FINE_SPAN_DB.0, FINE_SPAN_DB.1);
            (top, top - (span / 6.0).ceil() * 6.0)
        }
        _ => (0.0, -FINE_SPAN_DB.0),
    };
    let db = (bottom, top);
    let (low, high) = (range.0.ln(), range.1.ln());
    let x_of = |f: f32| map(f.ln(), (low, high), area.x, area.right());
    let y_of = |level: f32| map(level, db, area.bottom(), area.y);

    grid(
        c,
        area,
        range,
        settings.low_end_zoom,
        drawn,
        db,
        &x_of,
        &y_of,
    );

    if let Some(target) = target {
        draw_target(c, target, &shown, &grid_frequencies, &x_of, &y_of);
    }

    if let Some(frozen) = drawn.frozen.filter(|f| f.len() == grid_frequencies.len()) {
        let ghost: Vec<[f32; 2]> = shown
            .iter()
            .map(|&i| [x_of(grid_frequencies[i]), y_of(frozen[i])])
            .collect();
        let colour = c.colour(Role::Text).faded(0.35);
        let line = smooth(&ghost, c.px(2.0));
        c.shapes.polyline(&line, c.stroke(1.5), colour);
    }

    let Some(mix) = drawn
        .mix
        .filter(|m| m.curve.len() == grid_frequencies.len())
    else {
        return;
    };
    let points: Vec<[f32; 2]> = shown
        .iter()
        .map(|&i| [x_of(grid_frequencies[i]), y_of(mix.curve[i])])
        .collect();
    // Each stretch in the colour of its verdict: inside the range, too
    // much or too little, judged on the curves smoothed to 1/3 octave.
    let outside: Vec<f32> = match target {
        Some(t) => {
            let mix = smoothed(&mix.curve, &grid_frequencies, VERDICT_OCTAVES);
            let averages: Vec<f32> = t.curve.iter().map(|b| b.average).collect();
            let ranges: Vec<f32> = t.curve.iter().map(|b| b.range).collect();
            let (averages, ranges) = (
                smoothed(&averages, &grid_frequencies, VERDICT_OCTAVES),
                smoothed(&ranges, &grid_frequencies, VERDICT_OCTAVES),
            );
            shown
                .iter()
                .map(|&i| {
                    let band = Band {
                        average: averages[i],
                        range: ranges[i],
                    };
                    band.outside(mix[i])
                })
                .collect()
        }
        None => vec![0.0; shown.len()],
    };
    // The smoothed curve in runs of one colour, each segment taking the
    // worse verdict of the grid points either side of it; each run glows in
    // its own colour.
    let curve = smooth(&points, c.px(2.0));
    let mut runs: Vec<(Colour, Vec<[f32; 2]>)> = Vec::new();
    let mut segment = 0;
    for &point in &curve {
        while segment + 2 < points.len() && point[0] > points[segment + 1][0] {
            segment += 1;
        }
        let k = segment.min(outside.len().saturating_sub(2));
        let worse = match (outside.get(k), outside.get(k + 1)) {
            (Some(&a), Some(&b)) if a.abs() > b.abs() => a,
            (_, Some(&b)) => b,
            (Some(&a), None) => a,
            (None, None) => 0.0,
        };
        let colour = verdict_colour(c, worse, Role::TonalBalanceMix);
        match runs.last_mut() {
            Some((run_colour, run)) if *run_colour == colour => run.push(point),
            last => {
                // Each run starts where the one before ends.
                if let Some((_, run)) = last {
                    run.push(point);
                }
                runs.push((colour, vec![point]));
            }
        }
    }
    for (colour, run) in &runs {
        c.glow_ribbon(run, *colour);
    }
    let width = c.stroke(2.0);
    for (colour, run) in &runs {
        c.shapes.polyline(run, width, *colour);
    }
}

/// The Target's range as a soft band, strongest along its average and
/// fading toward its edges, smoothed so it reads as a shape; its average
/// as a thin line through it.
fn draw_target(
    c: &mut Canvas,
    target: &TargetScene,
    shown: &[usize],
    frequencies: &[f32],
    x_of: &dyn Fn(f32) -> f32,
    y_of: &dyn Fn(f32) -> f32,
) {
    let averages: Vec<f32> = target.curve.iter().map(|b| b.average).collect();
    let ranges: Vec<f32> = target.curve.iter().map(|b| b.range).collect();
    let (averages, ranges) = (
        smoothed(&averages, frequencies, TARGET_OCTAVES),
        smoothed(&ranges, frequencies, TARGET_OCTAVES),
    );
    let at = |level: &dyn Fn(usize) -> f32| -> Vec<[f32; 2]> {
        let points: Vec<[f32; 2]> = shown
            .iter()
            .map(|&i| [x_of(frequencies[i]), y_of(level(i))])
            .collect();
        smooth(&points, c.px(2.0))
    };
    let top = at(&|i| averages[i] + ranges[i]);
    let middle = at(&|i| averages[i]);
    let bottom = at(&|i| averages[i] - ranges[i]);
    let colour = c.colour(Role::TonalBalanceRange);
    let (edge, centre) = (colour.faded(0.08), colour.faded(0.3));
    for k in 0..middle
        .len()
        .min(top.len())
        .min(bottom.len())
        .saturating_sub(1)
    {
        let (a, b) = (k, k + 1);
        c.shapes
            .quad([top[a], top[b], middle[a], middle[b]], edge, centre);
        c.shapes
            .quad([middle[a], middle[b], bottom[a], bottom[b]], centre, edge);
    }
    let thin = c.stroke(0.8);
    c.shapes.polyline(&top, thin, colour.faded(0.3));
    c.shapes.polyline(&bottom, thin, colour.faded(0.3));
    c.shapes
        .polyline(&middle, c.stroke(1.2), colour.faded(0.75));
}

/// Grid lines with labels: decades across the whole range, or every
/// semitone across the low end, the keys' tonics marked in the accent.
#[allow(clippy::too_many_arguments)]
fn grid(
    c: &mut Canvas,
    area: Area,
    range: (f32, f32),
    notes: bool,
    drawn: &Drawn,
    db: (f32, f32),
    x_of: &dyn Fn(f32) -> f32,
    y_of: &dyn Fn(f32) -> f32,
) {
    let (grid, dim, accent) = (c.colour(Role::Grid), c.dim(), c.colour(Role::Accent));
    let mut level = (db.0 / 6.0).ceil() * 6.0;
    while level <= db.1 {
        let y = y_of(level);
        c.grid_h(area, y, grid, false);
        level += 6.0;
    }
    let label_y = area.bottom() - c.px(12.0) * c.styling.text_scale;
    if !notes {
        for (f, name) in [
            (50.0, "50"),
            (100.0, "100"),
            (200.0, "200"),
            (500.0, "500"),
            (1_000.0, "1k"),
            (2_000.0, "2k"),
            (5_000.0, "5k"),
            (10_000.0, "10k"),
        ] {
            let x = x_of(f);
            c.grid_v(area, x, grid, f == 1_000.0);
            c.text(name, x + c.px(2.0), label_y, 9.0, dim, Align::Left);
        }
        return;
    }
    // Every semitone; labels where there's room: naturals, or only C and G.
    let tonics: Vec<u8> = [
        drawn.target.and_then(|t| t.key),
        drawn.target.and_then(|t| t.suggest_shift),
    ]
    .into_iter()
    .flatten()
    .map(|k| k.tonic)
    .collect();
    let first = (12.0 * (range.0 / 440.0).log2()).ceil() as i32 + 69;
    let last = (12.0 * (range.1 / 440.0).log2()).floor() as i32 + 69;
    let per_semitone = area.width / (last - first).max(1) as f32;
    for midi in first..=last {
        let f = 440.0 * 2f32.powf((midi - 69) as f32 / 12.0);
        let x = x_of(f);
        let class = midi.rem_euclid(12) as usize;
        let tonic = tonics.contains(&(class as u8));
        let natural = !NOTE_NAMES[class].contains('#');
        if tonic {
            c.grid_v(area, x, accent.faded(0.6), true);
        } else if natural {
            c.grid_v(area, x, grid, class == 0);
        } else {
            c.grid_v(area, x, grid.faded(0.4), false);
        }
        let labelled = if per_semitone >= c.px(16.0) {
            natural || tonic
        } else {
            class == 0 || class == 7 || tonic
        };
        if labelled {
            let name = format!("{}{}", NOTE_NAMES[class], midi.div_euclid(12) - 1);
            let colour = if tonic { accent } else { dim };
            c.text(&name, x + c.px(2.0), label_y, 9.0, colour, Align::Left);
        }
    }
}

/// Five regions, each a bar from the Target's average to the mix inside
/// the Target's range; without a Target, the mix's region levels.
fn draw_broad(c: &mut Canvas, area: Area, drawn: &Drawn) {
    let (grid, dim) = (c.colour(Role::Grid), c.dim());
    let regions = drawn.target.and_then(|t| t.regions);
    let label_height = c.px(13.0) * c.styling.text_scale;
    let (bars, labels) = area.split_bottom(label_height);
    let gap = c.px(6.0);
    let width = (bars.width - gap * 4.0) / 5.0;
    let span = match regions {
        Some(_) => (-BROAD_SPAN_DB, BROAD_SPAN_DB),
        None => {
            let top = drawn
                .mix
                .map_or(0.0, |m| m.regions.iter().copied().fold(f32::MIN, f32::max));
            ((top / 6.0).ceil() * 6.0 - 36.0, (top / 6.0).ceil() * 6.0)
        }
    };
    let y_of = |v: f32| map(v, span, bars.bottom(), bars.y);
    if regions.is_some() {
        c.grid_h(bars, y_of(0.0), grid, true);
    }
    let range_colour = c.colour(Role::TonalBalanceRange);
    for (k, region) in Region::ALL.into_iter().enumerate() {
        let column = Area {
            x: bars.x + k as f32 * (width + gap),
            width,
            ..bars
        };
        let mix = drawn.mix.map(|m| m.regions[k]);
        let short = match region {
            Region::Sub => "Sub",
            Region::Bass => "Bass",
            Region::LowMid => "Low-mid",
            Region::Mid => "Mid",
            Region::High => "High",
        };
        c.text(
            short,
            column.x + column.width / 2.0,
            labels.y + c.px(1.0),
            9.0,
            dim,
            Align::Centre,
        );
        c.shapes.rect(column, grid.faded(0.18));
        match regions {
            Some(bands) => {
                let band: Band = bands[k];
                let (top, bottom) = (y_of(band.range), y_of(-band.range));
                c.shapes.rect(
                    Area {
                        y: top,
                        height: bottom - top,
                        ..column
                    },
                    range_colour.faded(0.3),
                );
                let Some(mix) = mix else { continue };
                let off = mix - band.average;
                let colour = verdict_colour(c, band.outside(mix), Role::TonalBalanceMix);
                let (zero, at) = (y_of(0.0), y_of(off.clamp(span.0, span.1)));
                let inner = column.inset(c.px(3.0).min(column.width / 4.0));
                c.shapes.rect(
                    Area {
                        y: zero.min(at),
                        height: (zero - at).abs().max(c.px(2.0)),
                        x: inner.x,
                        width: inner.width,
                    },
                    colour.faded(0.85),
                );
                if column.height > c.px(50.0) {
                    // Beyond the bar's end, away from the middle.
                    let text = format!("{off:+.1}");
                    let height = c.px(12.0) * c.styling.text_scale;
                    let y = if off >= 0.0 {
                        (at - height - c.px(2.0)).max(column.y)
                    } else {
                        (at + c.px(2.0)).min(column.bottom() - height)
                    };
                    let colour = verdict_colour(c, band.outside(mix), Role::Text);
                    c.text(
                        &text,
                        column.x + column.width / 2.0,
                        y,
                        10.0,
                        colour,
                        Align::Centre,
                    );
                }
            }
            None => {
                let Some(mix) = mix else { continue };
                let at = y_of(mix.clamp(span.0, span.1));
                let colour = c.colour(Role::TonalBalanceMix);
                let inner = column.inset(c.px(3.0).min(column.width / 4.0));
                c.shapes.rect(
                    Area {
                        y: at,
                        height: column.bottom() - at,
                        x: inner.x,
                        width: inner.width,
                    },
                    colour.faded(0.8),
                );
            }
        }
    }
}

/// The low end's figures in one line, each against the Target's where it has one.
fn readout_line(c: &mut Canvas, area: Area, drawn: &Drawn) {
    let Some(mix) = drawn.mix else {
        return;
    };
    let dim = c.dim();
    let mut parts: Vec<(String, Colour)> = Vec::new();
    for (label, found) in [("Sub", mix.sub), ("Bass", mix.bass)] {
        if let Some(f) = found {
            parts.push((
                format!("{label} {} {:.1} Hz", f.note, f.frequency),
                c.colour(Role::Text),
            ));
        }
    }
    if let Some(mono) = mix.low_end_mono {
        let colour = if mono < MONO_WARNING {
            c.colour(Role::TonalBalanceOver)
        } else {
            c.colour(Role::Text)
        };
        parts.push((format!("Low mono {mono:+.2}"), colour));
    }
    let target_crest = drawn.target.map_or([None; 2], |t| t.low_end_crest);
    for (k, label) in ["Sub crest", "Bass crest"].into_iter().enumerate() {
        let Some(crest) = mix.low_end_crest[k] else {
            continue;
        };
        let (text, colour) = match target_crest[k] {
            Some(band) => (
                format!(
                    "{label} {crest:.1} dB ({:.0}–{:.0})",
                    band.average - band.range,
                    band.average + band.range
                ),
                verdict_colour(c, band.outside(crest), Role::Text),
            ),
            None => (format!("{label} {crest:.1} dB"), c.colour(Role::Text)),
        };
        parts.push((text, colour));
    }
    // As many as fit, left to right.
    let mut x = area.x;
    let y = area.y + c.px(3.0);
    for (k, (text, colour)) in parts.into_iter().enumerate() {
        let width = c.px(10.0 * 0.6) * c.styling.text_scale * text.chars().count() as f32;
        if x + width > area.right() {
            break;
        }
        if k > 0 {
            c.text("·", x - c.px(9.0), y, 10.0, dim, Align::Left);
        }
        c.text(&text, x, y, 10.0, colour, Align::Left);
        x += width + c.px(16.0);
    }
}
