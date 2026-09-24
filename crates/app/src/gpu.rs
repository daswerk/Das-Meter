//! The window's wgpu surface and the text context shared by Meter renderers.

use std::sync::Arc;

use dasmeter_core::Colour;
use glyphon::{Cache, FontSystem, Resolution, SwashCache, TextAtlas, Viewport};
use winit::window::Window;

/// Everything a renderer needs to draw a frame of a given size.
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
    width: u32,
    height: u32,
}

/// A window's swapchain.
pub struct WindowSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    instance: wgpu::Instance,
    window: Arc<Window>,
}

/// Shaped-text state shared by every renderer in a window.
pub struct Text {
    pub font_system: FontSystem,
    pub swash_cache: SwashCache,
    pub viewport: Viewport,
    pub atlas: TextAtlas,
}

async fn device(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'static>>,
) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue), String> {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            // A meter next to a DAW should stay on the efficient GPU.
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: surface,
            ..Default::default()
        })
        .await
        .map_err(|e| e.to_string())?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .map_err(|e| e.to_string())?;
    Ok((adapter, device, queue))
}

impl Gpu {
    /// A GPU drawing into `window`, and the window's swapchain.
    pub async fn for_window(window: Arc<Window>) -> Result<(Gpu, WindowSurface), String> {
        let instance = wgpu::Instance::default();
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| e.to_string())?;
        let (adapter, device, queue) = device(&instance, Some(&surface)).await?;

        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(capabilities.formats[0]);
        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            // Vsync; the app core caps the rate further and skips frames with nothing new.
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);
        let gpu = Gpu {
            device,
            queue,
            format,
            width: config.width,
            height: config.height,
        };
        Ok((
            gpu,
            WindowSurface {
                surface,
                config,
                instance,
                window,
            },
        ))
    }

    /// A GPU drawing into offscreen textures of the given size.
    pub async fn offscreen(width: u32, height: u32) -> Result<Gpu, String> {
        let (_, device, queue) = device(&wgpu::Instance::default(), None).await?;
        Ok(Gpu {
            device,
            queue,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            width,
            height,
        })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

impl WindowSurface {
    pub fn resize(&mut self, gpu: &mut Gpu, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        gpu.width = self.config.width;
        gpu.height = self.config.height;
        self.surface.configure(&gpu.device, &self.config);
    }

    /// The next frame to draw into, or `None` if the window can't take one now
    /// (the caller asks for a redraw and tries again).
    pub fn frame(&mut self, gpu: &Gpu) -> Option<wgpu::SurfaceTexture> {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => Some(frame),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => None,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Suboptimal(_) => {
                self.surface.configure(&gpu.device, &self.config);
                None
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                if let Ok(surface) = self.instance.create_surface(self.window.clone()) {
                    self.surface = surface;
                    self.surface.configure(&gpu.device, &self.config);
                }
                None
            }
            wgpu::CurrentSurfaceTexture::Validation => panic!("surface validation error"),
        }
    }
}

impl Text {
    pub fn new(gpu: &Gpu) -> Text {
        let cache = Cache::new(&gpu.device);
        Text {
            font_system: FontSystem::new(),
            swash_cache: SwashCache::new(),
            viewport: Viewport::new(&gpu.device, &cache),
            atlas: TextAtlas::new(&gpu.device, &gpu.queue, &cache, gpu.format),
        }
    }

    pub fn update_viewport(&mut self, gpu: &Gpu) {
        let (width, height) = gpu.size();
        self.viewport
            .update(&gpu.queue, Resolution { width, height });
    }
}

/// A colour as shader output for `format`: linear when the surface encodes sRGB.
pub fn linear(colour: Colour, format: wgpu::TextureFormat) -> [f32; 4] {
    let channel = |c: u8| {
        let c = f32::from(c) / 255.0;
        if !format.is_srgb() {
            c
        } else if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    [
        channel(colour.r),
        channel(colour.g),
        channel(colour.b),
        f32::from(colour.a) / 255.0,
    ]
}

/// A colour as a render pass's clear colour.
pub fn clear_colour(colour: Colour, format: wgpu::TextureFormat) -> wgpu::Color {
    let [r, g, b, a] = linear(colour, format);
    wgpu::Color {
        r: f64::from(r),
        g: f64::from(g),
        b: f64::from(b),
        a: f64::from(a),
    }
}

/// A colour for glyphon, which takes sRGB.
pub fn text_colour(colour: Colour) -> glyphon::Color {
    glyphon::Color::rgba(colour.r, colour.g, colour.b, colour.a)
}
