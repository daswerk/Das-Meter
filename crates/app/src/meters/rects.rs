//! A small instanced-rectangle pipeline: a Meter renderer fills a buffer of
//! rectangles each frame and draws them in one call.

use crate::gpu::{Colour, Gpu};

/// Floats per instance: a rectangle's corners (x0, y0, x1, y1) and an RGBA colour.
const FLOATS: usize = 8;
const STRIDE: u64 = (FLOATS * size_of::<f32>()) as u64;

/// A rectangle in physical pixels, top-left origin.
#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub struct Rects {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    capacity: u64,
    count: u32,
    staged: Vec<f32>,
}

impl Rects {
    pub fn new(gpu: &Gpu) -> Rects {
        let shader = gpu
            .device
            .create_shader_module(wgpu::include_wgsl!("rects.wgsl"));
        let layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor::default());
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("rects"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: STRIDE,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4],
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
        let capacity = 64;
        Rects {
            pipeline,
            buffer: instance_buffer(gpu, capacity),
            capacity,
            count: 0,
            staged: Vec::new(),
        }
    }

    /// Starts a new frame's rectangles.
    pub fn clear(&mut self) {
        self.staged.clear();
    }

    pub fn push(&mut self, gpu: &Gpu, rect: Rect, colour: Colour) {
        let (width, height) = gpu.size();
        let (width, height) = (width as f32, height as f32);
        let x = |px: f32| px / width * 2.0 - 1.0;
        let y = |px: f32| 1.0 - px / height * 2.0;
        self.staged.extend_from_slice(&[
            x(rect.x),
            y(rect.y),
            x(rect.x + rect.width),
            y(rect.y + rect.height),
        ]);
        self.staged
            .extend_from_slice(&colour.for_surface(gpu.format));
    }

    /// Uploads the staged rectangles.
    pub fn prepare(&mut self, gpu: &Gpu) {
        let count = (self.staged.len() / FLOATS) as u64;
        if count > self.capacity {
            self.capacity = count.next_power_of_two();
            self.buffer = instance_buffer(gpu, self.capacity);
        }
        let bytes: Vec<u8> = self.staged.iter().flat_map(|f| f.to_ne_bytes()).collect();
        gpu.queue.write_buffer(&self.buffer, 0, &bytes);
        self.count = count as u32;
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass) {
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
        label: Some("rect instances"),
        size: capacity * STRIDE,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
