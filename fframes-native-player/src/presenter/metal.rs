use std::ffi::c_void;

use core_graphics_types::geometry::CGSize;
use fframes_skia_renderer::metal::SkiaMetalCtx;
use fframes_skia_renderer::metal::metal_rs::foreign_types::{ForeignType, ForeignTypeRef};
use fframes_skia_renderer::metal::metal_rs::objc::Message;
use fframes_skia_renderer::metal::metal_rs::objc::rc::autoreleasepool;
use fframes_skia_renderer::metal::metal_rs::objc::runtime::Sel;
use fframes_skia_renderer::metal::metal_rs::{MTLPixelFormat, MetalLayer, MetalLayerRef};
use fframes_skia_renderer::skia_safe::gpu::{
    self, DirectContext, SurfaceOrigin, SyncCpu, backend_render_targets, mtl,
};
use fframes_skia_renderer::skia_safe::{Canvas, ColorType};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use super::WindowTarget;
use crate::error::{PlayerError, PlayerResult};

/// Skia draws into the drawables of a `CAMetalLayer` of the window.
pub(super) struct MetalTarget {
    gpu: DirectContext,
    layer: MetalLayer,
    backend: SkiaMetalCtx,
}

impl MetalTarget {
    pub(super) fn new(window: &Window, vsync: bool) -> PlayerResult<Self> {
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

        Ok(Self {
            gpu,
            layer,
            backend,
        })
    }
}

impl WindowTarget for MetalTarget {
    fn is_gpu(&self) -> bool {
        true
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
