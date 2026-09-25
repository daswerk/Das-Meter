//! A Meter's shapes: quads (rectangles, thick line segments, gradient fills)
//! staged each frame into one small instance buffer and drawn in one call.

use dasmeter_core::Colour;

use crate::gpu::{Gpu, linear};

/// Floats per instance: top-left, top-right, bottom-left, bottom-right corners, then two colours.
const FLOATS: usize = 16;
const STRIDE: u64 = (FLOATS * size_of::<f32>()) as u64;

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

    /// A line from `from` to `to`, `thickness` pixels wide.
    pub fn line(&mut self, from: [f32; 2], to: [f32; 2], thickness: f32, colour: Colour) {
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        let length = (dx * dx + dy * dy).sqrt();
        if length == 0.0 {
            return;
        }
        let (nx, ny) = (
            -dy / length * thickness / 2.0,
            dx / length * thickness / 2.0,
        );
        self.quad(
            [
                [from[0] + nx, from[1] + ny],
                [to[0] + nx, to[1] + ny],
                [from[0] - nx, from[1] - ny],
                [to[0] - nx, to[1] - ny],
            ],
            colour,
            colour,
        );
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
