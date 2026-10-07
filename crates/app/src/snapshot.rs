//! Hidden `--render-snapshot PATH [menu|settings|bar|window|zoom|dragging|light|contrast|glass|cepstrum|spectrogram|harmonics|phase-scope|phase-scope-bar|phase-scope-filled|phase-scope-lr|phase-scope-overlay|WxH]`
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

/// A kick on every beat at 120 BPM over a bass line (an octave jump each
/// bar), `seconds` long, mono.
fn kick_and_bass(seconds: f64) -> Vec<f32> {
    let (kick, bass) = kick_and_bass_apart(seconds);
    kick.iter().zip(&bass).map(|(k, b)| k + b).collect()
}

/// [`kick_and_bass`] as two tracks: the kick, and the bass.
fn kick_and_bass_apart(seconds: f64) -> (Vec<f32>, Vec<f32>) {
    let beat = f64::from(RATE) / 2.0;
    let (mut kick_phase, mut bass_phase) = (0.0f64, 0.0f64);
    (0..frames(RATE, seconds))
        .map(|i| {
            let t = (i as f64 % beat) / f64::from(RATE);
            if i as f64 % beat < 1.0 {
                kick_phase = 0.0;
            }
            kick_phase +=
                std::f64::consts::TAU * (45.0 + 110.0 * (-t * 35.0).exp()) / f64::from(RATE);
            let kick = kick_phase.sin() * (-t * 7.0).exp() * 0.7;
            let bar = (i as f64 / (beat * 4.0)).floor() as i64;
            let bass_freq = if bar % 2 == 0 { 55.0 } else { 41.2 };
            bass_phase += std::f64::consts::TAU * bass_freq / f64::from(RATE);
            let bass = bass_phase.sin() * 0.3 * (1.0 - (-t * 20.0).exp());
            (kick as f32, bass as f32)
        })
        .unzip()
}

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
        (Some("window" | "zoom" | "dragging" | "bars-window" | "spectrogram-zoom"), _) => {
            (2400, 1440)
        }
        _ => (WIDTH, HEIGHT),
    };
    match open {
        None | Some("bar") => {}
        Some("window") => {
            core.handle(Event::SetMode(LayoutMode::Window), now);
            core.handle(Event::Pointer(Some((WindowKey::Main, [0.4, 0.25]))), now);
        }
        // A box dragged over the Spectrum (2 to 8 kHz or so), released into
        // the zoom window or still being dragged.
        Some(option @ ("zoom" | "dragging")) => {
            core.handle(Event::SetMode(LayoutMode::Window), now);
            // Past the first-launch "Start listening", which a click would press.
            core.handle(Event::StartListening, now);
            now += Duration::from_millis(100);
            core.decide(now);
            let window = WindowKey::Main;
            core.handle(
                Event::Click {
                    window,
                    at: [0.66, 0.12],
                },
                now,
            );
            core.handle(Event::Pointer(Some((window, [0.86, 0.3]))), now);
            if option == "zoom" {
                core.handle(
                    Event::Release {
                        window,
                        at: [0.86, 0.3],
                    },
                    now,
                );
                // The pointer in the zoom window, for its pitch readout.
                core.handle(Event::Pointer(Some((window, [0.25, 0.25]))), now);
            }
            for block in noise(1.0, 5).chunks(1024) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(block), now);
            }
        }
        Some(_) if custom.is_some() => {}
        // The Loudness Meter with its bars on, in the Bar or a window.
        Some(option @ ("bars" | "bars-window")) => {
            if option == "bars-window" {
                core.handle(Event::SetMode(LayoutMode::Window), now);
            }
            let meter = (0..8)
                .find(|&i| {
                    matches!(
                        core.meter_settings(i),
                        Some(dasmeter_core::MeterSettings::Loudness(_))
                    )
                })
                .ok_or("there's no Loudness Meter")?;
            let Some(dasmeter_core::MeterSettings::Loudness(mut settings)) =
                core.meter_settings(meter)
            else {
                unreachable!()
            };
            settings.show_bars = true;
            core.handle(
                Event::SetMeter {
                    meter,
                    settings: dasmeter_core::MeterSettings::Loudness(settings),
                },
                now,
            );
            for block in noise(1.0, 7).chunks(1024) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(block), now);
            }
        }
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
        // The Spectrogram in the Spectrum's place, on a sweep from 60 Hz to
        // 12 kHz over pink noise, with a 1 kHz tone joining halfway; or, in a
        // window, zoomed into the box around the tone.
        Some(option @ ("spectrogram" | "spectrogram-zoom")) => {
            let settings =
                dasmeter_core::MeterSettings::default_of(dasmeter_core::MeterKind::Spectrogram);
            core.handle(Event::SetMeter { meter: 1, settings }, now);
            if option == "spectrogram-zoom" {
                core.handle(Event::SetMode(LayoutMode::Window), now);
                core.handle(Event::StartListening, now);
                now += Duration::from_millis(100);
                core.decide(now);
                let window = WindowKey::Main;
                core.handle(
                    Event::Click {
                        window,
                        at: [0.55, 0.2],
                    },
                    now,
                );
                core.handle(Event::Pointer(Some((window, [0.95, 0.3]))), now);
                core.handle(
                    Event::Release {
                        window,
                        at: [0.95, 0.3],
                    },
                    now,
                );
            }
            let seconds = 10.0;
            let n = frames(RATE, seconds);
            let noise = pink_noise(n, -40.0, 9);
            let (f0, f1) = (60.0_f64, 12_000.0_f64);
            let rate = (f1 / f0).ln() / seconds;
            let audio: Vec<f32> = (0..n)
                .flat_map(|i| {
                    let t = i as f64 / f64::from(RATE);
                    let phase = std::f64::consts::TAU * f0 * ((rate * t).exp() - 1.0) / rate;
                    let mut x = 0.25 * phase.sin() + f64::from(noise[i]);
                    if t > seconds / 2.0 {
                        x += 0.1 * (std::f64::consts::TAU * 1_000.0 * t).sin();
                    }
                    [x as f32, x as f32]
                })
                .collect();
            for block in audio.chunks(1024) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(block), now);
                core.decide(now);
            }
        }
        // The Spectrum coloured by steady harmonics, on a 55 Hz bass tone
        // with its harmonics over pink noise, the pointer over it.
        Some("harmonics") => {
            let Some(dasmeter_core::MeterSettings::Spectrum(mut settings)) = core.meter_settings(1)
            else {
                return Err("the second Meter isn't a Spectrum".into());
            };
            settings.colouring = dasmeter_core::SpectrumColouring::SteadyHarmonics;
            core.handle(
                Event::SetMeter {
                    meter: 1,
                    settings: dasmeter_core::MeterSettings::Spectrum(settings),
                },
                now,
            );
            let n = frames(RATE, 4.0);
            let noise = pink_noise(n, -36.0, 11);
            let audio: Vec<f32> = (0..n)
                .flat_map(|i| {
                    let t = i as f64 / f64::from(RATE);
                    let x: f64 = (1..=8)
                        .map(|k| {
                            let k = f64::from(k);
                            (std::f64::consts::TAU * 55.0 * k * t).sin() * 0.3 / k
                        })
                        .sum::<f64>()
                        + f64::from(noise[i]);
                    [x as f32, x as f32]
                })
                .collect();
            for block in audio.chunks(1024) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(block), now);
                core.decide(now);
            }
            core.handle(Event::Pointer(Some((WindowKey::Bar, [0.3, 0.5]))), now);
        }
        // The Cepstrum in the Spectrum's place, on a 220 Hz harmonic tone,
        // with the pointer over it.
        Some("cepstrum") => {
            let settings =
                dasmeter_core::MeterSettings::default_of(dasmeter_core::MeterKind::Cepstrum);
            core.handle(Event::SetMeter { meter: 1, settings }, now);
            let partials = 100;
            let tone: Vec<f32> = (0..RATE as usize)
                .flat_map(|i| {
                    let t = i as f64 / f64::from(RATE);
                    let x: f64 = (1..=partials)
                        .map(|k| {
                            (std::f64::consts::TAU * 220.0 * f64::from(k) * t).sin() / f64::from(k)
                        })
                        .sum();
                    let x = (0.15 * x) as f32;
                    [x, x]
                })
                .collect();
            for block in tone.chunks(1024) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(block), now);
                core.decide(now);
            }
            core.handle(Event::Pointer(Some((WindowKey::Bar, [0.4, 0.5]))), now);
        }
        // The Phase Scope in the Spectrum's place, on a kick and a bass at
        // 120 BPM: the typed-in tempo, a beat or (`phase-scope-bar`) a bar,
        // filled, or Left and Right (the right channel quieter).
        Some(
            option @ ("phase-scope" | "phase-scope-bar" | "phase-scope-filled" | "phase-scope-lr"),
        ) => {
            let mut settings = dasmeter_core::PhaseScopeMeterSettings::default();
            match option {
                "phase-scope-bar" => settings.cycle = dasmeter_core::CycleLength::Bar,
                "phase-scope-filled" => settings.filled = true,
                "phase-scope-lr" => {
                    settings.channel_view = dasmeter_analysis::ChannelView::LeftRight;
                }
                _ => {}
            }
            let right = if option == "phase-scope-lr" { 0.5 } else { 1.0 };
            let settings = dasmeter_core::MeterSettings::PhaseScope(settings);
            core.handle(Event::SetMeter { meter: 1, settings }, now);
            let audio: Vec<f32> = kick_and_bass(8.0)
                .into_iter()
                .flat_map(|x| [x, x * right])
                .collect();
            for block in audio.chunks(1024) {
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                core.handle(Event::Audio(block), now);
                core.decide(now);
            }
        }
        // The Phase Scope on a Kick Send Plugin with the Bass from the same
        // DAW as its Overlay Source, both following the DAW at 120 BPM,
        // with suggestions on.
        Some("phase-scope-overlay") => {
            let settings = dasmeter_core::MeterSettings::PhaseScope(
                dasmeter_core::PhaseScopeMeterSettings {
                    suggestions: true,
                    ..Default::default()
                },
            );
            core.handle(Event::SetMeter { meter: 1, settings }, now);
            core.handle(
                Event::SetListenTo(dasmeter_core::ListenTo::SendPlugins),
                now,
            );
            let plugin = |id: u64, name: &str, colour: u32| dasmeter_core::SendPlugin {
                id,
                name: name.to_owned(),
                colour,
                mono: false,
                sample_rate: RATE,
                state: dasmeter_core::SendPluginState::Live,
                outdated: false,
                host_pid: 1,
            };
            let listed = [plugin(1, "Kick", 0xff_6a_3d), plugin(2, "Bass", 0x4d_a3_ff)];
            core.handle(Event::SendPlugins(&listed), now);
            core.handle(Event::PickSendPlugin { meter: 1, id: 1 }, now);
            core.handle(
                Event::PickOverlay {
                    meter: 1,
                    id: Some(2),
                },
                now,
            );
            let (kick, bass) = kick_and_bass_apart(8.0);
            for (block, (kick, bass)) in kick.chunks(512).zip(bass.chunks(512)).enumerate() {
                let beats = (block * 512) as f64 / f64::from(RATE) * 2.0;
                let timing = Some(dasmeter_core::Timing {
                    tempo: 120.0,
                    beats,
                    bar_start: (beats / 4.0).floor() * 4.0,
                    signature: (4, 4),
                    playing: true,
                });
                now += Duration::from_secs_f64(512.0 / f64::from(RATE));
                for (id, mono) in [(1, kick), (2, bass)] {
                    let frames: Vec<f32> = mono.iter().flat_map(|&x| [x, x]).collect();
                    core.handle(
                        Event::SendPluginAudio {
                            id,
                            frames: &frames,
                            timing,
                        },
                        now,
                    );
                }
                core.decide(now);
            }
        }
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
