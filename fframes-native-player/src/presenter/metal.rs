use std::ffi::c_void;
use std::time::{Duration, Instant};

use core_graphics_types::geometry::CGSize;
use fframes::usvgr;
use fframes_skia_renderer::metal::SkiaMetalCtx;
use fframes_skia_renderer::metal::metal_rs::foreign_types::{ForeignType, ForeignTypeRef};
use fframes_skia_renderer::metal::metal_rs::objc::Message;
use fframes_skia_renderer::metal::metal_rs::objc::rc::autoreleasepool;
use fframes_skia_renderer::metal::metal_rs::objc::runtime::Sel;
use fframes_skia_renderer::metal::metal_rs::{
    MTLPixelFormat, MTLRegion, MTLStorageMode, MetalLayer, MetalLayerRef, TextureRef,
};
use fframes_skia_renderer::skia_safe::gpu::{
    self, DirectContext, SurfaceOrigin, SyncCpu, backend_render_targets, mtl,
};
use fframes_skia_renderer::skia_safe::{
    AlphaType, Canvas, ColorType, ImageInfo, Surface, surfaces,
};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use super::WindowTarget;
use crate::error::{PlayerError, PlayerResult};
use crate::scheduler::PlaybackFrame;

/// Skia draws into the drawables of a `CAMetalLayer` of the window.
pub(super) struct MetalTarget {
    raster: Option<RasterPreview>,
    gpu: DirectContext,
    layer: MetalLayer,
    backend: SkiaMetalCtx,
}

impl MetalTarget {
    pub(super) fn new(
        window: &Window,
        vsync: bool,
        raster_fps: Option<usize>,
    ) -> PlayerResult<Self> {
        // The size is for the backend's own surfaces, which the player does not use.
        let backend = SkiaMetalCtx::new(16, 16)?;
        let gpu = backend.create_context()?;

        let RawWindowHandle::AppKit(handle) = window.window_handle()?.as_raw() else {
            return Err(PlayerError::Window(
                "metal can only present to an AppKit window".to_owned(),
            ));
        };
        // SAFETY: the view of a live window, `Presenter::new` runs on the main thread.
        let layer = unsafe { raw_window_metal::Layer::from_ns_view(handle.ns_view) };
        // SAFETY: `into_raw` hands over a retained `CAMetalLayer`.
        let layer = unsafe { MetalLayer::from_ptr(layer.into_raw().as_ptr().cast()) };

        layer.set_device(backend.device());
        layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        // Skia copies the drawable for blend modes that read the destination.
        layer.set_framebuffer_only(false);
        layer.set_presents_with_transaction(false);
        layer.set_display_sync_enabled(vsync);
        set_srgb_colorspace(&layer);

        let raster = raster_fps
            .filter(|_| backend.device().has_unified_memory())
            .map(RasterPreview::new);

        Ok(Self {
            raster,
            gpu,
            layer,
            backend,
        })
    }
}

impl WindowTarget for MetalTarget {
    fn gpu_context(
        &mut self,
    ) -> Option<(
        &mut DirectContext,
        Option<&fframes_skia_renderer::QueueLock>,
    )> {
        self.raster = None;
        Some((&mut self.gpu, None))
    }

    fn is_gpu(&self) -> bool {
        self.raster.is_none()
    }

    fn prepare_frame(&mut self, frame: Option<&PlaybackFrame>) {
        if self.raster.is_none() {
            return;
        }
        let mut remaining = MAX_RASTER_NODES;
        if let Some(frame) = frame
            && !matches!(frame, PlaybackFrame::Tree(tree) if simple_group(tree.root(), &mut remaining))
        {
            self.raster = None;
        }
    }

    fn resize(&mut self, width: u32, height: u32) -> PlayerResult<()> {
        self.layer
            .set_drawable_size(CGSize::new(f64::from(width), f64::from(height)));
        Ok(())
    }

    fn present(&mut self, draw: &mut dyn FnMut(&Canvas)) -> PlayerResult<()> {
        // The drawable is autoreleased.
        autoreleasepool(|| {
            // Waits while every drawable of the layer is on screen or queued for it.
            let Some(drawable) = self.layer.next_drawable() else {
                return Ok(());
            };

            let texture = drawable.texture();
            if let Some(raster) = &mut self.raster {
                if raster.draw(texture, draw) {
                    drawable.present();
                    if raster.slow_frames >= 6 {
                        self.raster = None;
                    }
                    return Ok(());
                }
                // Some devices expose private drawable textures. Draw this frame on the GPU.
                self.raster = None;
            }

            // SAFETY: the texture lives as long as the drawable, which outlives the surface.
            let texture_info = unsafe { mtl::TextureInfo::new(texture.as_ptr() as mtl::Handle) };
            let render_target = backend_render_targets::make_mtl(
                (texture.width() as i32, texture.height() as i32),
                &texture_info,
            );
            let mut surface = gpu::surfaces::wrap_backend_render_target(
                &mut self.gpu,
                &render_target,
                SurfaceOrigin::TopLeft,
                ColorType::BGRA8888,
                None,
                None,
            )
            .ok_or_else(|| PlayerError::Skia("can not draw into the drawable".to_owned()))?;

            draw(surface.canvas());
            self.gpu
                .flush_and_submit_surface(&mut surface, SyncCpu::Yes);
            drop(surface);

            // Skia submitted to the same queue, the drawable is presented after its work.
            let commands = self.backend.command_queue().new_command_buffer();
            commands.present_drawable(drawable);
            commands.commit();

            Ok(())
        })
    }
}

