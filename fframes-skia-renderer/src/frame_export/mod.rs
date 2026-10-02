use crate::{QueueLock, SkiaBackend, SkiaContext, SurfaceReader, lock_queue};
use fframes::{
    Color, EncoderFrameRenderer, EncoderInput, FFramesRendererError, FFramesRendererResult,
    RenderEncodingResult, RgbaFrameConverter, VideoEncoderInfo, VideoFrame, usvgr,
};
use skia_safe::{AlphaType, ColorType, ImageInfo, Surface, gpu};

mod planes;
#[cfg(all(target_os = "macos", feature = "metal"))]
pub(crate) mod videotoolbox;
#[cfg(feature = "vulkan-video")]
pub(crate) mod vulkan_frames;

/// How the frames Skia renders are handed to the video encoder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SkiaFrameExport {
    /// The fastest way the backend and the encoder support: hardware frames, then
    /// conversion on the GPU, then conversion on the CPU.
    #[default]
    Auto,
    /// Convert to the encoder's pixel format on the GPU and read the planes back, never
    /// hardware frames. Formats without a GPU conversion are converted on the CPU.
    GpuConversion,
    /// Read RGBA back and convert it on the CPU.
    CpuConversion,
}

/// The way frames of a render take, see [`SkiaEncoderFrameRenderer::path`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameExportPath {
    /// The encoder reads the texture Skia rendered.
    HardwareFrames,
    /// Converted on the GPU, planes read back.
    GpuConversion,
    /// RGBA read back and converted on the CPU.
    CpuConversion,
}

/// Hardware frames of one GPU context: the textures the encoder reads.
/// Created by [`SkiaBackend::hardware_frame_target`] together with the context.
pub trait HardwareFrameTarget {
    /// The surface the next frame has to be drawn into.
    fn begin_frame(&mut self, gpu: &mut gpu::DirectContext) -> FFramesRendererResult<&mut Surface>;

    /// Submits what was drawn since [`Self::begin_frame`] and returns it as a frame of the
    /// encoder.
    fn finish_frame(&mut self, gpu: &mut gpu::DirectContext) -> FFramesRendererResult<VideoFrame>;

    /// Ends the target and then the GPU context it was created with, in that order: the
    /// surfaces of the target live on the context. Destroying a context submits to its
    /// queue, so a target that shares the queue with the encoder does it under the lock
    /// of the queue.
    fn retire(self: Box<Self>, gpu: gpu::DirectContext) {
        drop(self);
        drop(gpu);
    }
}

/// `true` when the pixel format requested for the video is what hardware frames of
/// `hardware_format` deliver: 8 bit 4:2:0 without alpha (the default), or the hardware
/// format itself. A request for 10 bit, 4:4:4 or alpha describes the stream the user wants,
/// and hardware frames would encode 8 bit 4:2:0 in its place.
#[allow(dead_code)] // only backends with hardware frames ask
pub(crate) fn accepts_hardware_frames(
    encoder: &VideoEncoderInfo<'_>,
    hardware_format: fframes::ffmpeg_sys_fframes::AVPixelFormat,
) -> bool {
    use fframes::ffmpeg_sys_fframes::AVPixelFormat::{AV_PIX_FMT_NV12, AV_PIX_FMT_YUV420P};

    let requested = encoder.requested_pixel_format();
    encoder.lists(hardware_format)
        && (requested == hardware_format
            || requested == AV_PIX_FMT_YUV420P
            || requested == AV_PIX_FMT_NV12)
}

/// Draws an empty frame through `target`, the whole way a frame of the render takes, and
/// ends the target and its context.
#[allow(dead_code)] // only backends with hardware frames try them out
pub(crate) fn trial_frame(
    mut target: Box<dyn HardwareFrameTarget>,
    mut gpu: gpu::DirectContext,
) -> FFramesRendererResult<()> {
    let frame = target
        .begin_frame(&mut gpu)
        .map(|surface| {
            surface.canvas().clear(skia_safe::Color::BLACK);
        })
        .and_then(|()| target.finish_frame(&mut gpu));
    target.retire(gpu);

    frame.map(drop)
}

pub(crate) fn negotiate<TBackend: SkiaBackend + ?Sized>(
    backend: &TBackend,
    mode: SkiaFrameExport,
    encoder: &VideoEncoderInfo<'_>,
) -> RenderEncodingResult<EncoderInput> {
    let mut hardware_error = None;
    if mode == SkiaFrameExport::Auto
        && let Some(input) = backend.negotiate_hardware_frames(encoder)
    {
        match encoder.try_open(&input) {
            Ok(()) => return Ok(input),
            Err(err) => hardware_error = Some(err),
        }
    }

    // An encoder that takes nothing but hardware frames fails here too. The error from
    // its hardware frames says why.
    EncoderInput::requested(encoder).map_err(|err| hardware_error.unwrap_or(err))
}

