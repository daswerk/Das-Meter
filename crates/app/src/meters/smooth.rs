//! The Smooth Look's drawing: grids that fade toward the plot's edges,
//! glows under traces, fills that fade out, and the floating panel with its
//! gradient, vignette and inner shadow. In the Classic Look each helper
//! draws as the Meters always have.

use dasmeter_core::{Colour, Look, Role};

use super::Canvas;
use super::labels::Align;
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

    /// A grid colour for lines that don't fade along their length (arcs,
    /// diagonals): as given in Classic, fainter in Smooth.
    pub fn grid_colour(&self, colour: Colour) -> Colour {
        if self.smooth() {
            colour.faded(self.grid_strength(false))
        } else {
            colour
        }
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
        let colour = self.halo(colour).faded(GLOW_ALPHA * glow);
        self.shapes.glow_polyline(points, width, colour);
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

    /// Draws the curve through `points` with `draw`, given each piece and
    /// how strongly to draw it. Smooth: piece by piece, fading out toward
    /// `plot`'s left and right ends as the grid does. Classic: all of it at
    /// once at full strength.
    pub fn edge_faded(
        &mut self,
        plot: Area,
        points: &[[f32; 2]],
        mut draw: impl FnMut(&mut Self, &[[f32; 2]], f32),
    ) {
        if !self.smooth() {
            draw(self, points, 1.0);
            return;
        }
        for i in 0..points.len().saturating_sub(1) {
            let x = (points[i][0] + points[i + 1][0]) / 2.0;
            let strength = self.edge_strength(plot, x);
            if strength > 0.0 {
                draw(self, &points[i..i + 2], strength);
            }
        }
    }

    /// How strongly a trace shows at `x` across `plot`: fading out toward
    /// the left and right ends in Smooth, as the grid does; fully in Classic.
    pub fn edge_strength(&self, plot: Area, x: f32) -> f32 {
        if !self.smooth() {
            return 1.0;
        }
        edge_fade(
            x - plot.x,
            plot.right() - x,
            plot.width,
            self.styling.grid_fade,
        )
    }

    /// `text` with a soft glow behind it in Smooth. Classic draws plain text.
    pub fn glowing_text(
        &mut self,
        text: &str,
        [x, y]: [f32; 2],
        size: f32,
        colour: Colour,
        align: Align,
    ) {
        let glow = self.styling.glow;
        if self.smooth() && glow > 0.0 {
            // An oval fading out from the middle of the digits, wider
            // than it is tall: no corners for the eye to catch.
            let tall = self.px(size) * self.styling.text_scale;
            let wide = 0.6 * tall * text.chars().count() as f32;
            let left = match align {
                Align::Left => x,
                Align::Centre => x - wide / 2.0,
                Align::Right => x - wide,
            };
            let centre = [left + wide / 2.0, y + tall * 0.55];
            let radius = [wide / 2.0 + tall * 0.6, tall * 0.9];
            let halo = self.halo(colour).faded(TEXT_GLOW_ALPHA * glow);
            self.shapes.soft_oval(centre, radius, halo);
        }
        self.text(text, x, y, size, colour, align);
    }

    /// Whether the Theme is light: its effects are shadows, not glows.
    pub fn light(&self) -> bool {
        luminance(self.colour(Role::Background)) > 0.5
    }

    /// The colour a glow around `colour` takes: itself on a dark Theme, a
    /// soft shadow of it on a light one.
    fn halo(&self, colour: Colour) -> Colour {
        if self.light() {
            super::mix(colour, Colour::rgb(0, 0, 0), 0.6).faded(0.7)
        } else {
            colour
        }
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

/// Rough perceived brightness, 0 to 1.
fn luminance(c: Colour) -> f32 {
    (0.2126 * f32::from(c.r) + 0.7152 * f32::from(c.g) + 0.0722 * f32::from(c.b)) / 255.0
}

/// 0 at a plot's edge rising smoothly to 1 at `fade` of `length` in from it,
/// for a line `before` from one edge and `after` from the other.
fn edge_fade(before: f32, after: f32, length: f32, fade: f32) -> f32 {
    let span = (length * fade).max(1.0);
    let t = (before.min(after) / span).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// How strong the glow behind big numbers is at its middle, at full glow.
const TEXT_GLOW_ALPHA: f32 = 0.08;

/// Rings the vignette is built from: each a little further in, lighter.
const VIGNETTE_RINGS: usize = 20;

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
    // On a light Theme the darkening shows far more: kept gentler there.
    let edge = if c.light() { 0.2 } else { 0.45 } * s.vignette;
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
                .rounded_gradient(inner, ring_radius, t.faded(0.13), b.faded(0.13));
        }
    }
    // A faint light edge along the top, fading out well before the corners.
    let (left, right) = (area.x + radius, area.right() - radius);
    let thin = c.px(1.0).max(1.0);
    c.shapes.faded_hline(
        left,
        right,
        area.y + thin / 2.0,
        thin,
        white.faded(TOP_EDGE_ALPHA * opacity),
        (right - left) * 0.35,
    );
}

/// How bright the light edge along a panel's top is, at its middle.
const TOP_EDGE_ALPHA: f32 = 0.035;

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
