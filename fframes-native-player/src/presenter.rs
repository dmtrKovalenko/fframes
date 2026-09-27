use std::num::NonZeroU32;
use std::rc::Rc;

use fframes::usvgr;
use fframes_skia_renderer::render::RenderCache;
use fframes_skia_renderer::skia_safe::{
    self, AlphaType, Color, ColorType, ImageInfo, Rect, Surface, gpu,
};
use fframes_skia_renderer::{SkiaBackend, SkiaCpuCtx};
use winit::window::Window;

use crate::controls::{Controls, ControlsLayout, ControlsState};
use crate::error::{PlayerError, PlayerResult};
use crate::options::PlayerBackend;

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
    controls: Controls,
    layout: ControlsLayout,
    /// The longest time label, sizes the time slots of the control bar.
    duration_label: String,
}

impl Presenter {
    pub(crate) fn new(
        window: Rc<Window>,
        backend: PlayerBackend,
        background: fframes::Color,
        duration_label: String,
    ) -> PlayerResult<Self> {
        let context = softbuffer::Context::new(window.clone())?;
        let window_surface = softbuffer::Surface::new(&context, window.clone())?;
        let (backend, gpu_context) = create_backend(backend)?;
        let controls = Controls::new();
        let size = window.inner_size();
        let layout = ControlsLayout::new(
            &controls,
            &duration_label,
            size.width as f32,
            size.height as f32,
            window.scale_factor() as f32,
        );

        let mut presenter = Self {
            surface: None,
            render_cache: RenderCache::new(),
            gpu_context,
            _backend: backend,
            window_surface,
            window,
            background: Color::from_argb(background.a, background.r, background.g, background.b),
            controls,
            layout,
            duration_label,
        };

        presenter.resize()?;
        Ok(presenter)
    }

    pub(crate) fn is_gpu(&self) -> bool {
        self.gpu_context.is_some()
    }

    /// Where the control bar is for the current window size, in physical pixels.
    pub(crate) fn controls_layout(&self) -> &ControlsLayout {
        &self.layout
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
        self.layout = ControlsLayout::new(
            &self.controls,
            &self.duration_label,
            size.width as f32,
            size.height as f32,
            self.window.scale_factor() as f32,
        );

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

    /// Renders `tree` letterboxed into the window with the controls on top and presents it.
    pub(crate) fn present(
        &mut self,
        tree: Option<&usvgr::Tree>,
        controls: &ControlsState,
    ) -> PlayerResult<()> {
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

        self.controls.draw(canvas, &self.layout, controls);

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
