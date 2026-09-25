//! Draws one window: its Meters with the GPU renderers, the notes, and the
//! egui layer on top, all in one render pass.

use dasmeter_core::{Palette, Role, Scene, WindowKey};

use crate::gpu::{Gpu, Text, clear_colour};
use crate::meters::{Area, MeterRenderer};

/// Draws a scene with the GPU: one renderer per Meter, one for the notes, and
/// their shared text, then the menus and panel on top.
pub struct Painter {
    meters: Vec<MeterRenderer>,
    notes: MeterRenderer,
    pub text: Text,
    egui: egui_wgpu::Renderer,
    pub gpu: Gpu,
}

/// The menus and panel of one frame, tessellated.
pub struct UiPaint {
    jobs: Vec<egui::ClippedPrimitive>,
    textures: egui::TexturesDelta,
    pixels_per_point: f32,
}

impl UiPaint {
    pub fn new(ctx: &egui::Context, output: egui::FullOutput) -> UiPaint {
        UiPaint {
            jobs: ctx.tessellate(output.shapes, output.pixels_per_point),
            textures: output.textures_delta,
            pixels_per_point: output.pixels_per_point,
        }
    }
}

impl Painter {
    pub fn new(gpu: Gpu) -> Painter {
        let mut text = Text::new(&gpu);
        Painter {
            meters: Vec::new(),
            notes: MeterRenderer::new(&gpu, &mut text),
            text,
            egui: egui_wgpu::Renderer::new(
                &gpu.device,
                gpu.format,
                egui_wgpu::RendererOptions::default(),
            ),
            gpu,
        }
    }

    /// Draws `window`'s Meters from `scene` (or just the background, before
    /// the first scene or for a window without Meters) into `view`, with the
    /// egui layer over them. The Bar also shows the notes.
    pub fn paint(
        &mut self,
        scene: Option<&Scene>,
        key: Option<WindowKey>,
        scale: f32,
        view: &wgpu::TextureView,
        ui: Option<UiPaint>,
    ) {
        let (width, height) = self.gpu.size();
        let (width, height) = (width as f32, height as f32);
        let default_palette = Palette::dark();
        let palette = scene.map_or(&default_palette, |scene| &scene.palette);
        self.text.update_viewport(&self.gpu);
        let meters = scene
            .zip(key)
            .and_then(|(scene, key)| scene.windows.iter().find(|w| w.key == key))
            .map_or(&[][..], |window| &window.meters[..]);
        while self.meters.len() < meters.len() {
            self.meters
                .push(MeterRenderer::new(&self.gpu, &mut self.text));
        }
        let gap = 6.0 * scale;
        for (renderer, meter) in self.meters.iter_mut().zip(meters) {
            let frame = meter.frame;
            let area = Area {
                x: frame.x * width,
                y: frame.y * height,
                width: frame.width * width,
                height: frame.height * height,
            };
            // Half a gap around the window and around each Meter makes a
            // full gap between Meters and at the window's edges.
            let inner = |start: f32, length: f32, whole: f32| {
                let start = gap / 2.0 + start * (whole - gap) / whole;
                let length = length * (whole - gap) / whole;
                (start + gap / 2.0, (length - gap).max(1.0))
            };
            let (x, width) = inner(area.x, area.width, width);
            let (y, height) = inner(area.y, area.height, height);
            let area = Area {
                x,
                y,
                width,
                height,
            };
            renderer.prepare(&self.gpu, &mut self.text, area, scale, palette, meter);
        }
        let window = Area {
            x: 0.0,
            y: 0.0,
            width,
            height,
        };
        let notes = scene
            .filter(|_| key == Some(WindowKey::Bar))
            .map_or(&[][..], |scene| &scene.notes[..]);
        self.notes
            .prepare_notes(&self.gpu, &mut self.text, window, scale, palette, notes);

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [width as u32, height as u32],
            pixels_per_point: ui.as_ref().map_or(scale, |ui| ui.pixels_per_point),
        };
        let mut ui_buffers = Vec::new();
        if let Some(ui) = &ui {
            for (id, deltas) in &ui.textures.set {
                for delta in deltas {
                    self.egui
                        .update_texture(&self.gpu.device, &self.gpu.queue, *id, delta);
                }
            }
            ui_buffers = self.egui.update_buffers(
                &self.gpu.device,
                &self.gpu.queue,
                &mut encoder,
                &ui.jobs,
                &screen,
            );
        }
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear_colour(
                            palette[Role::Background],
                            self.gpu.format,
                        )),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            for renderer in self.meters.iter().take(meters.len()) {
                renderer.render(&self.text, &mut pass);
            }
            self.notes.render(&self.text, &mut pass);
            if let Some(ui) = &ui {
                self.egui
                    .render(&mut pass.forget_lifetime(), &ui.jobs, &screen);
            }
        }
        self.gpu
            .queue
            .submit(ui_buffers.into_iter().chain(Some(encoder.finish())));
        if let Some(mut ui) = ui {
            for id in &ui.textures.free {
                self.egui.free_texture(id);
            }
            ui.textures.clear();
        }
        self.text.atlas.trim();
    }
}
