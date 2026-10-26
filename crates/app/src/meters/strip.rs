//! The Waveform's picture on the GPU (`strip.wgsl`): its columns, merged
//! into groups of about a pixel by their number in time, are written once
//! into a ring texture and stay there; each frame only moves the camera.
//! Only the newest groups go up, so a frame costs a few texels; the camera
//! moves by whole pixels and each pixel shows one group as it is, so a group
//! never changes its look while it scrolls: the Waveform can't flicker.

use dasmeter_analysis::WaveformColumn;
use dasmeter_core::{Colour, WaveformColouring};

use super::shapes::Area;
use crate::gpu::Gpu;

/// Bytes in the uniform block: nine `vec4`s.
const UNIFORM_SIZE: u64 = 9 * 16;
/// Texels per group and lane: (min, max, low, mid), (high, …).
const ROWS_PER_LANE: usize = 2;
/// Auto gain: the loudest part shown is brought up to this height…
const AUTO_HEIGHT: f32 = 0.8;
/// …by at most this much (+30 dB), and eases up by this factor a frame
/// (it comes down at once, so a loud bit is never cut off).
pub const MAX_BOOST: f32 = 31.6;
const BOOST_RISE: f32 = 1.015;

/// One frame's picture, staged until [`Strip::prepare`] uploads it.
pub struct Picture {
    pub area: Area,
    /// Per lane, its groups newest first.
    pub lanes: Vec<Vec<WaveformColumn>>,
    /// The newest group's number in time.
    pub newest: u64,
    /// How many groups the ring must hold.
    pub capacity: usize,
    /// Columns per group (1 when a column is a pixel or wider).
    pub per_group: f32,
    /// A column's width in px.
    pub column_width: f32,
    /// Where the right edge is in time, in groups after the newest's start
    /// (the core's lag keeps it a little behind the newest column).
    pub camera: f32,
    pub gain: f32,
    pub decibels: bool,
    /// Bring a quiet Waveform up to a good height.
    pub auto_gain: bool,
    pub colouring: WaveformColouring,
    /// Low, mid and high.
    pub bands: [Colour; 3],
    /// Space kept above and below each lane, in px.
    pub margin: f32,
}

/// What the texture holds, so a frame only writes what's new.
#[derive(PartialEq, Clone, Copy)]
struct Shape {
    width: usize,
    lanes: usize,
    per_group: u32,
}

struct Ring {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    shape: Shape,
    /// The newest group written (it was still filling, so it's written again).
    written: Option<u64>,
}

pub struct Strip {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
    ring: Option<Ring>,
    staged: Option<Picture>,
    ready: bool,
    /// The auto gain now, eased between frames.
    boost: f32,
    srgb: bool,
    max_width: usize,
}

