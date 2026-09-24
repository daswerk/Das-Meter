//! The Das-Meter app: turns the app core's scene into windows and Meters.
//!
//! The shell is thin: it feeds System Capture audio and window events into the
//! app core, draws the scene when the core says so, and otherwise sleeps.

mod capture;
mod gpu;
mod meters;
mod snapshot;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use dasmeter_core::{AppCore, Decision, Event, MeterState, MeterView, Palette, Role, Scene};
use rtrb::Consumer;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use capture::{CaptureMessage, SystemCapture, Waker};
use gpu::{Gpu, Text, WindowSurface, clear_colour};
use meters::{Area, MeterRenderer};

fn version_line() -> String {
    format!("Das-Meter {}", env!("CARGO_PKG_VERSION"))
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--version") => println!("{}", version_line()),
        // Hidden: System Capture into the app core, readings printed once a second.
        Some("--print-readings") => print_readings(),
        // Hidden: a generated signal through the core, drawn offscreen to a PPM image.
        Some("--render-snapshot") => {
            let path = std::env::args().nth(2).unwrap_or("snapshot.ppm".into());
            if let Err(error) = snapshot::render(&path) {
                eprintln!("snapshot: {error}");
                std::process::exit(1);
            }
        }
        _ => run(),
    }
}

/// System Capture feeding the app core: the active Source's ring and the capture thread's messages.
struct Audio {
    messages: mpsc::Receiver<CaptureMessage>,
    ring: Option<Consumer<f32>>,
    waker: Arc<Waker>,
    settled: Arc<AtomicBool>,
    _capture: SystemCapture,
}

impl Audio {
    fn start(wake: impl Fn() + Send + Sync + 'static) -> Audio {
        let waker = Waker::new(wake);
        let settled = Arc::new(AtomicBool::new(false));
        let (capture, messages) = SystemCapture::start(waker.clone(), settled.clone());
        Audio {
            messages,
            ring: None,
            waker,
            settled,
            _capture: capture,
        }
    }

    /// Feeds everything captured so far into the core, in order.
    fn pump(&mut self, core: &mut AppCore, now: Duration) {
        self.waker.clear();
        loop {
            if let Some(ring) = &mut self.ring {
                let available = ring.slots();
                if let Ok(chunk) = ring.read_chunk(available) {
                    let (first, second) = chunk.as_slices();
                    core.handle(Event::Audio(first), now);
                    core.handle(Event::Audio(second), now);
                    chunk.commit_all();
                }
            }
            // The previous ring is complete once its successor is announced.
            match self.messages.try_recv() {
                Ok(CaptureMessage::Started { sample_rate, audio }) => {
                    core.handle(Event::CaptureStarted { sample_rate }, now);
                    self.ring = Some(audio);
                }
                Ok(CaptureMessage::Failed(reason)) => {
                    core.handle(Event::CaptureFailed(&reason), now);
                    self.ring = None;
                }
                Err(_) => break,
            }
        }
    }
}

fn run() {
    let event_loop = EventLoop::<()>::with_user_event()
        .build()
        .expect("create the event loop");
    let proxy = event_loop.create_proxy();
    let mut shell = Shell {
        start: Instant::now(),
        core: AppCore::new(),
        audio: Audio::start(move || {
            let _ = proxy.send_event(());
        }),
        window: None,
    };
    event_loop.run_app(&mut shell).expect("run the event loop");
}

struct Shell {
    start: Instant,
    core: AppCore,
    audio: Audio,
    window: Option<AppWindow>,
}

struct AppWindow {
    painter: Painter,
    surface: WindowSurface,
    // Dropped last: the surface must go before the window.
    window: Arc<Window>,
}

/// Draws a scene with the GPU: one renderer per Meter, one for the notes, and their shared text.
struct Painter {
    meters: Vec<MeterRenderer>,
    notes: MeterRenderer,
    text: Text,
    gpu: Gpu,
}

impl Painter {
    fn new(gpu: Gpu) -> Painter {
        let mut text = Text::new(&gpu);
        Painter {
            meters: Vec::new(),
            notes: MeterRenderer::new(&gpu, &mut text),
            text,
            gpu,
        }
    }

