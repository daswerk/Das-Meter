//! Stereo Bands: a column per octave band, each its band correlation drawn
//! as the Stereometer draws its Correlation: a track from −1 (bottom) to +1
//! (top), filled from 0 in the positive or negative colour, with a marker at
//! the reading. Silent bands are left empty.

use dasmeter_core::{BandDisplay, Role, StereoBandsMeterSettings};

use super::labels::Align;
use super::shapes::Area;
use super::{Canvas, map};

/// The widest a band's track gets, in logical px.
const TRACK_MAX_WIDTH: f32 = 14.0;

fn label(centre: f32) -> String {
    if centre >= 1_000.0 {
        format!("{}k", (centre / 1_000.0).round())
    } else {
        format!("{}", centre.round())
    }
}

pub fn draw(
    c: &mut Canvas,
    area: Area,
    _settings: &StereoBandsMeterSettings,
    bands: &[BandDisplay],
) {
    if bands.is_empty() {
        return;
    }
    let dim = c.dim();
    let label_height = c.px(13.0) * c.styling.text_scale;
    let (body, labels) = area.split_bottom(label_height);
    let scale_width = c.px(26.0);
    let columns = Area {
        x: area.x + scale_width,
        width: area.width - scale_width,
        ..body
    };
    let column_width = columns.width / bands.len() as f32;
    let track_width = c.px(TRACK_MAX_WIDTH).min(column_width * 0.5).max(c.px(2.0));
    // Room above and below for the reading.
    let reading_room = c.px(13.0);
    let track_area = Area {
        y: body.y + reading_room,
        height: (body.height - 2.0 * reading_room).max(1.0),
        ..columns
    };
    let y_of = |v: f32| {
        map(
            v.clamp(-1.0, 1.0),
            (-1.0, 1.0),
            track_area.bottom(),
            track_area.y,
        )
    };

    let grid = c.colour(Role::Grid);
    for (value, text) in [(1.0, "+1"), (0.0, "0"), (-1.0, "−1")] {
        let y = y_of(value);
        c.grid_h(columns, y, grid, value == 0.0);
        c.text(
            text,
            area.x + scale_width - c.px(4.0),
            (y - c.px(6.0)).clamp(body.y, body.bottom() - c.px(12.0)),
            9.0,
            dim,
            Align::Right,
        );
    }
    let (positive, negative) = (
        c.colour(Role::CorrelationPositive),
        c.colour(Role::CorrelationNegative),
    );
    let background = c.colour(Role::Background);
    for (i, band) in bands.iter().enumerate() {
        let centre = columns.x + (i as f32 + 0.5) * column_width;
        c.text(
            &label(band.centre),
            centre,
            labels.y + c.px(1.0),
            9.0,
            dim,
            Align::Centre,
        );
        let track = Area {
            x: centre - track_width / 2.0,
            width: track_width,
            ..track_area
        };
        c.shapes
            .rounded_rect(track, track_width / 2.0, background.faded(0.8));
        let Some(value) = band.correlation else {
            continue;
        };
        let colour = if value >= 0.0 { positive } else { negative };
        let (zero, at) = (y_of(0.0), y_of(value));
        c.shapes.rect(
            Area {
                y: zero.min(at),
                height: (at - zero).abs(),
                ..track
            },
            colour.faded(0.6),
        );
        let marker = c.px(2.0);
        c.shapes.rect(
            Area {
                x: track.x - c.px(2.0),
                y: at - marker / 2.0,
                width: track_width + c.px(4.0),
                height: marker,
            },
            colour,
        );
        if body.height > c.px(60.0) && column_width > c.px(26.0) {
            let text_y = if value >= 0.0 {
                at - c.px(13.0)
            } else {
                at + c.px(2.0)
            };
            c.text(
                &format!("{value:+.2}"),
                centre,
                text_y.clamp(body.y, body.bottom() - c.px(12.0)),
                9.0,
                dim,
                Align::Centre,
            );
        }
    }
}
