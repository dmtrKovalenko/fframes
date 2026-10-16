//! Draws the frames into the window. The GPU backends render into the images the window
//! shows (a `CAMetalLayer` drawable or a Vulkan swapchain image) and the CPU backend into
//! the pixel buffer of the window, so no frame is read back from the GPU.

mod cpu;
#[cfg(feature = "metal")]
mod metal;
#[cfg(feature = "vulkan")]
mod vulkan;

use std::rc::Rc;

use fframes::usvgr;
use fframes_skia_renderer::render::RenderCache;
use fframes_skia_renderer::skia_safe::{Canvas, Color, Rect};
use winit::window::Window;

use crate::controls::{Controls, ControlsLayout, ControlsState};
use crate::error::PlayerResult;
use crate::options::PlayerBackend;

/// The images of the window that Skia draws into.
trait WindowTarget {
    fn is_gpu(&self) -> bool;

    /// The window has a new size in physical pixels, neither is zero.
    fn resize(&mut self, width: u32, height: u32) -> PlayerResult<()>;

    /// Draws the next image of the window with `draw` and shows it. Returns once the GPU is
    /// done: video frame images point into the pixel buffers of the tree, which is dropped
    /// as soon as the next frame replaces it.
    fn present(&mut self, draw: &mut dyn FnMut(&Canvas)) -> PlayerResult<()>;
}

// Fields drop in declaration order: the cached Skia objects before the target that owns
// the context and the device.
pub(crate) struct Presenter {
    render_cache: RenderCache,
    target: Box<dyn WindowTarget>,
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
        vsync: bool,
        background: fframes::Color,
        duration_label: String,
    ) -> PlayerResult<Self> {
        let target = create_target(&window, backend, vsync)?;
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
            render_cache: RenderCache::new(),
            target,
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
        self.target.is_gpu()
    }

    /// Where the control bar is for the current window size, in physical pixels.
    pub(crate) fn controls_layout(&self) -> &ControlsLayout {
        &self.layout
    }

    /// Resizes the window images to the current window size.
    pub(crate) fn resize(&mut self) -> PlayerResult<()> {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            // Minimized, keep the old images around until the window is visible again.
            return Ok(());
        }

        self.target.resize(size.width, size.height)?;
        self.layout = ControlsLayout::new(
            &self.controls,
            &self.duration_label,
            size.width as f32,
            size.height as f32,
            self.window.scale_factor() as f32,
        );

        Ok(())
    }

    /// Renders `tree` letterboxed into the window with the controls on top and presents it.
    pub(crate) fn present(
        &mut self,
        tree: Option<&usvgr::Tree>,
        controls: &ControlsState,
    ) -> PlayerResult<()> {
        let Self {
            render_cache,
            target,
            background,
            controls: bar,
            layout,
            ..
        } = self;

        target.present(&mut |canvas| {
            let size = canvas.base_layer_size();
            let (width, height) = (size.width as f32, size.height as f32);
            canvas.clear(Color::BLACK);

            if let Some(tree) = tree {
                let video = tree.size();
                let scale = (width / video.width()).min(height / video.height());
                let offset_x = (width - video.width() * scale) / 2.;
                let offset_y = (height - video.height() * scale) / 2.;

                canvas.save();
                canvas.translate((offset_x, offset_y));
                canvas.scale((scale, scale));
                canvas.clip_rect(
                    Rect::from_wh(video.width(), video.height()),
                    None,
                    Some(true),
                );
                canvas.draw_color(*background, None);
                fframes_skia_renderer::render::render_tree(tree, canvas, render_cache);
                canvas.restore();
            }

            bar.draw(canvas, layout, controls);
        })
    }
}

fn create_target(
    window: &Rc<Window>,
    backend: PlayerBackend,
    vsync: bool,
) -> PlayerResult<Box<dyn WindowTarget>> {
    match backend {
        PlayerBackend::Auto => {
            #[cfg(feature = "metal")]
            match create_target(window, PlayerBackend::Metal, vsync) {
                Ok(target) => return Ok(target),
                Err(err) => eprintln!("fframes player: metal is not available ({err})"),
            }

            #[cfg(feature = "vulkan")]
            match create_target(window, PlayerBackend::Vulkan, vsync) {
                Ok(target) => return Ok(target),
                Err(err) => eprintln!("fframes player: vulkan is not available ({err})"),
            }

            eprintln!("fframes player: falling back to the Skia CPU renderer");
            create_target(window, PlayerBackend::Cpu, vsync)
        }
        #[cfg(feature = "metal")]
        PlayerBackend::Metal => Ok(Box::new(metal::MetalTarget::new(window, vsync)?)),
        #[cfg(feature = "vulkan")]
        PlayerBackend::Vulkan => Ok(Box::new(vulkan::VulkanTarget::new(window.clone(), vsync)?)),
        PlayerBackend::Cpu => Ok(Box::new(cpu::CpuTarget::new(window.clone())?)),
    }
}
