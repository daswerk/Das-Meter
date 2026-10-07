//! The Smooth Look's drawing: grids that fade toward the plot's edges,
//! glows under traces, fills that fade out, and the floating panel with its
//! gradient, vignette and inner shadow. In the Classic Look each helper
//! draws as the Meters always have.

use dasmeter_core::{Colour, Look, Role};

use super::Canvas;
use super::shapes::Area;

/// How much fainter a Smooth grid line is than a Classic one.
const GRID_STRENGTH: f32 = 0.6;
/// A reference line (0 dB, the target, 1 kHz) against the rest of the grid.
const REFERENCE_STRENGTH: f32 = 1.9;
/// The grid while the pointer is over the Meter.
const HOVER_STRENGTH: f32 = 1.35;
/// How wide a trace's glow spreads each side, in logical px, at full glow.
const GLOW_WIDTH: f32 = 7.0;
/// How strong the glow is at the line, at full glow.
const GLOW_ALPHA: f32 = 0.32;
/// How much a Meter dims in silence (at activity 0).
pub const SILENT_DIM: f32 = 0.5;

impl Canvas<'_> {
    pub fn smooth(&self) -> bool {
        self.styling.look == Look::Smooth
    }

    /// A grid line across `plot` at `y`, in `colour` as Classic draws it.
    /// Smooth: fainter, fading out toward the plot's left and right ends and
    /// fainter the nearer it lies to the top or bottom.
    pub fn grid_h(&mut self, plot: Area, y: f32, colour: Colour, reference: bool) {
        let thin = self.px(1.0).max(1.0);
        if !self.smooth() {
            self.shapes.rect(
                Area {
                    y: y - thin / 2.0,
                    height: thin,
                    ..plot
                },
                colour,
            );
            return;
        }
        let near = edge_fade(
            y - plot.y,
            plot.bottom() - y,
            plot.height,
            self.styling.grid_fade,
        );
        let colour = colour.faded(self.grid_strength(reference) * near);
        let fade = plot.width * self.styling.grid_fade;
        self.shapes
            .faded_hline(plot.x, plot.right(), y, thin, colour, fade);
    }

    /// A grid line down `plot` at `x`; see [`Canvas::grid_h`].
    pub fn grid_v(&mut self, plot: Area, x: f32, colour: Colour, reference: bool) {
        let thin = self.px(1.0).max(1.0);
        if !self.smooth() {
            self.shapes.rect(
                Area {
                    x: x - thin / 2.0,
                    width: thin,
                    ..plot
                },
                colour,
            );
            return;
        }
        let near = edge_fade(
            x - plot.x,
            plot.right() - x,
            plot.width,
            self.styling.grid_fade,
        );
        let colour = colour.faded(self.grid_strength(reference) * near);
        let fade = plot.height * self.styling.grid_fade;
        self.shapes
            .faded_vline(plot.y, plot.bottom(), x, thin, colour, fade);
    }

    fn grid_strength(&self, reference: bool) -> f32 {
        let mut strength = GRID_STRENGTH;
        if reference {
            strength *= REFERENCE_STRENGTH;
        }
        if self.hovered {
            strength *= HOVER_STRENGTH;
        }
        strength.min(1.0)
    }

    /// A soft glow under the curve through `points`, Smooth only.
    pub fn glow(&mut self, points: &[[f32; 2]], colour: Colour) {
        let glow = self.styling.glow;
        if !self.smooth() || glow <= 0.0 {
            return;
        }
        let width = self.px(GLOW_WIDTH) * (0.5 + 0.5 * glow);
        self.shapes
            .glow_polyline(points, width, colour.faded(GLOW_ALPHA * glow));
    }

    /// The fill under the curve through `points` down to `base`, shaded
    /// from `top` at the line to `bottom` at the base; Smooth fades it out to
    /// nothing at the base.
    pub fn fill_under(&mut self, points: &[[f32; 2]], base: f32, top: Colour, bottom: Colour) {
        let bottom = if self.smooth() {
            top.faded(0.0)
        } else {
            bottom
        };
        self.shapes.fill_under(points, base, top, bottom);
    }

    /// A soft glow behind text or a reading in `area`, Smooth only.
    pub fn text_glow(&mut self, area: Area, colour: Colour) {
        let glow = self.styling.glow;
        if !self.smooth() || glow <= 0.0 {
            return;
        }
        let spread = self.px(14.0);
        self.shapes
            .soft_rect(area.inset(self.px(4.0)), spread, colour.faded(0.1 * glow));
    }

    /// A soft rounded backdrop under a readout over the traces, Smooth only.
    pub fn backdrop(&mut self, area: Area) {
        if !self.smooth() {
            return;
        }
        let panel = self.colour(Role::Panel).faded(0.78);
        let radius = self.px(5.0);
        self.overlay
            .soft_rect(area.inset(radius / 2.0), radius, panel);
    }
}

