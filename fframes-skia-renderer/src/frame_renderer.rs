use crate::SkiaBackend;
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
    let (surface, mut gpu_context) = backend.create_skia_surface()?;
    if surface.width() == width && surface.height() == height {
        return Ok((surface, gpu_context));
    }

    let info = ImageInfo::new(
        (width, height),
        ColorType::RGBA8888,
        AlphaType::Premul,
        None,
    );
    let surface = match gpu_context.as_mut() {
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

    Ok((surface, gpu_context))
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
    surface: Option<(Surface, Option<gpu::DirectContext>)>,
    render_cache: crate::render::RenderCache,
}

impl<'a, TBackend: SkiaBackend> SkiaFrameRenderer<'a, TBackend> {
    pub fn new(backend: &'a TBackend) -> Self {
        Self {
            backend,
            surface: None,
            render_cache: crate::render::RenderCache::new(),
        }
    }
}

impl<TBackend: SkiaBackend> FrameRenderer for SkiaFrameRenderer<'_, TBackend> {
    fn render_tree(
        &mut self,
        tree: &usvgr::Tree,
        background: Color,
        width: u32,
        height: u32,
    ) -> FFramesRendererResult<RgbaFrame> {
        let (w, h) = (width as i32, height as i32);
        if !matches!(&self.surface, Some((s, _)) if s.width() == w && s.height() == h) {
            self.surface = Some(surface_with_size(self.backend, w, h)?);
        }
        let (surface, gpu_context) = self.surface.as_mut().expect("surface was just created");

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
