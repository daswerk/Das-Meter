//! A Meter's shapes: quads (rectangles, thick line segments, gradient fills)
//! staged each frame into one small instance buffer and drawn in one call.

use dasmeter_core::Colour;

use crate::gpu::{Gpu, linear};

/// Floats per instance: top-left, top-right, bottom-left, bottom-right corners, then two colours.
const FLOATS: usize = 16;
const STRIDE: u64 = (FLOATS * size_of::<f32>()) as u64;
/// How far a line's edge fades out, in physical pixels.
const FEATHER: f32 = 1.0;

/// A rectangle in physical pixels, top-left origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Area {
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// The area shrunk by `by` on every side.
    pub fn inset(&self, by: f32) -> Area {
        Area {
            x: self.x + by,
            y: self.y + by,
            width: (self.width - 2.0 * by).max(0.0),
            height: (self.height - 2.0 * by).max(0.0),
        }
    }

    /// Splits off the top `height` pixels: (top, rest).
    pub fn split_top(&self, height: f32) -> (Area, Area) {
        let height = height.clamp(0.0, self.height);
        (
            Area { height, ..*self },
            Area {
                y: self.y + height,
                height: self.height - height,
                ..*self
            },
        )
    }

    /// Splits off the bottom `height` pixels: (rest, bottom).
    pub fn split_bottom(&self, height: f32) -> (Area, Area) {
        let (rest, bottom) = self.split_top(self.height - height.clamp(0.0, self.height));
        (rest, bottom)
    }

    /// Splits off the right `width` pixels: (rest, right).
    pub fn split_right(&self, width: f32) -> (Area, Area) {
        let width = width.clamp(0.0, self.width);
        (
            Area {
                width: self.width - width,
                ..*self
            },
            Area {
                x: self.right() - width,
                width,
                ..*self
            },
        )
    }
}

pub struct Shapes {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    capacity: u64,
    count: u32,
    staged: Vec<f32>,
    /// Surface size and format, for converting to clip space and linear colour.
    size: (f32, f32),
    format: wgpu::TextureFormat,
    scissor: Option<(u32, u32, u32, u32)>,
}

