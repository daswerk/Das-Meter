//! The Tuner: the note large, a bar of cents from −50 to +50 with the
//! reading on it, and the pitch in Hz. In tune (within 5 cents) it turns the
//! in-tune colour; a held reading (the pitch just lost) is drawn faded.
//! Short and wide, as in the Bar, the note sits left of the bar.

use dasmeter_core::{Colour, Role, TunerDisplay, TunerMeterSettings};

use super::Canvas;
use super::labels::Align;
use super::shapes::Area;

/// Within this many cents a note counts as in tune.
const IN_TUNE_CENTS: f32 = 5.0;
/// Shorter than this (logical px), the note sits beside the bar.
const COMPACT_HEIGHT: f32 = 110.0;

pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &TunerMeterSettings,
    reading: Option<&TunerDisplay>,
) {
    let compact = area.height < c.px(COMPACT_HEIGHT);
    let fade = if reading.is_some_and(|r| r.held) {
        0.45
    } else {
        1.0
    };
    let in_tune = reading.is_some_and(|r| r.note.cents.abs() <= IN_TUNE_CENTS);
    let accent = if in_tune {
        c.colour(Role::TunerInTune)
    } else {
        c.colour(Role::TunerOff)
    }
    .faded(fade);
    let (text, dim) = (c.colour(Role::Text).faded(fade), c.dim());
    let note_colour = if in_tune { accent } else { text };
    let scale = c.scale * c.styling.text_scale;

    let (note_area, bar_area, caption_area) = if compact {
        let side = (area.height * 1.4).min(area.width * 0.35);
        let note = Area {
            width: side,
            ..area
        };
        let rest = Area {
            x: area.x + side,
            width: area.width - side,
            ..area
        };
        let (bar, caption) =
            rest.split_bottom((area.height * 0.4).min(c.px(16.0) * c.styling.text_scale));
        (note, bar, caption)
    } else {
        let (top, bottom) = area.split_top(area.height * 0.6);
        let (bar, caption) = bottom.split_bottom(c.px(18.0) * c.styling.text_scale);
        (top, bar, caption)
    };

    // The note: its name large, the octave smaller beside it.
    let (name, octave) = match reading {
        Some(r) => (r.note.name.to_owned(), r.note.octave.to_string()),
        None => ("–".to_owned(), String::new()),
    };
    let chars = name.chars().count() as f32 + 0.6 * octave.chars().count() as f32;
    let size = (note_area.height * 0.8 / scale)
        .min(note_area.width * 0.85 / (0.6 * chars.max(1.0)) / scale)
        .clamp(10.0, 160.0);
    let width = 0.6 * size * scale * chars;
    let x = note_area.x + (note_area.width - width) / 2.0;
    let y = note_area.y + (note_area.height - size * scale) / 2.0;
    let colour = if reading.is_some() { note_colour } else { dim };
    if in_tune && reading.is_some_and(|r| !r.held) {
        c.glowing_text(&name, [x, y], size, colour, Align::Left);
    } else {
        c.bold(&name, x, y, size, colour, Align::Left);
    }
    if !octave.is_empty() {
        let name_width = 0.6 * size * scale * name.chars().count() as f32;
        let small = size * 0.5;
        c.text(
            &octave,
            x + name_width,
            y + (size - small) * scale * 0.9,
            small,
            colour,
            Align::Left,
        );
    }

    cents_bar(c, bar_area, reading, accent, dim, compact);

    // The cents and the pitch; the reference when it isn't 440.
    let mut caption = match reading {
        Some(r) => format!("{:+.0} ¢   {:.1} Hz", r.note.cents, r.frequency),
        None => "Play one note".to_owned(),
    };
    if (settings.a4 - 440.0).abs() > 0.05 {
        caption += &format!("   A4 = {:.1} Hz", settings.a4);
    }
    let x = caption_area.x + caption_area.width / 2.0;
    let colour = if reading.is_some() { text } else { dim };
    c.text(
        &caption,
        x,
        caption_area.y + c.px(2.0),
        11.0,
        colour,
        Align::Centre,
    );
}

/// The bar from −50 to +50 cents, ticks every 10 and a longer one at 0, the
/// reading as a marker over it, filled from the centre.
fn cents_bar(
    c: &mut Canvas,
    area: Area,
    reading: Option<&TunerDisplay>,
    accent: Colour,
    dim: Colour,
    compact: bool,
) {
    let inset = c.px(12.0);
    let left = area.x + inset;
    let right = area.right() - inset;
    if right <= left {
        return;
    }
    let mid_y = area.y + area.height / 2.0;
    let x_of = |cents: f32| left + (cents.clamp(-50.0, 50.0) + 50.0) / 100.0 * (right - left);
    let grid = c.colour(Role::Grid);
    let thin = c.px(1.0).max(1.0);
    let tall = (area.height * 0.6).min(c.px(36.0));
    c.shapes.rect(
        Area {
            x: left,
            y: mid_y - thin / 2.0,
            width: right - left,
            height: thin,
        },
        grid,
    );
    for tick in (-50..=50).step_by(10) {
        let x = x_of(tick as f32);
        let height = if tick == 0 { tall } else { tall * 0.4 };
        let colour = if tick == 0 { dim } else { grid };
        c.shapes.rect(
            Area {
                x: x - thin / 2.0,
                y: mid_y - height / 2.0,
                width: thin.max(if tick == 0 { c.px(2.0) } else { thin }),
                height,
            },
            colour,
        );
    }
    if !compact && area.height > c.px(40.0) {
        for (cents, label) in [(-50.0, "−50"), (50.0, "+50")] {
            let align = if cents < 0.0 {
                Align::Left
            } else {
                Align::Right
            };
            c.text(label, x_of(cents), mid_y + tall / 2.0, 9.0, dim, align);
        }
    }
    let Some(reading) = reading else {
        return;
    };
    let x = x_of(reading.note.cents);
    let centre = x_of(0.0);
    // From the centre to the reading, then the marker.
    c.shapes.rect(
        Area {
            x: centre.min(x),
            y: mid_y - c.px(2.0),
            width: (x - centre).abs(),
            height: c.px(4.0),
        },
        accent.faded(0.5),
    );
    let marker = c.px(6.0).max(tall * 0.25);
    c.shapes.rounded_rect(
        Area {
            x: x - marker / 2.0,
            y: mid_y - tall / 2.0,
            width: marker,
            height: tall,
        },
        marker / 2.0,
        accent,
    );
}
