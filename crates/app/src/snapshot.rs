//! Hidden `--render-snapshot PATH`: runs the app core on a generated signal and
//! draws its scene offscreen into a PPM image. It checks the renderers without
//! a window, an audio device or screen-recording permission.

use std::time::Duration;

use dasmeter_analysis::signals::{frames, pink_noise, stereo};
use dasmeter_core::{AppCore, Decision, Event};

use crate::Painter;
use crate::gpu::Gpu;

const RATE: u32 = 48_000;
const SCALE: f32 = 2.0;
const WIDTH: u32 = 2400;
const HEIGHT: u32 = 680;

pub fn render(path: &str) -> Result<(), String> {
    // Six seconds of pink noise with an output change after four, so the
    // snapshot shows levels, loudness and the "Output changed" note.
    let mut core = AppCore::new();
    let noise = |seconds, seed| {
        let n = frames(RATE, seconds);
        stereo(&pink_noise(n, -12.0, seed), &pink_noise(n, -15.0, seed + 1))
    };
    let mut now = Duration::ZERO;
    core.handle(Event::CaptureStarted { sample_rate: RATE }, now);
    for (seconds, seed) in [(4.0, 1), (2.0, 3)] {
        for block in noise(seconds, seed).chunks(1024) {
            now += Duration::from_secs_f64(512.0 / f64::from(RATE));
            core.handle(Event::Audio(block), now);
        }
        if seed == 1 {
            core.handle(Event::CaptureStarted { sample_rate: RATE }, now);
        }
    }
    // The cursor over the Spectrum, for its readout.
    core.handle(Event::Pointer(Some([0.4, 0.5])), now);
    if core.decide(now) != Decision::Draw {
        return Err("the core had nothing to draw".into());
    }

    let gpu = pollster::block_on(Gpu::offscreen(WIDTH, HEIGHT))?;
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("snapshot"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
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
    painter.paint(
        core.scene(),
        SCALE,
        &texture.create_view(&wgpu::TextureViewDescriptor::default()),
    );

    let gpu = &painter.gpu;
    let row = (WIDTH * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("snapshot readback"),
        size: u64::from(row * HEIGHT),
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

    let mut ppm = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
    for y in 0..HEIGHT as usize {
        let start = y * row as usize;
        for pixel in pixels[start..start + WIDTH as usize * 4].chunks_exact(4) {
            ppm.extend_from_slice(&pixel[..3]);
        }
    }
    std::fs::write(path, ppm).map_err(|e| e.to_string())
}