impl Shapes {
    pub fn new(gpu: &Gpu) -> Shapes {
        let shader = gpu
            .device
            .create_shader_module(wgpu::include_wgsl!("shapes.wgsl"));
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor::default());
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("shapes"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: STRIDE,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4
                        ],
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: gpu.format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                cache: None,
                multiview_mask: None,
            });
        let capacity = 256;
        Shapes {
            pipeline,
            buffer: instance_buffer(gpu, capacity),
            capacity,
            count: 0,
            staged: Vec::new(),
            size: (1.0, 1.0),
            format: gpu.format,
            scissor: None,
        }
    }

    /// Starts a new frame's shapes, clipped to `clip`.
    pub fn begin(&mut self, gpu: &Gpu, clip: Area) {
        self.staged.clear();
        let (width, height) = gpu.size();
        self.size = (width as f32, height as f32);
        let x = clip.x.max(0.0) as u32;
        let y = clip.y.max(0.0) as u32;
        let right = (clip.right().max(0.0) as u32).min(width);
        let bottom = (clip.bottom().max(0.0) as u32).min(height);
        self.scissor = (right > x && bottom > y).then(|| (x, y, right - x, bottom - y));
    }

    /// A quad from its corners (top-left, top-right, bottom-left, bottom-right),
    /// shaded from `top` to `bottom`.
    pub fn quad(&mut self, corners: [[f32; 2]; 4], top: Colour, bottom: Colour) {
        if top.a == 0 && bottom.a == 0 {
            return;
        }
        let (width, height) = self.size;
        for [x, y] in corners {
            self.staged.push(x / width * 2.0 - 1.0);
            self.staged.push(1.0 - y / height * 2.0);
        }
        self.staged.extend_from_slice(&linear(top, self.format));
        self.staged.extend_from_slice(&linear(bottom, self.format));
    }

    pub fn rect(&mut self, area: Area, colour: Colour) {
        self.gradient(area, colour, colour);
    }

    /// A rectangle with rounded corners of `radius` pixels.
    pub fn rounded_rect(&mut self, area: Area, radius: f32, colour: Colour) {
        let r = radius.min(area.width / 2.0).min(area.height / 2.0);
        if r < 0.5 {
            self.rect(area, colour);
            return;
        }
        let (x0, y0, x1, y1) = (area.x, area.y, area.right(), area.bottom());
        // The middle column, then the side strips between the corners.
        self.rect(
            Area {
                x: x0 + r,
                width: area.width - 2.0 * r,
                ..area
            },
            colour,
        );
        for x in [x0, x1 - r] {
            self.rect(
                Area {
                    x,
                    y: y0 + r,
                    width: r,
                    height: area.height - 2.0 * r,
                },
                colour,
            );
        }
        // Each corner as a fan of thin triangles around its centre.
        const STEPS: usize = 6;
        let corners = [
            ([x0 + r, y0 + r], std::f32::consts::PI),
            ([x1 - r, y0 + r], 1.5 * std::f32::consts::PI),
            ([x1 - r, y1 - r], 0.0),
            ([x0 + r, y1 - r], 0.5 * std::f32::consts::PI),
        ];
        for (centre, start) in corners {
            let point = |i: usize| {
                let angle = start + std::f32::consts::FRAC_PI_2 * i as f32 / STEPS as f32;
                [centre[0] + r * angle.cos(), centre[1] + r * angle.sin()]
            };
            for i in 0..STEPS {
                self.quad([point(i), point(i + 1), centre, centre], colour, colour);
            }
        }
    }

    /// A rectangle shaded from `top` to `bottom`.
    pub fn gradient(&mut self, area: Area, top: Colour, bottom: Colour) {
        let (x0, y0, x1, y1) = (area.x, area.y, area.right(), area.bottom());
        self.quad([[x0, y0], [x1, y0], [x0, y1], [x1, y1]], top, bottom);
    }

    /// A line from `from` to `to`, `thickness` pixels wide, its long edges
    /// fading out over a pixel so it doesn't stair-step (there's no MSAA).
    pub fn line(&mut self, from: [f32; 2], to: [f32; 2], thickness: f32, colour: Colour) {
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        let length = (dx * dx + dy * dy).sqrt();
        if length == 0.0 {
            return;
        }
        // A solid core one pixel narrower than asked, and a pixel of fade
        // each side: the same ink as a hard line of `thickness`. Thinner than
        // a pixel, no core, just a fainter fade.
        let core = (thickness - FEATHER).max(0.0);
        let colour = if thickness < FEATHER {
            colour.faded(thickness / FEATHER)
        } else {
            colour
        };
        let (ux, uy) = (-dy / length, dx / length);
        let side = |d: f32| {
            [
                [from[0] + ux * d, from[1] + uy * d],
                [to[0] + ux * d, to[1] + uy * d],
            ]
        };
        let (inner, outer) = (core / 2.0, core / 2.0 + FEATHER);
        let clear = colour.faded(0.0);
        let ([a, b], [c, d]) = (side(inner), side(-inner));
        if core > 0.0 {
            self.quad([a, b, c, d], colour, colour);
        }
        let ([e, f], [g, h]) = (side(outer), side(-outer));
        self.quad([e, f, a, b], clear, colour);
        self.quad([c, d, g, h], colour, clear);
    }

    /// Lines through `points`, in order.
    pub fn polyline(&mut self, points: &[[f32; 2]], thickness: f32, colour: Colour) {
        for pair in points.windows(2) {
            self.line(pair[0], pair[1], thickness, colour);
        }
    }

    /// The area between the polyline through `points` and the horizontal line at
    /// `base`, shaded from `top` (at the line) to `bottom` (at the base).
    pub fn fill_under(&mut self, points: &[[f32; 2]], base: f32, top: Colour, bottom: Colour) {
        for pair in points.windows(2) {
            let ([x0, y0], [x1, y1]) = (pair[0], pair[1]);
            self.quad([[x0, y0], [x1, y1], [x0, base], [x1, base]], top, bottom);
        }
    }

    /// A soft band along `from`–`to`, `width` pixels each side of the line,
    /// fading from `colour` at the line to nothing: a glow under a trace.
    pub fn glow_line(&mut self, from: [f32; 2], to: [f32; 2], width: f32, colour: Colour) {
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        let length = (dx * dx + dy * dy).sqrt();
        if length == 0.0 || width <= 0.0 {
            return;
        }
        let (ux, uy) = (-dy / length * width, dx / length * width);
        let clear = colour.faded(0.0);
        let side = |s: f32| {
            [
                [from[0] + ux * s, from[1] + uy * s],
                [to[0] + ux * s, to[1] + uy * s],
            ]
        };
        let ([a, b], [e, f], [g, h]) = (side(0.0), side(1.0), side(-1.0));
        self.quad([e, f, a, b], clear, colour);
        self.quad([a, b, g, h], colour, clear);
    }

    /// Glows along the polyline through `points`.
    pub fn glow_polyline(&mut self, points: &[[f32; 2]], width: f32, colour: Colour) {
        for pair in points.windows(2) {
            self.glow_line(pair[0], pair[1], width, colour);
        }
    }

    /// A horizontal line from `x0` to `x1` at `y`, fading in over `fade`
    /// pixels at each end.
    pub fn faded_hline(
        &mut self,
        x0: f32,
        x1: f32,
        y: f32,
        thickness: f32,
        colour: Colour,
        fade: f32,
    ) {
        let fade = fade.min((x1 - x0) / 2.0).max(0.0);
        let (top, bottom) = (y - thickness / 2.0, y + thickness / 2.0);
        let clear = colour.faded(0.0);
        // Turned on its side, a quad's "top" and "bottom" colours run left to right.
        let across = |shapes: &mut Shapes, a: f32, b: f32, from: Colour, to: Colour| {
            shapes.quad([[a, top], [a, bottom], [b, top], [b, bottom]], from, to);
        };
        across(self, x0, x0 + fade, clear, colour);
        self.rect(
            Area {
                x: x0 + fade,
                y: top,
                width: (x1 - x0 - 2.0 * fade).max(0.0),
                height: thickness,
            },
            colour,
        );
        across(self, x1 - fade, x1, colour, clear);
    }

    /// A vertical line from `y0` to `y1` at `x`, fading in over `fade`
    /// pixels at each end.
    pub fn faded_vline(
        &mut self,
        y0: f32,
        y1: f32,
        x: f32,
        thickness: f32,
        colour: Colour,
        fade: f32,
    ) {
        let fade = fade.min((y1 - y0) / 2.0).max(0.0);
        let clear = colour.faded(0.0);
        let column = |y: f32, height: f32| Area {
            x: x - thickness / 2.0,
            y,
            width: thickness,
            height,
        };
        self.gradient(column(y0, fade), clear, colour);
        self.rect(column(y0 + fade, (y1 - y0 - 2.0 * fade).max(0.0)), colour);
        self.gradient(column(y1 - fade, fade), colour, clear);
    }

    /// A rounded rectangle shaded from `top` to `bottom`.
    pub fn rounded_gradient(&mut self, area: Area, radius: f32, top: Colour, bottom: Colour) {
        let r = radius.min(area.width / 2.0).min(area.height / 2.0);
        if r < 0.5 {
            self.gradient(area, top, bottom);
            return;
        }
        let at = |y: f32| super::mix(top, bottom, ((y - area.y) / area.height).clamp(0.0, 1.0));
        let (x0, y0, x1, y1) = (area.x, area.y, area.right(), area.bottom());
        self.gradient(
            Area {
                x: x0 + r,
                width: area.width - 2.0 * r,
                ..area
            },
            top,
            bottom,
        );
        for x in [x0, x1 - r] {
            self.gradient(
                Area {
                    x,
                    y: y0 + r,
                    width: r,
                    height: area.height - 2.0 * r,
                },
                at(y0 + r),
                at(y1 - r),
            );
        }
        const STEPS: usize = 6;
        let corners = [
            ([x0 + r, y0 + r], std::f32::consts::PI),
            ([x1 - r, y0 + r], 1.5 * std::f32::consts::PI),
            ([x1 - r, y1 - r], 0.0),
            ([x0 + r, y1 - r], 0.5 * std::f32::consts::PI),
        ];
        for (centre, start) in corners {
            let colour = at(centre[1]);
            let point = |i: usize| {
                let angle = start + std::f32::consts::FRAC_PI_2 * i as f32 / STEPS as f32;
                [centre[0] + r * angle.cos(), centre[1] + r * angle.sin()]
            };
            for i in 0..STEPS {
                self.quad([point(i), point(i + 1), centre, centre], colour, colour);
            }
        }
    }

    /// Shading `width` pixels in from each edge of `area`, `colour` at the
    /// edge fading to nothing inward: an inner shadow or a vignette. The
    /// first `radius` pixels of each edge are left out for rounded corners.
    pub fn inner_shade(&mut self, area: Area, radius: f32, width: f32, colour: Colour) {
        let width = width.min(area.width / 2.0).min(area.height / 2.0);
        if width <= 0.0 || colour.a == 0 {
            return;
        }
        let clear = colour.faded(0.0);
        let (x0, y0, x1, y1) = (area.x, area.y, area.right(), area.bottom());
        let (inner_x0, inner_x1) = (x0 + radius, x1 - radius);
        let (inner_y0, inner_y1) = (y0 + radius, y1 - radius);
        // Top and bottom, as trapezoids so the corners meet the sides.
        self.quad(
            [
                [inner_x0, y0],
                [inner_x1, y0],
                [x0 + width.max(radius), y0 + width],
                [x1 - width.max(radius), y0 + width],
            ],
            colour,
            clear,
        );
        self.quad(
            [
                [x0 + width.max(radius), y1 - width],
                [x1 - width.max(radius), y1 - width],
                [inner_x0, y1],
                [inner_x1, y1],
            ],
            clear,
            colour,
        );
        // Left and right, on their sides.
        self.quad(
            [
                [x0, inner_y0],
                [x0, inner_y1],
                [x0 + width, y0 + width.max(radius)],
                [x0 + width, y1 - width.max(radius)],
            ],
            colour,
            clear,
        );
        self.quad(
            [
                [x1 - width, y0 + width.max(radius)],
                [x1 - width, y1 - width.max(radius)],
                [x1, inner_y0],
                [x1, inner_y1],
            ],
            clear,
            colour,
        );
    }

    /// `area` in `colour`, with `spread` pixels of soft edge fading out
    /// around it: a glow behind text, or a shadow under a panel.
    pub fn soft_rect(&mut self, area: Area, spread: f32, colour: Colour) {
        if colour.a == 0 {
            return;
        }
        self.rect(area, colour);
        if spread <= 0.0 {
            return;
        }
        let clear = colour.faded(0.0);
        let (x0, y0, x1, y1) = (area.x, area.y, area.right(), area.bottom());
        let (ox0, oy0, ox1, oy1) = (x0 - spread, y0 - spread, x1 + spread, y1 + spread);
        self.quad([[ox0, oy0], [ox1, oy0], [x0, y0], [x1, y0]], clear, colour);
        self.quad([[x0, y1], [x1, y1], [ox0, oy1], [ox1, oy1]], colour, clear);
        self.quad([[ox0, oy0], [ox0, oy1], [x0, y0], [x0, y1]], clear, colour);
        self.quad([[x1, y0], [x1, y1], [ox1, oy0], [ox1, oy1]], colour, clear);
    }

    /// Uploads the staged shapes.
    pub fn prepare(&mut self, gpu: &Gpu) {
        let count = (self.staged.len() / FLOATS) as u64;
        if count > self.capacity {
            self.capacity = count.next_power_of_two();
            self.buffer = instance_buffer(gpu, self.capacity);
        }
        gpu.queue
            .write_buffer(&self.buffer, 0, bytemuck::cast_slice(&self.staged));
        self.count = count as u32;
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass) {
        // The scissor also clips the text drawn after the shapes.
        let Some((x, y, width, height)) = self.scissor else {
            return;
        };
        pass.set_scissor_rect(x, y, width, height);
        if self.count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.buffer.slice(..u64::from(self.count) * STRIDE));
        pass.draw(0..4, 0..self.count);
    }
}

