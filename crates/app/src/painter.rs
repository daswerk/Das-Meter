//! Draws one window: its Meters with the GPU renderers, the notes, and the
//! egui layer on top, all in one render pass.

use dasmeter_core::{Palette, Role, Scene, Styling, WindowKey};

use crate::gpu::{Gpu, Text, clear_colour};
use crate::meters::{Area, Look, MeterRenderer};

/// Draws a scene with the GPU: one renderer per Meter, one for the notes, and
/// their shared text, then the menus and panel on top.
pub struct Painter {
    /// The window's background and the Meters' shadows, under them all.
    backdrop: MeterRenderer,
    meters: Vec<MeterRenderer>,
    notes: MeterRenderer,
    pub text: Text,
    egui: egui_wgpu::Renderer,
    /// Staging memory for the shapes, reused frame after frame.
    belt: wgpu::util::StagingBelt,
    pub gpu: Gpu,
}

/// The staging belt's unit of allocation: a frame's shapes for a few Meters.
const BELT_CHUNK: u64 = 1 << 20;

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
            backdrop: MeterRenderer::new(&gpu, &mut text),
            meters: Vec::new(),
            notes: MeterRenderer::new(&gpu, &mut text),
            text,
            egui: egui_wgpu::Renderer::new(
                &gpu.device,
                gpu.format,
                egui_wgpu::RendererOptions::default(),
            ),
            belt: wgpu::util::StagingBelt::new(gpu.device.clone(), BELT_CHUNK),
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
        let styling = scene.map_or_else(Styling::default, |scene| scene.theme.styling);
        let gap = styling.gap * scale;
        let look = Look {
            palette,
            styling,
            scale,
            bar: key == Some(WindowKey::Bar),
        };
        let mut areas = Vec::with_capacity(meters.len());
        for (renderer, meter) in self.meters.iter_mut().zip(meters) {
            let area = meter_area(meter.frame, (width, height), gap);
            renderer.prepare(&self.gpu, &mut self.text, area, look, meter);
            areas.push(area);
        }
        let window = Area {
            x: 0.0,
            y: 0.0,
            width,
            height,
        };
        self.backdrop
            .prepare_backdrop(&self.gpu, &mut self.text, window, look, &areas);
        let notes = scene
            .filter(|_| key == Some(WindowKey::Bar))
            .map_or(&[][..], |scene| &scene.notes[..]);
        self.notes
            .prepare_notes(&self.gpu, &mut self.text, window, look, notes);

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.backdrop.upload(&mut encoder, &mut self.belt);
        for renderer in self.meters.iter_mut().take(meters.len()) {
            renderer.upload(&mut encoder, &mut self.belt);
        }
        self.notes.upload(&mut encoder, &mut self.belt);
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
                            palette[Role::Background].faded(styling.background_opacity),
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
            self.backdrop.render(&self.text, &mut pass);
            for renderer in self.meters.iter().take(meters.len()) {
                renderer.render(&self.text, &mut pass);
            }
            self.notes.render(&self.text, &mut pass);
            if let Some(ui) = &ui {
                self.egui
                    .render(&mut pass.forget_lifetime(), &ui.jobs, &screen);
            }
        }
        self.belt.finish();
        self.gpu
            .queue
            .submit(ui_buffers.into_iter().chain(Some(encoder.finish())));
        // The belt's memory comes back once the GPU is done with it.
        self.belt.recall();
        let _ = self.gpu.device.poll(wgpu::PollType::Poll);
        if let Some(mut ui) = ui {
            for id in &ui.textures.free {
                self.egui.free_texture(id);
            }
            ui.textures.clear();
        }
        self.text.atlas.trim();
    }
}

/// Where a Meter with `frame` (fractions of the window) is drawn in a
/// window `size` wide and high, `gap` apart from the others: half a gap
/// around the window and around each Meter makes a full gap between Meters
/// and at the window's edges. In physical pixels, or logical ones with
/// everything given in logical pixels.
pub fn meter_area(frame: dasmeter_core::Frame, size: (f32, f32), gap: f32) -> Area {
    let inner = |start: f32, length: f32, whole: f32| {
        let start = gap / 2.0 + start * (whole - gap);
        let length = length * (whole - gap);
        (start + gap / 2.0, (length - gap).max(1.0))
    };
    let (x, width) = inner(frame.x, frame.width, size.0);
    let (y, height) = inner(frame.y, frame.height, size.1);
    Area {
        x,
        y,
        width,
        height,
    }
}