/// On unified-memory Macs we can present a CPU-written drawable without submitting a
/// Metal render or blit pass. Keep one full-resolution raster surface between frames.
struct RasterPreview {
    surface: Option<Surface>,
    budget: Duration,
    slow_frames: u8,
}

impl RasterPreview {
    fn new(fps: usize) -> Self {
        Self {
            surface: None,
            budget: Duration::from_secs_f64(1. / (3. * fps.max(1) as f64)),
            slow_frames: 0,
        }
    }

    fn draw(&mut self, texture: &TextureRef, draw: &mut dyn FnMut(&Canvas)) -> bool {
        if !matches!(
            texture.storage_mode(),
            MTLStorageMode::Shared | MTLStorageMode::Managed
        ) {
            return false;
        }

        let start = Instant::now();
        let (width, height) = (texture.width(), texture.height());
        let Ok(w) = i32::try_from(width) else {
            return false;
        };
        let Ok(h) = i32::try_from(height) else {
            return false;
        };
        if self
            .surface
            .as_ref()
            .is_none_or(|s| s.width() != w || s.height() != h)
        {
            let info = ImageInfo::new((w, h), ColorType::BGRA8888, AlphaType::Premul, None);
            self.surface = surfaces::raster(&info, None, None);
        }
        let Some(surface) = self.surface.as_mut() else {
            return false;
        };
        draw(surface.canvas());
        // A raster surface owns contiguous pixels of the drawable's size and BGRA format.
        let Some(pixels) = surface.peek_pixels() else {
            return false;
        };
        texture.replace_region(
            MTLRegion::new_2d(0, 0, width, height),
            0,
            pixels.addr(),
            pixels.row_bytes() as u64,
        );
        if start.elapsed() > self.budget {
            self.slow_frames += 1;
        } else {
            self.slow_frames = 0;
        }
        true
    }
}

const MAX_RASTER_NODES: usize = 128;

/// Keep images (including video, shaders and wgpu layers), complex scenes and blend
/// effects on the GPU. Bound the work of checking a large scene before promotion.
fn simple_group(group: &usvgr::Group, remaining: &mut usize) -> bool {
    !group.should_isolate()
        && group.children().iter().all(|node| {
            if *remaining == 0 {
                return false;
            }
            *remaining -= 1;
            let path = match node {
                usvgr::Node::Group(child) => return simple_group(child, remaining),
                usvgr::Node::Text(text) => return simple_group(text.flattened(), remaining),
                usvgr::Node::Image(_) => return false,
                usvgr::Node::Path(path) => path,
                usvgr::Node::FastShape(shape) => shape.path(),
            };
            // A pattern can contain an arbitrary subtree, including GPU layers.
            path.fill()
                .is_none_or(|fill| !matches!(fill.paint(), usvgr::Paint::Pattern(_)))
                && path
                    .stroke()
                    .is_none_or(|stroke| !matches!(stroke.paint(), usvgr::Paint::Pattern(_)))
        })
}

/// Skia draws sRGB. Tagging the layer makes macOS convert it to the color space of the
/// display, an untagged layer is shown as is (oversaturated on wide gamut displays).
fn set_srgb_colorspace(layer: &MetalLayerRef) {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        static kCGColorSpaceSRGB: *const c_void;
        fn CGColorSpaceCreateWithName(name: *const c_void) -> *mut c_void;
        fn CGColorSpaceRelease(space: *mut c_void);
    }

    // SAFETY: `-[CAMetalLayer setColorspace:]` takes a `CGColorSpaceRef` and retains it.
    unsafe {
        let space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
        if space.is_null() {
            return;
        }

        let _: Result<(), _> = layer.send_message(Sel::register("setColorspace:"), (space,));
        CGColorSpaceRelease(space);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_small_vector_frames_use_the_raster_path() {
        let simple = r#"<path d="M0 0h10v10Z"/>"#;
        for (content, budget, expected) in [
            (simple.to_owned(), 1, true),
            (simple.repeat(2), 1, false),
            (format!(r#"<g opacity=".5">{simple}</g>"#), 128, false),
            (r#"<defs><pattern id="p" width="2" height="2" patternUnits="userSpaceOnUse"><rect width="2" height="2"/></pattern></defs><rect width="8" height="8" fill="url(#p)"/>"#.to_owned(), 128, false),
        ] {
            let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">{content}</svg>"#);
            let tree = usvgr::Tree::from_str(&svg, &usvgr::Options::default(), &usvgr::fontdb::Database::new()).unwrap();
            let mut remaining = budget;
            assert_eq!(simple_group(tree.root(), &mut remaining), expected, "{content}");
        }
    }
}