fn instance_buffer(gpu: &Gpu, capacity: u64) -> wgpu::Buffer {
    gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shape instances"),
        size: capacity * STRIDE,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// The curve through `points` (left to right) with a point added at least
/// every `step` pixels, rounded between them (Catmull-Rom on the heights)
/// so a line through few points reads as a curve, not a chain of corners.
/// Never goes past the highest or lowest of the four points around a span,
/// so peaks don't overshoot.
pub fn smooth(points: &[[f32; 2]], step: f32) -> Vec<[f32; 2]> {
    let Some(&last) = points.last() else {
        return Vec::new();
    };
    let mut curve = Vec::with_capacity(points.len() * 2);
    for i in 0..points.len() - 1 {
        let (a, b) = (points[i], points[i + 1]);
        let before = points[i.saturating_sub(1)][1];
        let after = points.get(i + 2).map_or(b[1], |p| p[1]);
        let (low, high) = (
            a[1].min(b[1]).min(before).min(after),
            a[1].max(b[1]).max(before).max(after),
        );
        let steps = ((b[0] - a[0]).abs() / step.max(0.1)).ceil().max(1.0) as usize;
        curve.push(a);
        for k in 1..steps {
            let t = k as f32 / steps as f32;
            let y = dasmeter_analysis::catmull_rom([before, a[1], b[1], after], t);
            curve.push([a[0] + (b[0] - a[0]) * t, y.clamp(low, high)]);
        }
    }
    curve.push(last);
    curve
}

#[cfg(test)]
mod tests {
    use super::smooth;

    #[test]
    fn a_smoothed_curve_keeps_its_points_and_fills_the_gaps() {
        let points = [[0.0, 10.0], [8.0, 0.0], [16.0, 10.0], [24.0, 10.0]];
        let curve = smooth(&points, 2.0);
        for p in points {
            assert!(curve.contains(&p), "{p:?} kept");
        }
        for pair in curve.windows(2) {
            assert!(
                pair[1][0] - pair[0][0] <= 2.0 + 1e-4,
                "no gap wider than asked"
            );
            assert!(pair[1][0] > pair[0][0], "still left to right");
        }
        // Rounded between the points, and never past the peak or the floor.
        let between = curve.iter().find(|p| p[0] == 4.0).unwrap();
        assert!(between[1] < 5.0, "bends toward the peak: {between:?}");
        assert!(curve.iter().all(|p| (0.0..=10.0).contains(&p[1])));
        // A flat run stays flat.
        assert!(
            curve
                .iter()
                .filter(|p| p[0] > 16.0)
                .all(|p| (p[1] - 10.0).abs() < 1e-4)
        );
    }

    #[test]
    fn close_points_are_left_alone() {
        let points = [[0.0, 0.0], [1.0, 5.0], [2.0, 0.0]];
        assert_eq!(smooth(&points, 2.0), points.to_vec());
    }
}
