//! Hidden `--render-snapshot PATH [menu|settings|bar|window|light|contrast|glass|WxH]`
//! (a PATH ending in .pam keeps the alpha channel): runs the app core on a
//! generated signal and draws its scene offscreen into a PPM image, optionally
//! with the Loudness Meter's menu or the settings panel open, or at the
//! size of a Bar on a laptop display. It checks the
//! renderers and the egui layer without a window, an audio device or
//! screen-recording permission.

use std::time::Duration;

use dasmeter_analysis::signals::{frames, pink_noise, stereo};
use dasmeter_core::{AppCore, Decision, Event, LayoutMode, Styling, WindowKey};

use crate::gpu::Gpu;
use crate::painter::{Painter, UiPaint};
use crate::ui::{Surface, Ui};

const RATE: u32 = 48_000;
const SCALE: f32 = 2.0;
const WIDTH: u32 = 2400;
const HEIGHT: u32 = 680;
/// A 180 px Bar across a 1512 px display, at 2×.
const BAR: (u32, u32) = (3024, 360);

pub fn render(path: &str, open: Option<&str>) -> Result<(), String> {
    // Six seconds of pink noise with an output change after four, so the
    // snapshot shows levels, loudness and the "Output changed" note.
    let mut core = AppCore::new();
    let noise = |seconds, seed| {
        let n = frames(RATE, seconds);
        stereo(&pink_noise(n, -12.0, seed), &pink_noise(n, -15.0, seed + 1))
    };
    let mut now = Duration::ZERO;
    core.handle(Event::CaptureStarted { sample_rate: RATE }, now);
    // The silence hint only shows when nothing has played.
    let noise_for = if open == Some("hint") { 0 } else { 2 };
    for (seconds, seed) in [(4.0, 1), (2.0, 3)].into_iter().take(noise_for) {
        for block in noise(seconds, seed).chunks(1024) {
            now += Duration::from_secs_f64(512.0 / f64::from(RATE));
            core.handle(Event::Audio(block), now);
        }
        if seed == 1 {
            core.handle(Event::CaptureStarted { sample_rate: RATE }, now);
        }
    }
    // The cursor over the Spectrum, for its readout.
    core.handle(Event::Pointer(Some((WindowKey::Bar, [0.4, 0.5]))), now);
    if core.decide(now) != Decision::Draw {
        return Err("the core had nothing to draw".into());
    }
    // "WxH": a window of that many logical px, for checking small sizes.
    let custom = open.and_then(|o| {
        let (w, h) = o.split_once('x')?;
        Some((w.parse::<u32>().ok()? * 2, h.parse::<u32>().ok()? * 2))
    });
    let (width, height) = match (open, custom) {
        (_, Some(size)) => size,
        (Some("bar"), _) => BAR,
        (Some("window"), _) => (2400, 1440),
        _ => (WIDTH, HEIGHT),
    };
    match open {
        None | Some("bar") => {}
        Some("window") => {
            core.handle(Event::SetMode(LayoutMode::Window), now);
            core.handle(Event::Pointer(Some((WindowKey::Main, [0.4, 0.25]))), now);
        }
        Some(_) if custom.is_some() => {}
        // Themes: the built-ins, and a see-through copy of Dark.
        Some("light") => core.handle(Event::ChooseTheme { light: 1, dark: 1 }, now),
        Some("contrast") => core.handle(Event::ChooseTheme { light: 2, dark: 2 }, now),
        Some("glass") => {
            core.handle(Event::DuplicateTheme { theme: 0 }, now);
            let styling = Styling {
                background_opacity: 0.55,
                corner_radius: 10.0,
                gap: 8.0,
                ..Styling::default()
            };
            core.handle(Event::SetThemeStyling { theme: 3, styling }, now);
            core.take_writes();
        }
        // Right-click the Loudness Meter, as a user would.
        Some("menu") => core.handle(
            Event::OpenMenu {
                window: WindowKey::Bar,
                at: [0.8, 0.1],
            },
            now,
        ),
        Some("settings") => core.handle(Event::ShowSettings(true), now),
        // The first-launch card, and the silence hint after 11 s of nothing.
        Some("welcome") => core.handle(Event::ShowWelcome, now),
        Some("hint") => {
            core.handle(Event::StartListening, now);
            let silence = vec![0.0f32; 2 * 512];
            for _ in 0..(11 * RATE / 512) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(&silence), now);
            }
        }
        Some(other) => return Err(format!("unknown option {other:?}")),
    }
    now += Duration::from_millis(100);
    core.decide(now);

    let gpu = pollster::block_on(Gpu::offscreen(width, height))?;
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("snapshot"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gpu.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut painter = Painter::new(gpu);

    // egui lays windows out over a few frames; the last one is drawn.
    let mut ui = Ui::new();
    ui.use_fonts(painter.text.font_system.db());
    let scene = core.scene().ok_or("no scene")?;
    let mut output = None;
    for frame in 0..4 {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width as f32 / SCALE, height as f32 / SCALE),
            )),
            time: Some(f64::from(frame) * 0.5),
            ..Default::default()
        };
        input
            .viewports
            .entry(egui::ViewportId::ROOT)
            .or_default()
            .native_pixels_per_point = Some(SCALE);
        let (mut out, _) = ui.run(input, scene, Surface::Overlay);
        // Textures made in earlier frames must reach the painter too.
        if let Some(previous) = output.take() {
            let previous: egui::FullOutput = previous;
            let mut textures = previous.textures_delta;
            textures.append(out.textures_delta);
            out.textures_delta = textures;
        }
        output = Some(out);
    }
    let ui = output.map(|output| UiPaint::new(&ui.ctx, output));

    painter.paint(
        core.scene(),
        core.scene()
            .and_then(|scene| scene.windows.first())
            .map(|w| w.key),
        SCALE,
        &texture.create_view(&wgpu::TextureViewDescriptor::default()),
        ui,
    );

    let gpu = &painter.gpu;
    let row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("snapshot readback"),
        size: u64::from(row * height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: None,
            },
        },
        texture.size(),
    );
    gpu.queue.submit(Some(encoder.finish()));
    buffer.map_async(wgpu::MapMode::Read, .., |result| {
        result.expect("map the snapshot");
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| e.to_string())?;
    let pixels = buffer.get_mapped_range(..).map_err(|e| e.to_string())?;

    // A .pam file keeps the alpha channel (for see-through Themes); else PPM.
    let alpha = path.ends_with(".pam");
    let mut ppm = if alpha {
        format!(
            "P7\nWIDTH {width}\nHEIGHT {height}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n"
        )
        .into_bytes()
    } else {
        format!("P6\n{width} {height}\n255\n").into_bytes()
    };
    for y in 0..height as usize {
        let start = y * row as usize;
        let (line, _) = pixels[start..start + width as usize * 4].as_chunks::<4>();
        for pixel in line {
            ppm.extend_from_slice(if alpha { &pixel[..] } else { &pixel[..3] });
        }
    }
    std::fs::write(path, ppm).map_err(|e| e.to_string())
}
