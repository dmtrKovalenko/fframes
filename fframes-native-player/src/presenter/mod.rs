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

use crate::controls::{Control, Controls, ControlsLayout, ControlsState, mix};
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

    /// Draws the next image of the window with `draw` and shows it. The GPU backends
    /// return before the GPU is done: Skia copies video frame pixels into its staging
    /// memory during the flush, so the tree can be dropped once this returns.
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

    /// Draws the current frame and the chrome around it before presenting the window.
    /// `controls.motion.studio` places the view between focus (0) and studio (1): the
    /// video grows or shrinks into place while the panel slides off its edge.
    pub(crate) fn present(
        &mut self,
        frame: Option<&usvgr::Tree>,
        controls: &ControlsState,
        model: &Studio,
        scroll: &StudioScroll,
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
            let studio = controls.motion.studio;
            let view = studio_layout.as_ref();
            canvas.clear(mix(Color::BLACK, STUDIO_MATTE, studio));
            let size = canvas.base_layer_size();
            let window = Rect::from_iwh(size.width, size.height);
            let picture = video_rect(
                frame.map(usvgr::Tree::size),
                window,
                view,
                studio,
                pixel_scale,
            );
            if let (Some(tree), Some(picture)) = (frame, picture) {
                draw_video(canvas, tree, picture, *background, render_cache);
            }
            if let Some((progress, playing)) = controls.motion.flash {
                let center = picture.unwrap_or(window).center();
                bar.flash(canvas, center, pixel_scale, progress, playing);
            }

            if let Some(ui) = studio_layout.as_mut() {
                canvas.save();
                canvas.scale((pixel_scale, pixel_scale));
                if studio > 0. {
                    let (dx, dy) = ui.closed_offset();
                    canvas.save();
                    canvas.translate((dx * (1. - studio), dy * (1. - studio)));
                    ui.draw(canvas, bar, controls, model, scroll);
                    canvas.restore();
                }
                ui.draw_restore(
                    canvas,
                    bar,
                    controls.motion.hover(Control::Focus),
                    controls.motion.dock * (1. - studio),
                );
                canvas.restore();
            }
            bar.draw(canvas, layout, controls);
        })
    }
}

const STUDIO_MATTE: Color = Color::from_rgb(14, 14, 14);

/// Where the video sits, in physical pixels: letterboxed into the window in the focus
/// view, into the stage of the studio, and in between while the view changes.
fn video_rect(
    video: Option<fframes::usvgr::Size>,
    window: Rect,
    view: Option<&StudioLayout>,
    studio: f32,
    pixel_scale: f32,
) -> Option<Rect> {
    let video = video?;
    let fit = |viewport: Rect, top: bool| {
        let scale = (viewport.width() / video.width()).min(viewport.height() / video.height());
        let width = video.width() * scale;
        let height = video.height() * scale;
        let x = viewport.center_x() - width / 2.;
        let y = if top {
            viewport.top
        } else {
            viewport.center_y() - height / 2.
        };
        Rect::from_xywh(x, y, width, height)
    };
    let focus = fit(window, false);
    let Some(ui) = view else {
        return Some(focus);
    };
    let stage = ui.stage;
    let staged = fit(
        Rect::from_ltrb(
            stage.left * pixel_scale,
            stage.top * pixel_scale,
            stage.right * pixel_scale,
            stage.bottom * pixel_scale,
        ),
        ui.portrait,
    );
    let lerp = |a: f32, b: f32| a + (b - a) * studio;
    Some(Rect::from_ltrb(
        lerp(focus.left, staged.left),
        lerp(focus.top, staged.top),
        lerp(focus.right, staged.right),
        lerp(focus.bottom, staged.bottom),
    ))
}

fn draw_video(
    canvas: &Canvas,
    tree: &usvgr::Tree,
    picture: Rect,
    background: Color,
    cache: &mut RenderCache,
) {
    let video = tree.size();
    let scale = picture.width() / video.width();
    canvas.save();
    canvas.translate((picture.left, picture.top));
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
