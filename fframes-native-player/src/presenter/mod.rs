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
use crate::studio::{Studio, StudioLayout, StudioScroll};

/// The images of the window that Skia draws into.
trait WindowTarget {
    fn is_gpu(&self) -> bool;

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
        config: &crate::app::AppConfig<'_>,
    ) -> PlayerResult<Self> {
        let target = create_target(&window, config.backend, config.vsync)?;
        let duration_label = crate::controls::format_time(config.duration_in_frames, config.fps);
        let background = config.background;
        let model = &config.studio;
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

    /// Draws the current frame and overlays the controls before presenting the window.
    pub(crate) fn present(
        &mut self,
        frame: Option<&usvgr::Tree>,
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
            canvas.clear(matte(view));
            if let Some(tree) = frame {
                draw_video(canvas, tree, view, pixel_scale, *background, render_cache);
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

fn matte(view: Option<&StudioLayout>) -> Color {
    if view.is_some() {
        Color::from_rgb(14, 14, 14)
    } else {
        Color::BLACK
    }
}

fn draw_video(
    canvas: &Canvas,
    tree: &usvgr::Tree,
    view: Option<&StudioLayout>,
    pixel_scale: f32,
    background: Color,
    cache: &mut RenderCache,
) {
    let size = canvas.base_layer_size();
    let viewport = view.map_or_else(
        || Rect::from_iwh(size.width, size.height),
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
    let video = tree.size();
    let scale = (viewport.width() / video.width()).min(viewport.height() / video.height());
    let width = video.width() * scale;
    let height = video.height() * scale;
    let x = viewport.center_x() - width / 2.;
    let y = if view.is_some_and(|ui| ui.portrait) {
        viewport.top
    } else {
        viewport.center_y() - height / 2.
    };

    canvas.save();
    canvas.translate((x, y));
    canvas.scale((scale, scale));
    canvas.clip_rect(
        Rect::from_wh(video.width(), video.height()),
        None,
        Some(true),
    );
    canvas.draw_color(background, None);
    fframes_skia_renderer::render::render_tree(tree, canvas, cache);
    canvas.restore();
}

fn create_target(
    window: &Rc<Window>,
    backend: PlayerBackend,
    vsync: bool,
) -> PlayerResult<Box<dyn WindowTarget>> {
    match backend {
        PlayerBackend::Auto => {
            #[cfg(feature = "metal")]
            match metal::MetalTarget::new(window, vsync) {
                Ok(target) => return Ok(Box::new(target)),
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
