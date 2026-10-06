//! The Spectrogram: the Spectrum over time, newest at the right edge,
//! log-frequency from the bottom up, each level a colour from the Theme's
//! Spectrum fill, line and accent up to its text colour at the top of the
//! range; the box being dragged to zoom into, and the zoom window showing
//! that box's frequencies finer and scrolling slower.

use dasmeter_core::meters::ZOOM_TITLE_HEIGHT;
use dasmeter_core::{Colour, Role, SpectrogramMeterSettings, SpectrogramZoom};

use super::labels::Align;
use super::shapes::Area;
use super::spectrum::{fraction_of, frequency_marks, hertz};
use super::{Canvas, map, mix};

/// Levels up to this one stay the panel's colour; the colours fade in over
/// the next [`FADE_IN`] levels, so quiet parts have no hard edge.
const FIRST_LEVEL: usize = 8;
const FADE_IN: usize = 16;

const FREQUENCY_MARKS: [(f32, &str); 8] = [
    (50.0, "50"),
    (100.0, "100"),
    (200.0, "200"),
    (500.0, "500"),
    (1_000.0, "1k"),
    (2_000.0, "2k"),
    (5_000.0, "5k"),
    (10_000.0, "10k"),
];

/// The colour of each level, from the quietest (faint fill) to the loudest.
fn colours(c: &Canvas) -> [Colour; 256] {
    let stops = [
        (0.0, c.colour(Role::SpectrumFill).faded(0.15)),
        (0.35, c.colour(Role::SpectrumFill)),
        (0.65, c.colour(Role::SpectrumLine)),
        (0.88, c.colour(Role::Accent)),
        (1.0, c.colour(Role::Text)),
    ];
    std::array::from_fn(|level| {
        let t = level as f32 / 255.0;
        let upper = stops
            .iter()
            .position(|(at, _)| *at >= t)
            .unwrap_or(stops.len() - 1);
        let lower = upper.saturating_sub(1);
        let ((a_at, a), (b_at, b)) = (stops[lower], stops[upper]);
        let span = (b_at - a_at).max(f32::EPSILON);
        let colour = mix(a, b, ((t - a_at) / span).clamp(0.0, 1.0));
        let fade = (level.saturating_sub(FIRST_LEVEL) as f32 / FADE_IN as f32).min(1.0);
        Colour {
            a: (f32::from(colour.a) * fade).round() as u8,
            ..colour
        }
    })
}

/// `area` is the whole picture with its scale, which the box and the zoom
/// window are measured in.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    c: &mut Canvas,
    area: Area,
    settings: &SpectrogramMeterSettings,
    columns: &[Vec<u8>],
    range: (f32, f32),
    lag: f32,
    selecting: Option<[f32; 4]>,
    zoom: Option<&SpectrogramZoom>,
) {
    let scale_width = if settings.show_scale { c.px(30.0) } else { 0.0 };
    let plot = Area {
        x: area.x + scale_width,
        width: (area.width - scale_width).max(1.0),
        ..area
    };
    let panel = zoom.map(|zoom| fraction_of(area, zoom.panel));
    // Labels and lines under the zoom window would show through it.
    c.covered = panel;
    let palette = colours(c);
    picture(c, plot, columns, lag, palette, None);
    if settings.show_scale {
        let marks: Vec<(f32, String)> = FREQUENCY_MARKS
            .iter()
            .map(|&(f, name)| (f, name.to_owned()))
            .collect();
        scale(c, area, plot, range, &marks, panel);
    }
    c.covered = None;

    // The box being dragged, or the one zoomed into.
    if let Some(selection) = selecting.or(zoom.map(|zoom| zoom.selection)) {
        let accent = c.colour(Role::Accent);
        let outline = fraction_of(area, selection);
        let thin = c.px(1.0).max(1.0);
        c.overlay.rect(outline, accent.faded(0.08));
        for edge in [
            Area {
                height: thin,
                ..outline
            },
            Area {
                y: outline.bottom() - thin,
                height: thin,
                ..outline
            },
            Area {
                width: thin,
                ..outline
            },
            Area {
                x: outline.right() - thin,
                width: thin,
                ..outline
            },
        ] {
            c.overlay.rect(edge, accent.faded(0.8));
        }
    }
    if let (Some(zoom), Some(panel)) = (zoom, panel) {
        draw_zoom(c, panel, zoom, palette);
    }
}

/// The picture, on the GPU: newest against the right edge, scrolling
/// smoothly by the columns still to come in.
fn picture(
    c: &mut Canvas,
    plot: Area,
    columns: &[Vec<u8>],
    lag: f32,
    palette: [Colour; 256],
    back: Option<Colour>,
) {
    let rows = columns.first().map_or(0, Vec::len);
    c.heatmap.draw(
        plot,
        columns,
        dasmeter_analysis::spectrogram::COLUMNS,
        rows,
        lag,
        palette,
        back,
    );
}

