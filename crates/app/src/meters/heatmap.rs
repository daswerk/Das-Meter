//! A heat map drawn on the GPU (the Spectrogram's picture): its levels go up
//! as a texture, so it's smooth at any size, and a palette colours them.
//! Drawn after a Meter's shapes and before its overlay and labels.

use dasmeter_core::Colour;

use super::shapes::Area;
use crate::gpu::{Gpu, linear};

/// Bytes in the uniform block: three `vec4`s, then 256 palette colours.
const UNIFORM_SIZE: u64 = (3 + 256) * 16;

/// One frame's picture, staged until [`Heatmap::prepare`] uploads it.
struct Staged {
    area: Area,
    /// Columns stored, oldest first, each `rows` levels.
    levels: Vec<u8>,
    stored: usize,
    capacity: usize,
    rows: usize,
    lag: f32,
    palette: [Colour; 256],
}

pub struct Heatmap {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    /// The texture, its bind group and its size (rows × capacity).
    texture: Option<(wgpu::Texture, wgpu::BindGroup, (usize, usize))>,
    staged: Option<Staged>,
    /// What's uploaded: the levels (to skip uploading them again unchanged).
    uploaded: Vec<u8>,
    ready: bool,
    format: wgpu::TextureFormat,
}

impl Heatmap {
    pub fn new(gpu: &Gpu) -> Heatmap {
        let shader = gpu
            .device
            .create_shader_module(wgpu::include_wgsl!("heatmap.wgsl"));
        let layout = gpu
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("heatmap"),
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
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("heatmap"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            });
        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("heatmap"),
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
        let sampler = gpu.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("heatmap"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("heatmap"),
            size: UNIFORM_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Heatmap {
            pipeline,
            layout,
            sampler,
            uniform,
            texture: None,
            staged: None,
            uploaded: Vec::new(),
            ready: false,
            format: gpu.format,
        }
    }

    /// Starts a frame without a picture.
    pub fn begin(&mut self) {
        self.staged = None;
        self.ready = false;
    }

    /// The picture for this frame: `columns` oldest first (at most
    /// `capacity`, each `rows` levels from the bottom up, 0 to 255), the
    /// newest against `area`'s right edge, `lag` columns of it still to
    /// scroll in; each level coloured from `palette`.
    pub fn draw(
        &mut self,
        area: Area,
        columns: &[Vec<u8>],
        capacity: usize,
        rows: usize,
        lag: f32,
        palette: [Colour; 256],
    ) {
        let stored = columns.len().min(capacity);
        let mut levels = Vec::with_capacity(stored * rows);
        for column in &columns[columns.len() - stored..] {
            levels.extend((0..rows).map(|row| column.get(row).copied().unwrap_or(0)));
        }
        self.staged = Some(Staged {
            area,
            levels,
            stored,
            capacity,
            rows,
            lag,
            palette,
        });
    }

    /// Uploads the staged picture.
    pub fn prepare(&mut self, gpu: &Gpu) {
        let Some(staged) = self.staged.take() else {
            return;
        };
        if staged.stored == 0 || staged.rows == 0 || staged.area.width < 1.0 {
            return;
        }
        let size = (staged.rows, staged.capacity);
        if self.texture.as_ref().is_none_or(|(_, _, s)| *s != size) {
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("heatmap"),
                size: wgpu::Extent3d {
                    width: size.0 as u32,
                    height: size.1 as u32,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("heatmap"),
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
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            self.texture = Some((texture, bind_group, size));
            self.uploaded.clear();
        }
        let Some((texture, _, _)) = &self.texture else {
            return;
        };
        if self.uploaded != staged.levels {
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &staged.levels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(staged.rows as u32),
                    rows_per_image: Some(staged.stored as u32),
                },
                wgpu::Extent3d {
                    width: staged.rows as u32,
                    height: staged.stored as u32,
                    depth_or_array_layers: 1,
                },
            );
            self.uploaded = staged.levels;
        }

        let (width, height) = gpu.size();
        let (width, height) = (width as f32, height as f32);
        let area = staged.area;
        let mut uniform: Vec<f32> = Vec::with_capacity((UNIFORM_SIZE / 4) as usize);
        uniform.extend_from_slice(&[
            area.x / width * 2.0 - 1.0,
            1.0 - area.y / height * 2.0,
            area.right() / width * 2.0 - 1.0,
            1.0 - area.bottom() / height * 2.0,
            staged.stored as f32,
            staged.capacity as f32,
            staged.lag,
            staged.rows as f32,
            staged.capacity as f32 / area.width.max(1.0),
            staged.rows as f32 / area.height.max(1.0),
            0.0,
            0.0,
        ]);
        for colour in staged.palette {
            uniform.extend_from_slice(&linear(colour, self.format));
        }
        gpu.queue
            .write_buffer(&self.uniform, 0, bytemuck::cast_slice(&uniform));
        self.ready = true;
    }

    pub fn render(&self, pass: &mut wgpu::RenderPass) {
        let (true, Some((_, bind_group, _))) = (self.ready, &self.texture) else {
            return;
        };
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.draw(0..4, 0..1);
    }
}
