use std::num::NonZeroU32;
use std::rc::Rc;

use fframes::usvgr;
use fframes_skia_renderer::render::RenderCache;
use fframes_skia_renderer::skia_safe::{
    self, AlphaType, Color, ColorType, ImageInfo, Paint, Rect, Surface, gpu,
};
use fframes_skia_renderer::{SkiaBackend, SkiaCpuCtx};
use winit::window::Window;

use crate::error::{PlayerError, PlayerResult};
use crate::options::PlayerBackend;

/// Everything drawn over the video.
pub(crate) struct Overlay {
    pub visible: bool,
    /// Playback progress in `0.0..=1.0`.
    pub progress: f32,
    pub paused: bool,
}

/// Height of the seek area at the bottom of the window in logical pixels.
pub(crate) const SEEK_BAR_HIT_HEIGHT: f64 = 32.0;

// Fields drop in declaration order: skia resources first, then the context, then the
// backend that owns the device.
pub(crate) struct Presenter {
    surface: Option<Surface>,
    render_cache: RenderCache,
    gpu_context: Option<gpu::DirectContext>,
    _backend: Box<dyn SkiaBackend>,
    window_surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    window: Rc<Window>,
    background: Color,
}

impl Presenter {
    pub(crate) fn new(
        window: Rc<Window>,
        backend: PlayerBackend,
        background: fframes::Color,
    ) -> PlayerResult<Self> {
        let context = softbuffer::Context::new(window.clone())?;
        let window_surface = softbuffer::Surface::new(&context, window.clone())?;
        let (backend, gpu_context) = create_backend(backend)?;

        let mut presenter = Self {
            surface: None,
            render_cache: RenderCache::new(),
            gpu_context,
            _backend: backend,
            window_surface,
            window,
            background: Color::from_argb(background.a, background.r, background.g, background.b),
        };

        presenter.resize()?;
        Ok(presenter)
    }

    pub(crate) fn is_gpu(&self) -> bool {
        self.gpu_context.is_some()
    }

    /// Recreates the surfaces for the current window size.
    pub(crate) fn resize(&mut self) -> PlayerResult<()> {
        let size = self.window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            // Minimized, keep the old surface around until the window is visible again.
            return Ok(());
        };

        self.window_surface.resize(width, height)?;

        let info = surface_image_info(size.width, size.height);
        let surface = match self.gpu_context.as_mut() {
            Some(context) => gpu::surfaces::render_target(
                context,
                gpu::Budgeted::Yes,
                &info,
                None,
                gpu::SurfaceOrigin::TopLeft,
                None,
                false,
                None,
            ),
            None => skia_safe::surfaces::raster(&info, None, None),
        };

        self.surface = Some(surface.ok_or_else(|| {
            PlayerError::Skia(format!(
                "failed to create a {}x{} surface",
                size.width, size.height
            ))
        })?);

        Ok(())
    }

    /// Renders `tree` letterboxed into the window with the overlay on top and presents it.
    pub(crate) fn present(
        &mut self,
        tree: Option<&usvgr::Tree>,
        overlay: &Overlay,
    ) -> PlayerResult<()> {
        let scale_factor = self.window.scale_factor() as f32;
        let Some(surface) = self.surface.as_mut() else {
            return Ok(());
        };

        let (width, height) = (surface.width(), surface.height());
        let canvas = surface.canvas();
        canvas.clear(Color::BLACK);

        if let Some(tree) = tree {
            let video = tree.size();
            let scale = (width as f32 / video.width()).min(height as f32 / video.height());
            let offset_x = (width as f32 - video.width() * scale) / 2.;
            let offset_y = (height as f32 - video.height() * scale) / 2.;

            canvas.save();
            canvas.translate((offset_x, offset_y));
            canvas.scale((scale, scale));
            canvas.clip_rect(
                Rect::from_wh(video.width(), video.height()),
                None,
                Some(true),
            );
            canvas.draw_color(self.background, None);
            fframes_skia_renderer::render::render_tree(tree, canvas, &mut self.render_cache);
            canvas.restore();
        }

        if overlay.visible {
            draw_overlay(canvas, overlay, width as f32, height as f32, scale_factor);
        }

        // Video frame images point into the tree's pixel buffers: wait for the GPU so the
        // tree can be dropped as soon as the next frame replaces it.
        if let Some(context) = self.gpu_context.as_mut() {
            context.flush_submit_and_sync_cpu();
        }

        let mut buffer = self.window_surface.buffer_mut()?;
        // The window surface might still have the previous size if the resize event
        // has not been processed yet: skip this frame, a redraw follows the resize.
        if buffer.len() != (width * height) as usize {
            return Ok(());
        }

        let info = surface_image_info(width as u32, height as u32);
        let pixels = fframes::bytemuck::cast_slice_mut::<u32, u8>(&mut buffer);
        if !surface.read_pixels(&info, pixels, width as usize * 4, (0, 0)) {
            return Err(PlayerError::Skia(
                "failed to read back the frame".to_string(),
            ));
        }

        buffer.present()?;
        Ok(())
    }
}

