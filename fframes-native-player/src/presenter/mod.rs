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
use fframes_skia_renderer::skia_safe::{Canvas, Color, Image, ImageInfo, Rect, gpu, surfaces};
use fframes_skia_renderer::{QueueLock, lock_queue};
use winit::window::Window;

use crate::controls::{Controls, ControlsLayout, ControlsState};
use crate::error::PlayerResult;
use crate::options::PlayerBackend;
use crate::scheduler::PlaybackFrame;
use crate::studio::{Studio, StudioLayout, StudioScroll};

/// The images of the window that Skia draws into.
trait WindowTarget {
    fn is_gpu(&self) -> bool;

    fn gpu_context(&mut self) -> Option<(&mut gpu::DirectContext, Option<&QueueLock>)> {
        None
    }

    /// An empty cache for the canvas of this target. A target on a GPU that the backend
    /// shares with wgpu samples the wgpu layers of the frames in place.
    fn render_cache(&self) -> RenderCache {
        RenderCache::new()
    }

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
    pub studio_layout: Option<StudioLayout>,
    render_cache: RenderCache,
    target: Box<dyn WindowTarget>,
    window: Rc<Window>,
    background: Color,
    controls: Controls,
    layout: ControlsLayout,
    /// The longest time label, sizes the time slots of the control bar.
    duration_label: String,
    audio_rows: usize,
    video_aspect: f32,
}

impl Presenter {
    pub(crate) fn new(
        window: Rc<Window>,
        backend: PlayerBackend,
        vsync: bool,
        background: fframes::Color,
        duration_label: String,
        model: &Studio,
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
            render_cache: target.render_cache(),
            target,
            window,
            background: Color::from_argb(background.a, background.r, background.g, background.b),
            controls,
            layout,
            duration_label,
            studio_layout: None,
            audio_rows: model.audio_rows(),
            video_aspect: model.video_aspect,
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
        let logical = size.to_logical::<f32>(self.window.scale_factor());
        self.studio_layout = StudioLayout::new(
            logical.width,
            logical.height,
            self.audio_rows,
            self.video_aspect,
        );
        self.layout = ControlsLayout::new(
            &self.controls,
            &self.duration_label,
            size.width as f32,
            size.height as f32,
            self.window.scale_factor() as f32,
        );

        Ok(())
    }

    /// Draws just the scene. Controls and the timeline are composited when it is shown.
    pub(crate) fn snapshot(&mut self, tree: &usvgr::Tree, studio_open: bool) -> Option<Image> {
        let size = self.window.inner_size();
        let info = ImageInfo::new_n32_premul((size.width as i32, size.height as i32), None);
        let mut context = self.target.gpu_context();
        let mut surface = match context.as_mut() {
            Some((gpu, queue)) => {
                let _lock = lock_queue(*queue);
                gpu::surfaces::render_target(
                    gpu,
                    gpu::Budgeted::Yes,
                    &info,
                    None,
                    gpu::SurfaceOrigin::TopLeft,
                    None,
                    false,
                    None,
                )
            }
            None => surfaces::raster(&info, None, None),
        }?;
        draw_scene(
            surface.canvas(),
            Some(tree),
            self.studio_layout.as_ref().filter(|_| studio_open),
            self.window.scale_factor() as f32,
            self.background,
            &mut self.render_cache,
        );
        let image = surface.image_snapshot();
        if let Some((gpu, queue)) = context {
            let _lock = lock_queue(queue);
            gpu.flush_and_submit_surface(&mut surface, gpu::SyncCpu::Yes);
        }
        Some(image)
    }

    /// Draws the current frame and overlays the controls before presenting the window.
    pub(crate) fn present(
        &mut self,
        frame: Option<&PlaybackFrame>,
        controls: &ControlsState,
        model: &Studio,
        scroll: &StudioScroll,
        studio_open: bool,
    ) -> PlayerResult<()> {
        let pixel_scale = self.window.scale_factor() as f32;
        let Self {
            render_cache,
            target,
            background,
            controls: bar,
            layout,
            studio_layout,
            ..
        } = self;

        target.present(&mut |canvas| {
            let view = studio_layout.as_ref().filter(|_| studio_open);
            match frame {
                Some(PlaybackFrame::Image(image)) => {
                    canvas.draw_image(image, (0, 0), None);
                }
                Some(PlaybackFrame::Tree(tree)) => draw_scene(
                    canvas,
                    Some(tree),
                    view,
                    pixel_scale,
                    *background,
                    render_cache,
                ),
                None => draw_scene(canvas, None, view, pixel_scale, *background, render_cache),
            }

            let show_bar = view.is_none();
            if let Some(ui) = studio_layout.as_mut() {
                canvas.save();
                canvas.scale((pixel_scale, pixel_scale));
                if studio_open {
                    ui.draw(canvas, bar, controls, model, scroll);
                } else if controls.visible {
                    ui.draw_restore(
                        canvas,
                        bar,
                        controls.hovered == Some(crate::controls::Control::Focus),
                    );
                }
                canvas.restore();
            }
            if show_bar {
                bar.draw(canvas, layout, controls);
            }
        })
    }
}

fn draw_scene(
    canvas: &Canvas,
    tree: Option<&usvgr::Tree>,
    view: Option<&StudioLayout>,
    pixel_scale: f32,
    background: Color,
    render_cache: &mut RenderCache,
) {
    let size = canvas.base_layer_size();
    let (width, height) = (size.width as f32, size.height as f32);
    let viewport = view.map_or_else(
        || Rect::from_wh(width, height),
        |ui| {
            let stage = ui.stage;
            Rect::from_ltrb(
                stage.left * pixel_scale,
                stage.top * pixel_scale,
                stage.right * pixel_scale,
                stage.bottom * pixel_scale,
            )
        },
    );
    canvas.clear(if view.is_some() {
        Color::from_rgb(14, 14, 14)
    } else {
        Color::BLACK
    });

    if let Some(tree) = tree {
        let video = tree.size();
        let scale = (viewport.width() / video.width()).min(viewport.height() / video.height());
        let offset_x = viewport.center_x() - video.width() * scale / 2.;
        let offset_y = if view.is_some_and(|ui| ui.portrait) {
            viewport.top
        } else {
            viewport.center_y() - video.height() * scale / 2.
        };

        canvas.save();
        canvas.translate((offset_x, offset_y));
        canvas.scale((scale, scale));
        canvas.clip_rect(
            Rect::from_wh(video.width(), video.height()),
            None,
            Some(true),
        );
        canvas.draw_color(background, None);
        fframes_skia_renderer::render::render_tree(tree, canvas, render_cache);
        canvas.restore();
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