/// The frequency scale: faint lines across `plot` (around `hole`, where the
/// zoom window is), labels right-aligned on its left.
fn scale(
    c: &mut Canvas,
    area: Area,
    plot: Area,
    range: (f32, f32),
    marks: &[(f32, String)],
    hole: Option<Area>,
) {
    let (grid, dim) = (c.colour(Role::Grid), c.dim());
    let (low, high) = (range.0.max(1.0).ln(), range.1.max(1.0).ln());
    let y_of = |f: f32| map(f.max(1.0).ln(), (low, high), plot.bottom(), plot.y);
    let thin = c.px(1.0).max(1.0);
    let mut last_label: Option<f32> = None;
    for (frequency, name) in marks {
        let frequency = *frequency;
        if frequency < range.0 || frequency > range.1 {
            continue;
        }
        let y = y_of(frequency);
        let line = Area {
            y: y - thin / 2.0,
            height: thin,
            ..plot
        };
        let pieces = match hole {
            Some(hole) if y >= hole.y && y <= hole.bottom() => vec![
                Area {
                    width: (hole.x - line.x).max(0.0),
                    ..line
                },
                Area {
                    x: hole.right(),
                    width: (line.right() - hole.right()).max(0.0),
                    ..line
                },
            ],
            _ => vec![line],
        };
        for piece in pieces.into_iter().filter(|p| p.width > 0.0) {
            c.overlay.rect(piece, grid.faded(0.35));
        }
        let clear = last_label.is_none_or(|last| last - y > c.px(12.0));
        if clear && y - c.px(6.0) > area.y && y + c.px(6.0) < area.bottom() {
            c.text(
                name,
                plot.x - c.px(4.0),
                y - c.px(6.0),
                9.0,
                dim,
                Align::Right,
            );
            last_label = Some(y);
        }
    }
}

/// The zoom window: a frame and title bar with a close button, then the
/// box's frequencies as their own picture with their own scale.
fn draw_zoom(c: &mut Canvas, panel: Area, zoom: &SpectrogramZoom, palette: [Colour; 256]) {
    let (text, dim) = (c.colour(Role::Text), c.dim());
    let opaque = |colour: Colour| Colour { a: 255, ..colour };
    let back = opaque(c.colour(Role::Panel));
    // Only the frame's edges: the overlay goes over the zoom picture.
    let edge = opaque(c.colour(Role::Grid));
    let thin = c.px(1.0);
    for side in [
        Area {
            height: thin,
            ..panel
        },
        Area {
            y: panel.bottom() - thin,
            height: thin,
            ..panel
        },
        Area {
            width: thin,
            ..panel
        },
        Area {
            x: panel.right() - thin,
            width: thin,
            ..panel
        },
    ] {
        c.overlay.rect(side, edge);
    }
    let inside = panel.inset(thin);
    let title_height = c.px(ZOOM_TITLE_HEIGHT);
    let (title, body) = inside.split_top(title_height);
    c.overlay.rect(title, back);
    let label = format!("{} – {}", hertz(zoom.range.0), hertz(zoom.range.1));
    let close = Area {
        x: title.right() - title_height,
        width: title_height,
        ..title
    };
    c.text(
        &label,
        title.x + c.px(8.0),
        title.y + c.px(4.0),
        10.0,
        dim,
        Align::Left,
    );
    c.overlay
        .rect(close.inset(c.px(3.0)), c.colour(Role::Grid).faded(0.6));
    c.text(
        "×",
        close.x + close.width / 2.0,
        close.y + c.px(2.0),
        13.0,
        text,
        Align::Centre,
    );

    let scale_width = c.px(36.0);
    let plot = Area {
        x: body.x + scale_width,
        width: (body.width - scale_width - c.px(4.0)).max(1.0),
        height: (body.height - c.px(4.0)).max(1.0),
        ..body
    };
    // The scale's strip, then the picture over the panel's colour.
    c.overlay.rect(
        Area {
            width: scale_width,
            ..body
        },
        back,
    );
    c.overlay.rect(
        Area {
            x: plot.right(),
            width: (body.right() - plot.right()).max(0.0),
            ..body
        },
        back,
    );
    c.overlay.rect(
        Area {
            y: plot.bottom(),
            height: (body.bottom() - plot.bottom()).max(0.0),
            ..body
        },
        back,
    );
    picture(c, plot, &zoom.columns, zoom.lag, palette, Some(back));
    let marks = frequency_marks(zoom.range);
    scale(c, body, plot, zoom.range, &marks, None);
}