impl Strip {
    pub fn new(gpu: &Gpu) -> Strip {
        let shader = gpu
            .device
            .create_shader_module(wgpu::include_wgsl!("strip.wgsl"));
        let layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("waveform"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            });
        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("waveform"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("waveform"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[],
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
        let uniform = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("waveform"),
            size: UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Strip {
            pipeline,
            layout,
            uniform,
            ring: None,
            staged: None,
            ready: false,
            boost: 1.0,
            srgb: gpu.format.is_srgb(),
            max_width: gpu.device.limits().max_texture_dimension_2d as usize,
        }
    }

    /// Starts a frame without a picture.
    pub fn begin(&mut self) {
        self.staged = None;
        self.ready = false;
    }

    /// This frame's picture.
    pub fn draw(&mut self, picture: Picture) {
        self.staged = Some(picture);
    }

    /// Writes the new groups and moves the camera.
    pub fn prepare(&mut self, gpu: &Gpu) {
        let Some(picture) = self.staged.take() else {
            return;
        };
        let lanes = picture.lanes.len();
        let stored = picture.lanes.iter().map(Vec::len).max().unwrap_or(0);
        if lanes == 0 || stored == 0 || picture.area.width < 1.0 {
            return;
        }
        let shape = Shape {
            width: (picture.capacity + 2).clamp(2, self.max_width),
            lanes,
            per_group: picture.per_group.to_bits(),
        };
        if self.ring.as_ref().is_none_or(|r| r.shape != shape) {
            self.ring = Some(self.ring_for(gpu, shape));
        }
        let Some(ring) = &mut self.ring else {
            return;
        };
        let stored = stored.min(shape.width);
        // New groups since the last frame, the one that was filling again;
        // after a jump back in numbering (the count restarted), all of them.
        let fresh = match ring.written {
            Some(written) if written <= picture.newest => {
                ((picture.newest - written) as usize + 1).min(stored)
            }
            _ => stored,
        };
        write_groups(gpu, ring, &picture.lanes, picture.newest, fresh);
        ring.written = Some(picture.newest);

        // Auto gain: the loudest part on show, brought up to a good height.
        let gain = if picture.auto_gain {
            let peak = picture
                .lanes
                .iter()
                .flat_map(|groups| groups.iter().take(stored))
                .fold(0.0f32, |peak, c| peak.max(c.max).max(-c.min));
            let wanted = if peak > 1e-6 {
                (AUTO_HEIGHT / (peak * picture.gain)).clamp(1.0, MAX_BOOST)
            } else {
                self.boost
            };
            self.boost = wanted.min(self.boost * BOOST_RISE).max(1.0);
            picture.gain * self.boost
        } else {
            self.boost = 1.0;
            picture.gain
        };

        // The camera moves by whole pixels, so the picture only ever shifts,
        // placed so pixel centres fall mid-group (never on a boundary, where
        // rounding could pick either neighbour from frame to frame).
        let group_px = picture.column_width * picture.per_group;
        let edge = picture.area.right().fract();
        let camera = ((picture.camera * group_px - edge).round() + edge) / group_px;
        let (width, height) = gpu.size();
        let (width, height) = (width as f32, height as f32);
        let area = picture.area;
        let colour = |c: Colour| {
            [
                f32::from(c.r) / 255.0,
                f32::from(c.g) / 255.0,
                f32::from(c.b) / 255.0,
                1.0,
            ]
        };
        let ceiling = picture
            .bands
            .iter()
            .map(|c| f32::from(c.r.max(c.g).max(c.b)) / 255.0)
            .fold(1.0 / 255.0, f32::max);
        let mode = match picture.colouring {
            WaveformColouring::Solid => 0.0,
            WaveformColouring::Rgb => 1.0,
            WaveformColouring::Rekordbox => 2.0,
        };
        let mut uniform: Vec<f32> = Vec::with_capacity((UNIFORM_SIZE / 4) as usize);
        uniform.extend_from_slice(&[
            area.x / width * 2.0 - 1.0,
            1.0 - area.y / height * 2.0,
            area.right() / width * 2.0 - 1.0,
            1.0 - area.bottom() / height * 2.0,
            area.x,
            area.y,
            area.width,
            area.height,
            camera,
            group_px,
            stored as f32,
            0.0,
            shape.width as f32,
            (picture.newest % shape.width as u64) as f32,
            lanes as f32,
            picture.margin,
            gain,
            if picture.decibels { 1.0 } else { 0.0 },
            mode,
            if self.srgb { 1.0 } else { 0.0 },
        ]);
        for band in picture.bands {
            uniform.extend_from_slice(&colour(band));
        }
        uniform.extend_from_slice(&[ceiling, 0.0, 0.0, 0.0]);
        gpu.queue
            .write_buffer(&self.uniform, 0, bytemuck::cast_slice(&uniform));
        self.ready = true;
    }

    fn ring_for(&self, gpu: &Gpu, shape: Shape) -> Ring {
        let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("waveform"),
            size: wgpu::Extent3d {
                width: shape.width as u32,
                height: (shape.lanes * ROWS_PER_LANE) as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("waveform"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
            ],
        });
        Ring {
            texture,
            bind_group,
            shape,
            written: None,
        }
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass) {
        let (true, Some(ring)) = (self.ready, &self.ring) else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &ring.bind_group, &[]);
        pass.draw(0..4, 0..1);
    }
}

/// Writes the newest `fresh` groups of each lane at their places in the
/// ring, as up to two runs (the ring wraps).
fn write_groups(gpu: &Gpu, ring: &Ring, lanes: &[Vec<WaveformColumn>], newest: u64, fresh: usize) {
    let width = ring.shape.width as u64;
    let oldest = newest + 1 - fresh as u64;
    let mut start = oldest;
    while start <= newest {
        let x = start % width;
        let run = (newest - start + 1).min(width - x) as usize;
        for (lane, groups) in lanes.iter().enumerate() {
            let mut texels: Vec<[f32; 4]> = vec![[0.0; 4]; run * ROWS_PER_LANE];
            for i in 0..run {
                let number = start + i as u64;
                let back = (newest - number) as usize;
                let c = groups.get(back).copied().unwrap_or_default();
                texels[i] = [c.min, c.max, c.low, c.mid];
                texels[run + i] = [c.high, 0.0, 0.0, 0.0];
            }
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &ring.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: x as u32,
                        y: (lane * ROWS_PER_LANE) as u32,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&texels),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some((run * 16) as u32),
                    rows_per_image: Some(ROWS_PER_LANE as u32),
                },
                wgpu::Extent3d {
                    width: run as u32,
                    height: ROWS_PER_LANE as u32,
                    depth_or_array_layers: 1,
                },
            );
        }
        start += run as u64;
    }
}