/// Writes the uniforms of a runtime effect by name, wherever its layout puts them.
pub(crate) fn pack_uniforms(
    effect: &skia_safe::RuntimeEffect,
    values: &[(&str, &[f32])],
) -> FFramesRendererResult<skia_safe::Data> {
    let mut data = vec![0_u8; effect.uniform_size()];
    for (name, value) in values {
        let uniform = effect
            .uniforms()
            .iter()
            .find(|uniform| uniform.name() == *name)
            .filter(|uniform| uniform.size_in_bytes() == size_of_val(*value))
            .ok_or_else(|| {
                FFramesRendererError::Skia(format!("conversion shader has no uniform `{name}`"))
            })?;

        let bytes: Vec<u8> = value.iter().flat_map(|v| v.to_ne_bytes()).collect();
        data[uniform.offset()..uniform.offset() + bytes.len()].copy_from_slice(&bytes);
    }

    Ok(skia_safe::Data::new_copy(&data))
}

/// The queue side of a GPU context: how its surfaces are read back.
struct ContextQueue {
    /// Held around everything that submits to the queue of the context.
    lock: Option<QueueLock>,
    reader: Option<Box<dyn SurfaceReader>>,
}

impl ContextQueue {
    /// Submits what was drawn and reads `surface` back into `pixels` as rows of `info`
    /// without padding.
    fn read(
        &mut self,
        gpu: Option<&mut gpu::DirectContext>,
        surface: &mut Surface,
        info: &ImageInfo,
        pixels: &mut [u8],
    ) -> FFramesRendererResult<()> {
        // A reader copies the bytes of the surface as they are.
        if let (Some(reader), Some(gpu)) = (self.reader.as_mut(), gpu)
            && surface.image_info().color_type() == info.color_type()
        {
            return reader.read(gpu, surface, pixels);
        }

        // Skia submits and waits for the GPU in one call, the queue is busy for all of it.
        let _queue = lock_queue(self.lock.as_ref());
        if surface.read_pixels(info, pixels, info.min_row_bytes(), (0, 0)) {
            Ok(())
        } else {
            Err(FFramesRendererError::Skia(
                "failed to read pixels from the Skia surface".to_owned(),
            ))
        }
    }
}

enum Target {
    Hardware(Box<dyn HardwareFrameTarget>),
    Planes {
        surface: Surface,
        exporter: planes::PlaneExporter,
    },
    Rgba {
        surface: Surface,
        info: ImageInfo,
    },
}

/// A frame as it leaves the GPU thread.
pub(crate) enum Rendered {
    Frame(VideoFrame),
    /// Premultiplied RGBA that still has to be converted, which the encoder threads do.
    Rgba(Vec<u8>),
}

/// Skia as a `fframes::EncoderFrameRenderer`: draws frames on one GPU context and returns
/// them the way [`fframes::FFramesRenderBackend::negotiate_encoder_input`] negotiated.
/// Keeps the surfaces, GPU context and render cache between frames.
pub struct SkiaEncoderFrameRenderer<'b> {
    /// Only taken when the renderer is dropped.
    target: Option<Target>,
    render_cache: crate::render::RenderCache,
    /// Only used when RGBA is converted on the calling thread.
    converter: Option<RgbaFrameConverter>,
    input: EncoderInput,
    width: i32,
    height: i32,
    // dropped after the surfaces that were created on it
    gpu: Option<gpu::DirectContext>,
    /// Hardware targets deal with their queue themselves.
    queue: ContextQueue,
    /// The GPU context lives on the device of the backend.
    backend: std::marker::PhantomData<&'b ()>,
}