    /// Draws `scene` (or just the background, before the first) into `view`.
    fn paint(&mut self, scene: Option<&Scene>, scale: f32, view: &wgpu::TextureView) {
        let (width, height) = self.gpu.size();
        let (width, height) = (width as f32, height as f32);
        let default_palette = Palette::dark();
        let palette = scene.map_or(&default_palette, |scene| &scene.palette);
        self.text.update_viewport(&self.gpu);
        let meters = scene
            .and_then(|scene| scene.windows.first())
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
            // Half a gap on each side of every Meter makes a full gap between them.
            let area = Area {
                x: area.x + gap / 2.0,
                y: area.y + gap,
                width: (area.width - gap).max(1.0),
                height: (area.height - 2.0 * gap).max(1.0),
            };
            renderer.prepare(&self.gpu, &mut self.text, area, scale, palette, &meter.state);
        }
        let window = Area {
            x: 0.0,
            y: 0.0,
            width,
            height,
        };
        let notes = scene.map_or(&[][..], |scene| &scene.notes[..]);
        self.notes
            .prepare_notes(&self.gpu, &mut self.text, window, scale, palette, notes);

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
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
        }
        self.gpu.queue.submit(Some(encoder.finish()));
        self.text.atlas.trim();
    }
}

impl Shell {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    fn draw(&mut self) {
        let Some(app) = &mut self.window else { return };
        let Some(frame) = app.surface.frame(&app.painter.gpu) else {
            app.window.request_redraw();
            return;
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        app.painter
            .paint(self.core.scene(), app.window.scale_factor() as f32, &view);
        app.window.pre_present_notify();
        app.painter.gpu.queue.present(frame);
    }
}

impl ApplicationHandler for Shell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Das-Meter")
            .with_inner_size(LogicalSize::new(1200.0, 340.0))
            .with_min_inner_size(LogicalSize::new(600.0, 240.0));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("create the window"),
        );
        let (gpu, surface) = match pollster::block_on(Gpu::for_window(window.clone())) {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("Das-Meter needs a GPU it can draw with: {error}");
                event_loop.exit();
                return;
            }
        };
        self.window = Some(AppWindow {
            painter: Painter::new(gpu),
            surface,
            window,
        });
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _wake: ()) {
        // Audio arrived; about_to_wait feeds it in and decides.
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(app) = &mut self.window {
                    app.surface
                        .resize(&mut app.painter.gpu, size.width, size.height);
                    app.window.request_redraw();
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(app) = &self.window {
                    app.window.request_redraw();
                }
            }
            WindowEvent::Occluded(occluded) => {
                let now = self.now();
                self.core.handle(Event::Visible(!occluded), now);
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(app) = &self.window {
                    let size = app.window.inner_size();
                    let point = [
                        position.x as f32 / size.width.max(1) as f32,
                        position.y as f32 / size.height.max(1) as f32,
                    ];
                    let now = self.now();
                    self.core.handle(Event::Pointer(Some(point)), now);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                let now = self.now();
                self.core.handle(Event::Pointer(None), now);
            }
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = self.now();
        self.audio.pump(&mut self.core, now);
        let decision = self.core.decide(now);
        self.audio.settled.store(
            decision == Decision::Sleep { until: None },
            Ordering::Release,
        );
        match decision {
            Decision::Draw => {
                if let Some(app) = &self.window {
                    app.window.request_redraw();
                }
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            Decision::Sleep { until: Some(until) } => {
                event_loop.set_control_flow(ControlFlow::WaitUntil(self.start + until));
            }
            Decision::Sleep { until: None } => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

/// Runs System Capture into the app core without a window and prints the
/// Loudness Meter's readings once a second, for checks against a reference.
fn print_readings() {
    let start = Instant::now();
    let mut core = AppCore::new();
    let mut audio = Audio::start(|| {});
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let now = start.elapsed();
        audio.pump(&mut core, now);
        if core.decide(now) != Decision::Draw {
            continue;
        }
        let Some(scene) = core.scene() else { continue };
        let notes: Vec<_> = scene.notes.iter().map(|note| note.text()).collect();
        let loudness = scene.windows[0].meters.iter().find_map(|meter| match &meter.state {
            MeterState::Live(MeterView::Loudness { display, .. }) => Some(Ok(*display)),
            MeterState::Live(_) => None,
            other => Some(Err(other.clone())),
        });
        match loudness {
            Some(Ok(d)) => {
                let show = |level: dasmeter_core::Level| {
                    level
                        .db()
                        .map_or_else(|| "-inf".to_owned(), |db| format!("{db:.1}"))
                };
                println!(
                    "{:>6.1}s {} Hz  M {:>6}  S {:>6}  I {:>6} LUFS  LRA {:>5}  TP {:>6}  peak L {:>6} R {:>6}  RMS L {:>6} R {:>6}  {}",
                    now.as_secs_f64(),
                    d.sample_rate,
                    show(d.momentary),
                    show(d.short_term),
                    show(d.integrated),
                    show(d.range),
                    show(d.true_peak_max),
                    show(d.left.peak),
                    show(d.right.peak),
                    show(d.left.rms),
                    show(d.right.rms),
                    notes.join(", "),
                );
            }
            other => println!("{:>6.1}s {other:?} {}", now.as_secs_f64(), notes.join(", ")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_line_names_the_app() {
        assert!(version_line().starts_with("Das-Meter "));
    }
}