/// 0 at a plot's edge rising smoothly to 1 at `fade` of `length` in from it,
/// for a line `before` from one edge and `after` from the other.
fn edge_fade(before: f32, after: f32, length: f32, fade: f32) -> f32 {
    let span = (length * fade).max(1.0);
    let t = (before.min(after) / span).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Rings the vignette is built from: each a little further in, lighter.
const VIGNETTE_RINGS: usize = 8;

/// The floating panel behind a Meter in the Smooth Look: a gradient a
/// little lighter at the top, darker toward its edges (vignette), and a
/// faint light edge along the top.
pub fn panel(c: &mut Canvas, area: Area, radius: f32, opacity: f32) {
    let s = c.styling;
    let panel = c.colour(Role::Panel);
    let (white, black) = (Colour::rgb(255, 255, 255), Colour::rgb(0, 0, 0));
    let top = super::mix(panel, white, 0.05 * s.gradient).faded(opacity);
    let bottom = super::mix(panel, black, 0.22 * s.gradient).faded(opacity);
    // Darker at the edge, then rings of the panel's own shading laid over
    // it further and further in: the middle ends up the panel colour.
    let edge = 0.45 * s.vignette;
    let darken = |colour: Colour| super::mix(colour, black.faded(colour.a as f32 / 255.0), edge);
    c.shapes
        .rounded_gradient(area, radius, darken(top), darken(bottom));
    if edge > 0.0 {
        let depth = area.width.min(area.height) * 0.25;
        for ring in 1..=VIGNETTE_RINGS {
            let inset = depth * ring as f32 / VIGNETTE_RINGS as f32;
            let inner = area.inset(inset);
            let ring_radius = (radius - inset * 0.5).max(radius * 0.5);
            let (t, b) = (
                super::mix(top, bottom, (inner.y - area.y) / area.height),
                super::mix(top, bottom, (inner.bottom() - area.y) / area.height),
            );
            c.shapes
                .rounded_gradient(inner, ring_radius, t.faded(0.3), b.faded(0.3));
        }
    }
    let line = Area {
        x: area.x + radius,
        width: (area.width - 2.0 * radius).max(0.0),
        height: c.px(1.0).max(1.0),
        ..area
    };
    c.shapes.rect(line, white.faded(0.06 * opacity));
}

/// The window under the Meters in the Smooth Look: the background a little
/// lighter at the top, and a soft shadow under each Meter's panel.
pub fn backdrop(c: &mut Canvas, window: Area, panels: &[Area], opacity: f32) {
    let s = c.styling;
    let background = c.colour(Role::Background);
    let (white, black) = (Colour::rgb(255, 255, 255), Colour::rgb(0, 0, 0));
    let top = super::mix(background, white, 0.04 * s.gradient);
    let bottom = super::mix(background, black, 0.3 * s.gradient);
    c.shapes
        .gradient(window, top.faded(opacity), bottom.faded(opacity));
    let spread = c.px(s.gap.clamp(2.0, 10.0));
    for panel in panels {
        let lowered = Area {
            y: panel.y + spread * 0.3,
            ..*panel
        }
        .inset(spread * 0.4);
        c.shapes
            .soft_rect(lowered, spread, black.faded(0.35 * opacity));
    }
}
