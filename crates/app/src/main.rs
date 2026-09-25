//! The Das-Meter app: turns the app core's scene into windows and Meters.
//!
//! The shell is thin: it feeds System Capture or Send Plugin audio and window
//! events into the app core, draws the scene when the core says so, and
//! otherwise sleeps.

mod capture;
mod gpu;
#[cfg(target_os = "macos")]
mod main_menu;
mod meters;
mod send_plugins;
mod snapshot;
mod ui;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use dasmeter_core::{
    AppCore, Decision, Event, ListenTo, MeterState, MeterView, Palette, Role, Scene,
};
use rtrb::Consumer;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use capture::{CaptureMessage, SystemCapture, Waker};
use gpu::{Gpu, Text, WindowSurface, clear_colour};
use meters::{Area, MeterRenderer};
use send_plugins::{LIST_EVERY, SendPluginInput};
use ui::Ui;

fn version_line() -> String {
    format!("Das-Meter {}", env!("CARGO_PKG_VERSION"))
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--version") => println!("{}", version_line()),
        // Hidden: System Capture into the app core, readings printed once a second.
        Some("--print-readings") => print_readings(),
        // Hidden: two Loudness Meters on the first two Send Plugins, printed once a second.
        Some("--print-send-plugins") => print_send_plugins(),
        // Hidden: a generated signal through the core, drawn offscreen to a PPM
        // image, optionally with a Meter menu or the settings panel open.
        Some("--render-snapshot") => {
            let path = std::env::args().nth(2).unwrap_or("snapshot.ppm".into());
            let open = std::env::args().nth(3);
            if let Err(error) = snapshot::render(&path, open.as_deref()) {
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
        audio: None,
        wake: Arc::new(move || {
            let _ = proxy.send_event(());
        }),
        send_plugins: SendPluginInput::new(),
        pointer: None,
        window: None,
        ui: Ui::new(),
        ui_wake: None,
        #[cfg(target_os = "macos")]
        main_menu: None,
    };
    event_loop.run_app(&mut shell).expect("run the event loop");
}

struct Shell {
    start: Instant,
    core: AppCore,
    /// System Capture, running only while Listen to is System Capture.
    audio: Option<Audio>,
    wake: Arc<dyn Fn() + Send + Sync>,
    send_plugins: SendPluginInput,
    /// The pointer's last position in the window (fractions), for clicks.
    pointer: Option<[f32; 2]>,
    window: Option<AppWindow>,
    /// The Meter menus and the settings panel.
    ui: Ui,
    /// When egui asked to be drawn again (an animation, a tooltip), if it did.
    ui_wake: Option<Duration>,
    #[cfg(target_os = "macos")]
    main_menu: Option<main_menu::MainMenu>,
}

struct AppWindow {
    egui: egui_winit::State,
    painter: Painter,
    surface: WindowSurface,
    // Dropped last: the surface must go before the window.
    window: Arc<Window>,
}

/// Draws a scene with the GPU: one renderer per Meter, one for the notes, and
/// their shared text, then the menus and panel on top.
struct Painter {
    meters: Vec<MeterRenderer>,
    notes: MeterRenderer,
    text: Text,
    egui: egui_wgpu::Renderer,
    gpu: Gpu,
}

/// The menus and panel of one frame, tessellated.
struct UiPaint {
    jobs: Vec<egui::ClippedPrimitive>,
    textures: egui::TexturesDelta,
    pixels_per_point: f32,
}

impl UiPaint {
    fn new(ctx: &egui::Context, output: egui::FullOutput) -> UiPaint {
        UiPaint {
            jobs: ctx.tessellate(output.shapes, output.pixels_per_point),
            textures: output.textures_delta,
            pixels_per_point: output.pixels_per_point,
        }
    }
}

impl Painter {
    fn new(gpu: Gpu) -> Painter {
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

    /// Draws `scene` (or just the background, before the first) into `view`,
    /// with the menus and panel over it.
    fn paint(
        &mut self,
        scene: Option<&Scene>,
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
            renderer.prepare(&self.gpu, &mut self.text, area, scale, palette, meter);
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

impl Shell {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }

    /// Feeds the active Source's audio into the core, starting System Capture
    /// or pausing it to follow Listen to. Returns when to pump again, if a
    /// timer is needed (Send Plugins can't wake the app).
    fn pump(&mut self, now: Duration) -> Option<Duration> {
        match self.core.listen_to() {
            ListenTo::SystemCapture => {
                self.send_plugins.stop();
                let wake = self.wake.clone();
                let audio = self
                    .audio
                    .get_or_insert_with(|| Audio::start(move || wake()));
                audio.pump(&mut self.core, now);
                None
            }
            ListenTo::SendPlugins => {
                // System Capture pauses while listening to Send Plugins.
                self.audio = None;
                self.send_plugins
                    .pump(&mut self.core, now, self.start + now);
                let every = if self.send_plugins.listening() {
                    self.core.app_settings().frame_interval()
                } else {
                    LIST_EVERY
                };
                Some(now + every)
            }
        }
    }

    fn draw(&mut self) {
        let now = self.now();
        let Some(app) = &mut self.window else { return };
        let Some(frame) = app.surface.frame(&app.painter.gpu) else {
            app.window.request_redraw();
            return;
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut actions = Vec::new();
        self.ui_wake = None;
        let ui = self.core.scene().map(|scene| {
            let input = app.egui.take_egui_input(&app.window);
            let (mut output, done) = self.ui.run(input, scene);
            actions = done;
            let platform = std::mem::take(&mut output.platform_output);
            app.egui.handle_platform_output(&app.window, platform);
            let delay = output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .map(|viewport| viewport.repaint_delay);
            // egui asks for a repaint after a delay (an animation, a tooltip); a
            // very long one means it has nothing to show.
            if let Some(delay) = delay.filter(|d| *d < Duration::from_secs(60)) {
                self.ui_wake = Some(now + delay);
            }
            UiPaint::new(&self.ui.ctx, output)
        });
        app.painter.paint(
            self.core.scene(),
            app.window.scale_factor() as f32,
            &view,
            ui,
        );
        app.window.pre_present_notify();
        app.painter.gpu.queue.present(frame);
        for action in actions {
            self.core.handle(action, now);
        }
    }

    /// The refresh rate of the display the window is on, for the frame-rate cap.
    fn report_refresh_rate(&mut self) {
        let Some(app) = &self.window else { return };
        let rate = app
            .window
            .current_monitor()
            .and_then(|monitor| monitor.refresh_rate_millihertz());
        if let Some(millihertz) = rate {
            let now = self.now();
            self.core
                .handle(Event::DisplayRefreshRate((millihertz + 500) / 1000), now);
        }
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
        let egui = egui_winit::State::new(
            self.ui.ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(gpu.device.limits().max_texture_dimension_2d as usize),
        );
        let painter = Painter::new(gpu);
        self.ui.use_fonts(painter.text.font_system.db());
        self.window = Some(AppWindow {
            egui,
            painter,
            surface,
            window,
        });
        self.report_refresh_rate();
        #[cfg(target_os = "macos")]
        if self.main_menu.is_none() {
            let wake = self.wake.clone();
            self.main_menu = Some(main_menu::MainMenu::install(move || wake()));
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _wake: ()) {
        // Audio arrived; about_to_wait feeds it in and decides.
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // The menus and panel see every event first; a click on them stays theirs.
        let mut on_ui = false;
        let ui_shown = self
            .core
            .scene()
            .is_some_and(|scene| scene.menu.is_some() || scene.settings_open);
        if let Some(app) = &mut self.window {
            let response = app.egui.on_window_event(&app.window, &event);
            // egui asks for a repaint after nearly every event, a redraw
            // included; only an open menu or panel needs one.
            let redraw = matches!(event, WindowEvent::RedrawRequested);
            if response.repaint && ui_shown && !redraw {
                app.window.request_redraw();
            }
            on_ui = response.consumed;
        }
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
                self.report_refresh_rate();
            }
            WindowEvent::Moved(_) => self.report_refresh_rate(),
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
                    self.pointer = Some(point);
                    let now = self.now();
                    self.core.handle(Event::Pointer(Some(point)), now);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer = None;
                let now = self.now();
                self.core.handle(Event::Pointer(None), now);
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button,
                ..
            } if !on_ui => {
                let Some(point) = self.pointer else { return };
                let now = self.now();
                match button {
                    MouseButton::Left => self.core.handle(Event::Click(point), now),
                    MouseButton::Right => self.core.handle(Event::OpenMenu(point), now),
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => self.draw(),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = self.now();
        #[cfg(target_os = "macos")]
        if let Some(menu) = &mut self.main_menu {
            let commands = menu.commands();
            let clicked = !commands.is_empty();
            for command in commands {
                match command {
                    main_menu::Command::Core(event) => self.core.handle(event, now),
                    main_menu::Command::Open(url) => {
                        self.ui.ctx.open_url(egui::OpenUrl::new_tab(url));
                        if let Some(app) = &self.window {
                            app.window.request_redraw();
                        }
                    }
                }
            }
            menu.show(self.core.listen_to(), clicked);
        }
        let pump_at = self.pump(now);
        let decision = self.core.decide(now);
        if let Some(audio) = &self.audio {
            audio.settled.store(
                decision == Decision::Sleep { until: None },
                Ordering::Release,
            );
        }
        let wake_at = match decision {
            Decision::Draw => {
                if let Some(app) = &self.window {
                    app.window.request_redraw();
                }
                None
            }
            Decision::Sleep { until } => until,
        };
        if self.ui_wake.is_some_and(|at| at <= now) {
            self.ui_wake = None;
            if let Some(app) = &self.window {
                app.window.request_redraw();
            }
        }
        let wake_at = [wake_at, pump_at, self.ui_wake].into_iter().flatten().min();
        match wake_at {
            Some(at) => event_loop.set_control_flow(ControlFlow::WaitUntil(self.start + at)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
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
        let loudness = scene.windows[0]
            .meters
            .iter()
            .find_map(|meter| match &meter.state {
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

/// Listens to Send Plugins without a window: two Loudness Meters, the first
/// picking the first Send Plugin listed (by name) and the second the second,
/// printed once a second. For checking two DAW tracks against each other.
fn print_send_plugins() {
    use dasmeter_core::{LoudnessMeterSettings, MeterSettings, SendPlugin};

    let start = Instant::now();
    let mut core = AppCore::with_meters(vec![
        MeterSettings::Loudness(LoudnessMeterSettings::default()),
        MeterSettings::Loudness(LoudnessMeterSettings::default()),
    ]);
    core.handle(Event::SetListenTo(ListenTo::SendPlugins), Duration::ZERO);
    for meter in 0..2 {
        core.handle(
            Event::ShowSourceLabel { meter, shown: true },
            Duration::ZERO,
        );
    }
    let mut input = SendPluginInput::new();
    let mut picked = false;
    let mut printed = Duration::ZERO;
    loop {
        std::thread::sleep(dasmeter_core::DEFAULT_FRAME_INTERVAL);
        let now = start.elapsed();
        input.pump(&mut core, now, start + now);
        if !picked {
            let mut listed: Vec<SendPlugin> = input.listed();
            listed.retain(|p| p.state != dasmeter_core::SendPluginState::Gone);
            listed.sort_by(|a, b| a.name.cmp(&b.name));
            if listed.len() >= 2 {
                for (meter, plugin) in listed.iter().take(2).enumerate() {
                    core.handle(
                        Event::PickSendPlugin {
                            meter,
                            id: plugin.id,
                        },
                        now,
                    );
                }
                picked = true;
            }
        }
        core.decide(now);
        if now < printed + Duration::from_secs(1) {
            continue;
        }
        printed = now;
        let Some(scene) = core.scene() else { continue };
        let line: Vec<String> = scene.windows[0]
            .meters
            .iter()
            .map(|meter| {
                let source = meter.source.as_ref().map_or("-", |s| s.name.as_str());
                match &meter.state {
                    MeterState::Live(MeterView::Loudness { display, .. }) => format!(
                        "{source}: M {} LUFS, peak {} dBFS, {} Hz",
                        display
                            .momentary
                            .db()
                            .map_or("-inf".into(), |db| format!("{db:.1}")),
                        display
                            .left
                            .peak
                            .db()
                            .map_or("-inf".into(), |db| format!("{db:.1}")),
                        display.sample_rate
                    ),
                    other => format!("{source}: {other:?}"),
                }
            })
            .collect();
        println!("{:>6.1}s  {}", now.as_secs_f64(), line.join("   |   "));
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