impl<'b> SkiaEncoderFrameRenderer<'b> {
    pub fn new<TBackend: SkiaBackend + ?Sized>(
        backend: &'b TBackend,
        mode: SkiaFrameExport,
        input: &EncoderInput,
        width: u32,
        height: u32,
    ) -> FFramesRendererResult<Self> {
        let (width, height) = (width as i32, height as i32);

        let (target, gpu, queue) = if input.is_hardware() {
            // hardware frames bring their own surfaces
            let (gpu, target) = backend.hardware_frame_target(input, width, height)?;
            let queue = ContextQueue {
                lock: None,
                reader: None,
            };
            (Target::Hardware(target), Some(gpu), queue)
        } else {
            // Scaled renders (`scale_resolution`) need a surface of the output size, not
            // the one the backend was created with.
            let SkiaContext {
                surface,
                mut gpu,
                queue_lock,
                reader,
            } = crate::context_with_size(backend, width, height)?;
            let exporter = match gpu.as_mut() {
                Some(gpu) if mode != SkiaFrameExport::CpuConversion => {
                    planes::PlaneExporter::new(gpu, input.pixel_format, width, height)
                        .and_then(Result::ok)
                }
                _ => None,
            };

            let target = match exporter {
                Some(exporter) => Target::Planes { surface, exporter },
                None => Target::Rgba {
                    surface,
                    // Always read back as RGBA regardless of the surface's native (often
                    // BGRA) order.
                    info: ImageInfo::new(
                        (width, height),
                        ColorType::RGBA8888,
                        AlphaType::Premul,
                        None,
                    ),
                },
            };
            let queue = ContextQueue {
                lock: queue_lock,
                reader,
            };
            (target, gpu, queue)
        };

        Ok(Self {
            target: Some(target),
            render_cache: crate::render::RenderCache::new(),
            converter: None,
            input: input.clone(),
            width,
            height,
            gpu,
            queue,
            backend: std::marker::PhantomData,
        })
    }

    /// The way the frames of this renderer take to the encoder.
    pub fn path(&self) -> FrameExportPath {
        match self.target {
            Some(Target::Hardware(_)) => FrameExportPath::HardwareFrames,
            Some(Target::Planes { .. }) => FrameExportPath::GpuConversion,
            Some(Target::Rgba { .. }) | None => FrameExportPath::CpuConversion,
        }
    }

    /// Draws `tree` and takes it off the GPU. `rgba_buffer` provides the buffer of the
    /// given size for frames that leave as RGBA.
    pub(crate) fn render(
        &mut self,
        tree: &usvgr::Tree,
        background: skia_safe::Color,
        rgba_buffer: impl FnOnce(usize) -> Vec<u8>,
    ) -> FFramesRendererResult<Rendered> {
        let (width, height) = (self.width, self.height);
        let render_cache = &mut self.render_cache;
        let mut draw = |surface: &mut Surface| {
            let canvas = surface.canvas();
            canvas.clear(background);
            canvas.save();
            crate::apply_fit(canvas, tree, width, height);
            crate::render::render_tree(tree, canvas, render_cache);
            canvas.restore();
        };

        match self
            .target
            .as_mut()
            .expect("the target is only taken on drop")
        {
            Target::Hardware(target) => {
                let gpu = self
                    .gpu
                    .as_mut()
                    .expect("hardware targets are created on a GPU context");
                draw(target.begin_frame(gpu)?);
                target.finish_frame(gpu).map(Rendered::Frame)
            }
            Target::Planes { surface, exporter } => {
                draw(surface);
                exporter.convert(surface)?;

                let (gpu, queue) = (self.gpu.as_mut(), &mut self.queue);
                exporter
                    .read_back(|planes, info, pixels| queue.read(gpu, planes, info, pixels))
                    .map(Rendered::Frame)
            }
            Target::Rgba { surface, info } => {
                draw(surface);

                let mut pixels = rgba_buffer(info.compute_byte_size(info.min_row_bytes()));
                self.queue
                    .read(self.gpu.as_mut(), surface, info, &mut pixels)?;
                Ok(Rendered::Rgba(pixels))
            }
        }
    }
}

impl Drop for SkiaEncoderFrameRenderer<'_> {
    fn drop(&mut self) {
        // Everything that lives on the GPU context goes before the context.
        self.render_cache = crate::render::RenderCache::new();
        let target = self.target.take();

        match (target, self.gpu.take()) {
            (Some(Target::Hardware(target)), Some(gpu)) => target.retire(gpu),
            (target, gpu) => {
                drop(target);
                self.queue.reader = None;
                // destroying a context waits for its queue
                let _queue = lock_queue(self.queue.lock.as_ref());
                drop(gpu);
            }
        }
    }
}

impl EncoderFrameRenderer for SkiaEncoderFrameRenderer<'_> {
    fn render_tree(
        &mut self,
        tree: &usvgr::Tree,
        background: Color,
    ) -> FFramesRendererResult<VideoFrame> {
        let background =
            skia_safe::Color::from_argb(background.a, background.r, background.g, background.b);

        match self.render(tree, background, |size| vec![0; size])? {
            Rendered::Frame(frame) => Ok(frame),
            Rendered::Rgba(pixels) => {
                let encoding_error = |err| FFramesRendererError::from_chunk(0, err);
                if self.converter.is_none() {
                    self.converter = Some(
                        RgbaFrameConverter::for_input(&self.input, self.width, self.height)
                            .map_err(encoding_error)?,
                    );
                }

                self.converter
                    .as_mut()
                    .expect("the converter was just created")
                    .convert(&pixels)
                    .map_err(encoding_error)
            }
        }
    }
}
