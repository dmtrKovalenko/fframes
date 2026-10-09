use crate::{SkiaBackend, SkiaContext, lock_queue};
use fframes::{
    Color, FFramesRendererError, FFramesRendererResult, FrameRenderer, RgbaFrame, usvgr,
};
use skia_safe::{AlphaType, Canvas, ColorType, ImageInfo, Surface, gpu, surfaces};

/// Creates a surface of the requested size. The backends create surfaces of the size they
/// were constructed with; scaled renders (`scale_resolution`, previews) need another size.
pub fn surface_with_size<TBackend: SkiaBackend + ?Sized>(
    backend: &TBackend,
    width: i32,
    height: i32,
) -> FFramesRendererResult<(Surface, Option<gpu::DirectContext>)> {
    let context = context_with_size(backend, width, height)?;
    Ok((context.surface, context.gpu))
}

/// [`surface_with_size`] with everything else of the new context, see
/// [`SkiaBackend::create_skia_context`].
pub fn context_with_size<TBackend: SkiaBackend + ?Sized>(
    backend: &TBackend,
    width: i32,
    height: i32,
) -> FFramesRendererResult<SkiaContext> {
    let context = backend.create_skia_context()?;
    if context.surface.width() == width && context.surface.height() == height {
        return Ok(context);
    }

    let queue_lock = context.queue_lock.clone();
    let _queue = lock_queue(queue_lock.as_ref());
    // Declare the context after the guard so failed resizing drops it while locked.
    let mut context = context;
    let info = ImageInfo::new(
        (width, height),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    context.surface = match context.gpu.as_mut() {
        Some(gpu_context) => gpu::surfaces::render_target(
            gpu_context,
            gpu::Budgeted::Yes,
            &info,
            None,
            gpu::SurfaceOrigin::TopLeft,
            None,
            false,
            None,
        ),
        None => surfaces::raster(&info, None, None),
    }
    .ok_or_else(|| {
        FFramesRendererError::Skia(format!("can not create a {width}x{height} surface"))
    })?;

    Ok(context)
}

/// Scales the canvas so `tree` fills `width`x`height`, see `fframes::fit_transform`.
pub fn apply_fit(canvas: &Canvas, tree: &usvgr::Tree, width: i32, height: i32) {
    let fit = fframes::fit_transform(tree, width as u32, height as u32);
    if !fit.is_identity() {
        canvas.scale((fit.sx, fit.sy));
    }
}

/// Skia as a `fframes::FrameRenderer` for `fframes::Previewer` and the CLI. Keeps the surface,
/// GPU context and render cache between frames.
pub struct SkiaFrameRenderer<'a, TBackend: SkiaBackend> {
    backend: &'a TBackend,
    cache_config: crate::SkiaCacheConfig,
    surface: Option<SkiaContext>,
    render_cache: crate::render::RenderCache,
}

impl<TBackend: SkiaBackend> SkiaFrameRenderer<'_, TBackend> {
    /// Drops the surface and its GPU context. Destroying a context waits for its queue.
    fn release_surface(&mut self) {
        if let Some(SkiaContext {
            surface,
            gpu,
            queue_lock,
            reader,
        }) = self.surface.take()
        {
            // what lives on the context goes first
            self.render_cache = crate::render::RenderCache::with_config(self.cache_config);
            drop(surface);
            let _queue = lock_queue(queue_lock.as_ref());
            drop(gpu);
            // Keep readback resources alive until the GPU context has waited.
            drop(reader);
        }
    }
}

impl<TBackend: SkiaBackend> Drop for SkiaFrameRenderer<'_, TBackend> {
    fn drop(&mut self) {
        self.release_surface();
    }
}

impl<'a, TBackend: SkiaBackend> SkiaFrameRenderer<'a, TBackend> {
    pub fn new(backend: &'a TBackend) -> Self {
        Self {
            backend,
            cache_config: crate::SkiaCacheConfig::default(),
            surface: None,
            render_cache: crate::render::RenderCache::new(),
        }
    }

    /// Uses these cache limits for subsequent frames. Clears previously cached render resources.
    pub fn with_cache_config(mut self, config: crate::SkiaCacheConfig) -> Self {
        self.cache_config = config;
        self.render_cache = crate::render::RenderCache::with_config(config);
        self
    }
}

impl<TBackend: SkiaBackend> FrameRenderer for SkiaFrameRenderer<'_, TBackend> {
    fn fast_shapes(&self) -> bool {
        true
    }

    fn svg_text_cache_capacity(&self) -> Option<usize> {
        Some(self.cache_config.text_capacity)
    }

    fn render_tree(
        &mut self,
        tree: &usvgr::Tree,
        background: Color,
        width: u32,
        height: u32,
    ) -> FFramesRendererResult<RgbaFrame> {
        let (w, h) = (width as i32, height as i32);
        if !matches!(&self.surface, Some(c) if c.surface.width() == w && c.surface.height() == h) {
            self.release_surface();
            self.surface = Some(context_with_size(self.backend, w, h)?);
        }
        let SkiaContext {
            surface,
            gpu: gpu_context,
            queue_lock,
            ..
        } = self.surface.as_mut().expect("surface was just created");

        let canvas = surface.canvas();
        canvas.clear(skia_safe::Color::from_argb(
            background.a,
            background.r,
            background.g,
            background.b,
        ));
        canvas.save();
        apply_fit(canvas, tree, w, h);
        crate::render::render_tree(tree, canvas, &mut self.render_cache);
        canvas.restore();

        // the flush and the readback submit to the queue of the context
        let _queue = lock_queue(queue_lock.as_ref());
        if let Some(gpu_context) = gpu_context.as_mut() {
            gpu_context.flush_submit_and_sync_cpu();
        }

        // Always read back as RGBA regardless of the surface's native (often BGRA) order.
        let info = ImageInfo::new((w, h), ColorType::RGBA8888, AlphaType::Premul, None);
        let row_bytes = info.min_row_bytes();
        let mut pixels = vec![0; row_bytes * height as usize];
        let image = surface.image_snapshot();
        let pixmap = skia_safe::Pixmap::new(&info, &mut pixels, row_bytes)
            .ok_or_else(|| FFramesRendererError::Skia("can not create a pixmap".to_owned()))?;

        if !image.read_pixels_to_pixmap_with_context(
            gpu_context.as_mut(),
            &pixmap,
            (0, 0),
            skia_safe::image::CachingHint::Allow,
        ) {
            return Err(FFramesRendererError::Skia(
                "failed to read pixels from the Skia surface".to_owned(),
            ));
        }
        drop(pixmap);

        Ok(RgbaFrame::from_premultiplied(width, height, pixels))
    }
}
