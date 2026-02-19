#[cfg(feature = "metal")]
pub mod metal;
#[cfg(feature = "vulkan")]
pub mod vulkan;

use fframes::FFramesRendererResult;
use skia_safe::{Surface, gpu::DirectContext, surfaces};

use crate::SkiaFFramesRenderer;

pub trait SkiaBackend: Sync + Send {
    fn create_skia_surface(&self) -> FFramesRendererResult<(Surface, Option<DirectContext>)>;
}

#[derive(Clone, Copy)]
pub struct SkiaCpuCtx {
    width: usize,
    height: usize,
}

/// This is a glue for making skia fframes renderer work on CPU.
/// Please do not use it for the actual rendering, instead consider to use
/// the built-in provided CPU rendering backend (it is faster).
///
/// This is only intended for additional compatibility and testing purposes.
impl SkiaCpuCtx {
    pub fn new(width: usize, height: usize) -> Self {
        Self { width, height }
    }
}

impl SkiaBackend for SkiaCpuCtx {
    fn create_skia_surface(&self) -> FFramesRendererResult<(Surface, Option<DirectContext>)> {
        let surface = surfaces::raster_n32_premul((self.width as i32, self.height as i32))
            .ok_or_else(|| {
                fframes::FFramesRendererError::Custom("Failed to create skia surface".to_string())
            })?;

        Ok((surface, None))
    }
}

impl<'a> SkiaFFramesRenderer<'a, SkiaCpuCtx> {
    /// Creates new CPU based skia renderer. It is not recommended to use it for the final
    /// video rendering, if you are rendering a video on a target without GPU consider
    /// using a built-in CPU renderer.
    ///
    /// Intended for compatibility layer with GPU renderer and/or testing.
    pub fn new_cpu(
        ctx: &'a SkiaCpuCtx,
        pipeline_config: crate::SkiaPipelineConfig,
    ) -> FFramesRendererResult<Self> {
        Ok(Self::new(pipeline_config, ctx))
    }
}
