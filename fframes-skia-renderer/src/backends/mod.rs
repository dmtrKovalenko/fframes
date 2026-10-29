#[cfg(feature = "metal")]
pub mod metal;
#[cfg(feature = "vulkan")]
pub mod vulkan;
#[cfg(feature = "vulkan")]
pub(crate) mod vulkan_readback;
#[cfg(feature = "wgpu")]
mod vulkan_wgpu;

use fframes::{EncoderInput, FFramesRendererError, FFramesRendererResult, VideoEncoderInfo};
use skia_safe::{Surface, gpu::DirectContext, surfaces};

use crate::{HardwareFrameTarget, SkiaFFramesRenderer};

/// The lock of the GPU queue a context submits to, see [`SkiaContext::queue_lock`].
pub type QueueLock = std::sync::Arc<std::sync::Mutex<()>>;

/// Holds `lock` (when there is one) until the guard is dropped.
pub fn lock_queue(lock: Option<&QueueLock>) -> Option<std::sync::MutexGuard<'_, ()>> {
    lock.map(|lock| {
        lock.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    })
}

/// Reads the pixels of a GPU surface without keeping the queue of its context busy, see
/// [`SkiaContext::reader`].
pub trait SurfaceReader {
    /// Submits everything drawn with `gpu` followed by a copy of `surface`, waits for the
    /// copy and writes it to `pixels`: rows of `width * 4` bytes without padding, in the
    /// byte order of the surface (which has to be RGBA or BGRA 8888).
    ///
    /// The queue is only held while submitting, not while the GPU works.
    fn read(
        &mut self,
        gpu: &mut DirectContext,
        surface: &mut Surface,
        pixels: &mut [u8],
    ) -> FFramesRendererResult<()>;
}

/// A GPU context of a backend with a surface to draw into.
pub struct SkiaContext {
    pub surface: Surface,
    /// `None` for backends that render on the CPU.
    pub gpu: Option<DirectContext>,
    /// A Vulkan queue must not be used from two threads at once, and a device can have
    /// fewer queues than a render has GPU contexts (integrated GPUs expose a single one).
    /// Contexts that share a queue share this lock; whoever drives a context holds it
    /// around every call that reaches the queue: a flush that submits, a pixel readback,
    /// dropping the context. `None` when the backend needs no such lock.
    pub queue_lock: Option<QueueLock>,
    /// The fast way to read surfaces of this context back when the queue is shared.
    /// Without it Skia's own readback is used under [`Self::queue_lock`].
    pub reader: Option<Box<dyn SurfaceReader>>,
}

pub trait SkiaBackend: Sync + Send {
    /// The [`fframes::SharedGpu::id`] of the device this backend's contexts draw on, when
    /// the backend shares it with wgpu: layers rendered on it are sampled in place.
    fn shared_gpu_id(&self) -> Option<u64> {
        None
    }

    fn create_skia_surface(&self) -> FFramesRendererResult<(Surface, Option<DirectContext>)>;

    /// Like [`Self::create_skia_surface`], with what the context needs to share its GPU
    /// queue with other contexts of the backend.
    fn create_skia_context(&self) -> FFramesRendererResult<SkiaContext> {
        let (surface, gpu) = self.create_skia_surface()?;
        Ok(SkiaContext {
            surface,
            gpu,
            queue_lock: None,
            reader: None,
        })
    }

    /// A GPU context without a surface, and the lock of its queue. Code that draws into
    /// surfaces of its own size uses it (`FFramesContext::render_svgr`). The default makes
    /// a whole context with [`Self::create_skia_context`] and drops the surface.
    fn create_offscreen_context(
        &self,
    ) -> FFramesRendererResult<(Option<DirectContext>, Option<QueueLock>)> {
        let context = self.create_skia_context()?;
        Ok((context.gpu, context.queue_lock))
    }

    /// Offers hardware frames to the video encoder of a render: textures Skia draws into and
    /// the encoder reads, so the pixels never leave the GPU. Called once per render before
    /// the encoders are opened.
    ///
    /// `None` (the default) when the backend has no such frames for this encoder; the
    /// frames are converted to its pixel format and read back then.
    fn negotiate_hardware_frames(&self, _encoder: &VideoEncoderInfo<'_>) -> Option<EncoderInput> {
        None
    }

    /// A new GPU context together with the frames negotiated by
    /// [`Self::negotiate_hardware_frames`] that are rendered on it. Called on every GPU
    /// thread of a render.
    fn hardware_frame_target(
        &self,
        _input: &EncoderInput,
        _width: i32,
        _height: i32,
    ) -> FFramesRendererResult<(DirectContext, Box<dyn HardwareFrameTarget>)> {
        Err(FFramesRendererError::Skia(
            "this Skia backend has no hardware frames".to_owned(),
        ))
    }
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