fn surface_image_info(width: u32, height: u32) -> ImageInfo {
    // BGRA in memory is `0xAARRGGBB` as a little-endian `u32`, which is what softbuffer
    // expects (it ignores the alpha byte).
    ImageInfo::new(
        (width as i32, height as i32),
        ColorType::BGRA8888,
        AlphaType::Premul,
        None,
    )
}

fn draw_overlay(
    canvas: &skia_safe::Canvas,
    overlay: &Overlay,
    width: f32,
    height: f32,
    scale_factor: f32,
) {
    let bar_height = 4. * scale_factor;
    let top = height - bar_height;

    let mut paint = Paint::default();
    paint.set_anti_alias(true);

    paint.set_color(Color::from_argb(110, 255, 255, 255));
    canvas.draw_rect(Rect::from_xywh(0., top, width, bar_height), &paint);

    paint.set_color(Color::from_rgb(255, 72, 72));
    canvas.draw_rect(
        Rect::from_xywh(0., top, width * overlay.progress.clamp(0., 1.), bar_height),
        &paint,
    );

    if overlay.paused {
        // Two vertical bars in the bottom-left corner.
        let size = 14. * scale_factor;
        let left = 12. * scale_factor;
        let bottom = top - 10. * scale_factor;
        paint.set_color(Color::from_argb(200, 255, 255, 255));
        for x in [left, left + size * 0.6] {
            canvas.draw_rect(Rect::from_xywh(x, bottom - size, size * 0.35, size), &paint);
        }
    }
}

type Backend = (Box<dyn SkiaBackend>, Option<gpu::DirectContext>);

fn create_backend(backend: PlayerBackend) -> PlayerResult<Backend> {
    match backend {
        PlayerBackend::Auto => {
            #[cfg(feature = "metal")]
            match create_backend(PlayerBackend::Metal) {
                Ok(backend) => return Ok(backend),
                Err(err) => eprintln!("fframes player: metal is not available ({err})"),
            }

            #[cfg(feature = "vulkan")]
            match create_backend(PlayerBackend::Vulkan) {
                Ok(backend) => return Ok(backend),
                Err(err) => eprintln!("fframes player: vulkan is not available ({err})"),
            }

            eprintln!("fframes player: falling back to the Skia CPU renderer");
            create_backend(PlayerBackend::Cpu)
        }
        #[cfg(feature = "metal")]
        PlayerBackend::Metal => {
            gpu_backend(fframes_skia_renderer::metal::SkiaMetalCtx::new(16, 16))
        }
        #[cfg(feature = "vulkan")]
        PlayerBackend::Vulkan => {
            gpu_backend(fframes_skia_renderer::vulkan::SkiaVulkanCtx::new(16, 16))
        }
        PlayerBackend::Cpu => Ok((Box::new(SkiaCpuCtx::new(16, 16)), None)),
    }
}

/// The backend contexts create a fixed-size surface: only its GPU context is kept, the
/// player creates its own window-sized surfaces on it.
#[cfg(any(feature = "metal", feature = "vulkan"))]
fn gpu_backend<T: SkiaBackend + 'static>(
    backend: fframes::FFramesRendererResult<T>,
) -> PlayerResult<Backend> {
    let backend = backend?;
    let (_surface, context) = backend.create_skia_surface()?;
    let context = context.ok_or_else(|| PlayerError::Skia("no GPU context".to_string()))?;

    Ok((Box::new(backend), Some(context)))
}
