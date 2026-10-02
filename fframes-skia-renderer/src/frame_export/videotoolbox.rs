//! Hardware frames for `VideoToolbox` encoders (`h264_videotoolbox`, `hevc_videotoolbox`).
//!
//! The encoder takes `CVPixelBuffer`s. A pixel buffer that is backed by an `IOSurface` is
//! also a Metal texture, so Skia draws every frame straight into the BGRA pixel buffer the
//! encoder is handed afterwards:
//!
//! ```text
//! CVPixelBuffer (BGRA, IOSurface) ◄── MTLTexture ◄── Skia surface ◄── frame
//!        └──────────────────────────► VideoToolbox encoder
//! ```
//!
//! Nothing is read back or converted by us. `VideoToolbox` converts to the YUV format of
//! the codec on its side, with the color matrix the stream is tagged with.
//!
//! The pixel buffers come from a `CVPixelBufferPool` of our own rather than from libav's
//! frame pool: the encoder keeps reading a buffer after it took the frame, and only a
//! `CVPixelBufferPool` knows that such a buffer must not be handed out again yet.
use crate::HardwareFrameTarget;
use crate::metal::SkiaMetalCtx;
use core_foundation::base::{CFAllocatorRef, CFRelease, CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use fframes::ffmpeg_sys_fframes::{AVHWDeviceType, AVPixelFormat, av_buffer_create};
use fframes::{
    AvBuffer, EncoderInput, FFramesRendererError, FFramesRendererResult, VideoEncoderInfo,
    VideoFrame,
};
use skia_safe::gpu::{self, DirectContext, SurfaceOrigin, backend_render_targets, mtl};
use skia_safe::{ColorType, Surface};
use std::ffi::c_void;

type CVPixelBufferPoolRef = *mut c_void;
type CVPixelBufferRef = *mut c_void;
type CVMetalTextureCacheRef = *mut c_void;
type CVMetalTextureRef = *mut c_void;

/// `kCVPixelFormatType_32BGRA` (`'BGRA'`)
const CV_PIXEL_FORMAT_32_BGRA: i32 = 0x4247_5241;
/// `MTLPixelFormatBGRA8Unorm`
const MTL_PIXEL_FORMAT_BGRA8_UNORM: usize = 80;
/// `MTLTextureUsageShaderRead | MTLTextureUsageRenderTarget`
const MTL_TEXTURE_USAGE_RENDER_TARGET: i64 = 0x1 | 0x4;

#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    static kCVPixelBufferPixelFormatTypeKey: CFStringRef;
    static kCVPixelBufferWidthKey: CFStringRef;
    static kCVPixelBufferHeightKey: CFStringRef;
    static kCVPixelBufferIOSurfacePropertiesKey: CFStringRef;
    static kCVPixelBufferMetalCompatibilityKey: CFStringRef;
    static kCVMetalTextureUsage: CFStringRef;
    static kCVImageBufferYCbCrMatrixKey: CFStringRef;
    static kCVImageBufferYCbCrMatrix_ITU_R_601_4: CFStringRef;

    fn CVBufferSetAttachment(
        buffer: CVPixelBufferRef,
        key: CFStringRef,
        value: core_foundation::base::CFTypeRef,
        attachment_mode: u32,
    );

    fn CVPixelBufferPoolCreate(
        allocator: CFAllocatorRef,
        pool_attributes: CFDictionaryRef,
        pixel_buffer_attributes: CFDictionaryRef,
        pool_out: *mut CVPixelBufferPoolRef,
    ) -> i32;

    fn CVPixelBufferPoolCreatePixelBuffer(
        allocator: CFAllocatorRef,
        pool: CVPixelBufferPoolRef,
        pixel_buffer_out: *mut CVPixelBufferRef,
    ) -> i32;

    fn CVPixelBufferRelease(pixel_buffer: CVPixelBufferRef);

    fn CVMetalTextureCacheCreate(
        allocator: CFAllocatorRef,
        cache_attributes: CFDictionaryRef,
        metal_device: *mut c_void,
        texture_attributes: CFDictionaryRef,
        cache_out: *mut CVMetalTextureCacheRef,
    ) -> i32;

    fn CVMetalTextureCacheCreateTextureFromImage(
        allocator: CFAllocatorRef,
        texture_cache: CVMetalTextureCacheRef,
        source_image: CVPixelBufferRef,
        texture_attributes: CFDictionaryRef,
        pixel_format: usize,
        width: usize,
        height: usize,
        plane_index: usize,
        texture_out: *mut CVMetalTextureRef,
    ) -> i32;

    fn CVMetalTextureGetTexture(texture: CVMetalTextureRef) -> *mut c_void;

    fn CVMetalTextureCacheFlush(texture_cache: CVMetalTextureCacheRef, options: u64);
}

fn skia_error(message: impl Into<String>) -> FFramesRendererError {
    FFramesRendererError::Skia(message.into())
}

/// Offers BGRA pixel buffers to encoders that take `VideoToolbox` frames.
pub(crate) fn negotiate(
    backend: &SkiaMetalCtx,
    encoder: &VideoEncoderInfo<'_>,
) -> Option<EncoderInput> {
    // ProRes derives its profile from the pixel format: BGRA would make it 4444.
    if !matches!(encoder.name(), "h264_videotoolbox" | "hevc_videotoolbox")
        || !super::accepts_hardware_frames(encoder, AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX)
    {
        return None;
    }

    // Fails when FFmpeg was built without VideoToolbox (the `videotoolbox` feature).
    let device = fframes::hardware_device(AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX).ok()?;
    // Tells the encoder what the pixel buffers hold. The buffers themselves come from
    // the pool of the frame target.
    let input = EncoderInput::hardware_frames(
        &device,
        AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX,
        AVPixelFormat::AV_PIX_FMT_BGRA,
        (encoder.width, encoder.height),
        |_| {},
    )
    .ok()?;

    // Draw a frame the way the render will. When Metal can not render into the pixel
    // buffers on this machine the frames take another way.
    let (gpu, target) =
        VideoToolboxFrameTarget::new(backend, &input, encoder.width, encoder.height).ok()?;
    super::trial_frame(Box::new(target), gpu).ok()?;

    Some(input)
}

/// Releases the pixel buffer of a frame once libav and the encoder are done with it.
unsafe extern "C" fn release_pixel_buffer(_opaque: *mut c_void, data: *mut u8) {
    unsafe { CVPixelBufferRelease(data.cast()) }
}

/// The Metal texture that views a pixel buffer.
struct MetalTexture(CVMetalTextureRef);

impl Drop for MetalTexture {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0.cast_const()) }
    }
}

/// The pixel buffer a frame is being drawn into.
struct CurrentFrame {
    // dropped before the texture it wraps
    surface: Surface,
    texture: MetalTexture,
    frame: VideoFrame,
}

/// Renders frames into pixel buffers the encoder takes, see the module docs.
pub(crate) struct VideoToolboxFrameTarget {
    current: Option<CurrentFrame>,
    pixel_buffers: CVPixelBufferPoolRef,
    texture_cache: CVMetalTextureCacheRef,
    frames: AvBuffer,
    width: i32,
    height: i32,
}

impl VideoToolboxFrameTarget {
    pub(crate) fn new(
        backend: &SkiaMetalCtx,
        input: &EncoderInput,
        width: i32,
        height: i32,
    ) -> FFramesRendererResult<(DirectContext, Self)> {
        let Some(frames) = &input.hw_frames_ctx else {
            return Err(skia_error(
                "VideoToolbox frames need a hardware frames context",
            ));
        };
        if input.software_format() != AVPixelFormat::AV_PIX_FMT_BGRA {
            return Err(skia_error("only BGRA VideoToolbox frames are supported"));
        }

        let gpu = backend.create_context()?;
        let key = |key: CFStringRef| unsafe { CFString::wrap_under_get_rule(key) };

        // IOSurface backed buffers Metal can render into.
        let buffer_attributes = unsafe {
            CFDictionary::from_CFType_pairs(&[
                (
                    key(kCVPixelBufferPixelFormatTypeKey),
                    CFNumber::from(CV_PIXEL_FORMAT_32_BGRA).as_CFType(),
                ),
                (
                    key(kCVPixelBufferWidthKey),
                    CFNumber::from(width).as_CFType(),
                ),
                (
                    key(kCVPixelBufferHeightKey),
                    CFNumber::from(height).as_CFType(),
                ),
                (
                    key(kCVPixelBufferIOSurfacePropertiesKey),
                    CFDictionary::<CFString, CFType>::from_CFType_pairs(&[]).as_CFType(),
                ),
                (
                    key(kCVPixelBufferMetalCompatibilityKey),
                    CFBoolean::true_value().as_CFType(),
                ),
            ])
        };
        let mut pixel_buffers = std::ptr::null_mut();
        let status = unsafe {
            CVPixelBufferPoolCreate(
                std::ptr::null(),
                std::ptr::null(),
                buffer_attributes.as_concrete_TypeRef(),
                &raw mut pixel_buffers,
            )
        };
        if status != 0 || pixel_buffers.is_null() {
            return Err(skia_error(format!(
                "can not create a pixel buffer pool (CVReturn {status})"
            )));
        }

        // Textures of the cache are only readable by default.
        let texture_attributes = unsafe {
            CFDictionary::from_CFType_pairs(&[(
                key(kCVMetalTextureUsage),
                CFNumber::from(MTL_TEXTURE_USAGE_RENDER_TARGET),
            )])
        };
        let mut texture_cache = std::ptr::null_mut();
        let status = unsafe {
            CVMetalTextureCacheCreate(
                std::ptr::null(),
                std::ptr::null(),
                backend.device_ptr(),
                texture_attributes.as_concrete_TypeRef(),
                &raw mut texture_cache,
            )
        };
        if status != 0 || texture_cache.is_null() {
            unsafe { CFRelease(pixel_buffers.cast_const()) };
            return Err(skia_error(format!(
                "can not create a Metal texture cache (CVReturn {status})"
            )));
        }

        Ok((
            gpu,
            Self {
                current: None,
                pixel_buffers,
                texture_cache,
                frames: frames.clone(),
                width,
                height,
            },
        ))
    }

    /// A frame with a pixel buffer nobody uses: the pool does not recycle buffers the
    /// encoder still reads.
    fn new_frame(&self) -> FFramesRendererResult<(VideoFrame, CVPixelBufferRef)> {
        let frame = VideoFrame::empty().map_err(|err| FFramesRendererError::from_chunk(0, err))?;

        unsafe {
            let mut pixel_buffer = std::ptr::null_mut();
            let status = CVPixelBufferPoolCreatePixelBuffer(
                std::ptr::null(),
                self.pixel_buffers,
                &raw mut pixel_buffer,
            );
            if status != 0 || pixel_buffer.is_null() {
                return Err(skia_error(format!(
                    "can not allocate a pixel buffer (CVReturn {status})"
                )));
            }

            // FFmpeg passes hardware pixel buffers through unchanged. The session's
            // YCbCrMatrix property tags the stream, but does not choose the RGB input
            // conversion matrix without this attachment. Match the BT.601 stream tag
            // and the software/GPU YUV converters instead of VideoToolbox's HD default.
            CVBufferSetAttachment(
                pixel_buffer,
                kCVImageBufferYCbCrMatrixKey,
                kCVImageBufferYCbCrMatrix_ITU_R_601_4.cast(),
                1, // kCVAttachmentMode_ShouldPropagate
            );

            // What libav's own VideoToolbox frames look like: the pixel buffer in
            // `data[3]`, kept alive by `buf[0]`.
            let buffer = av_buffer_create(
                pixel_buffer.cast(),
                1,
                Some(release_pixel_buffer),
                std::ptr::null_mut(),
                0,
            );
            if buffer.is_null() {
                CVPixelBufferRelease(pixel_buffer);
                return Err(skia_error("can not allocate a frame buffer"));
            }

            let raw = &mut *frame.as_ptr();
            raw.buf[0] = buffer;
            raw.data[3] = pixel_buffer.cast();
            raw.format = AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX as i32;
            raw.width = self.width;
            raw.height = self.height;
            raw.hw_frames_ctx = self.frames.new_ref();

            Ok((frame, pixel_buffer))
        }
    }
}

impl VideoToolboxFrameTarget {
    /// Gives up a frame that was begun but never finished. What Skia recorded for it is
    /// executed first, the texture of the frame can not be released before that.
    fn abandon_frame(&mut self, gpu: &mut DirectContext) {
        if self.current.is_some() {
            gpu.flush_submit_and_sync_cpu();
            self.current = None;
        }
    }
}

impl HardwareFrameTarget for VideoToolboxFrameTarget {
    fn begin_frame(&mut self, gpu: &mut DirectContext) -> FFramesRendererResult<&mut Surface> {
        self.abandon_frame(gpu);
        let (frame, pixel_buffer) = self.new_frame()?;

        let (texture, surface) = unsafe {
            let mut texture = std::ptr::null_mut();
            let status = CVMetalTextureCacheCreateTextureFromImage(
                std::ptr::null(),
                self.texture_cache,
                pixel_buffer,
                std::ptr::null(),
                MTL_PIXEL_FORMAT_BGRA8_UNORM,
                self.width as usize,
                self.height as usize,
                0,
                &raw mut texture,
            );
            if status != 0 || texture.is_null() {
                return Err(skia_error(format!(
                    "can not map a pixel buffer into Metal (CVReturn {status})"
                )));
            }
            let texture = MetalTexture(texture);

            let texture_info =
                mtl::TextureInfo::new(CVMetalTextureGetTexture(texture.0).cast_const());
            let render_target =
                backend_render_targets::make_mtl((self.width, self.height), &texture_info);
            let surface = gpu::surfaces::wrap_backend_render_target(
                gpu,
                &render_target,
                SurfaceOrigin::TopLeft,
                ColorType::BGRA8888,
                None,
                None,
            )
            .ok_or_else(|| skia_error("can not render into a pixel buffer"))?;

            (texture, surface)
        };

        let current = self.current.insert(CurrentFrame {
            surface,
            texture,
            frame,
        });
        Ok(&mut current.surface)
    }

    fn retire(self: Box<Self>, gpu: DirectContext) {
        self.retire_with(gpu);
    }

    fn finish_frame(&mut self, gpu: &mut DirectContext) -> FFramesRendererResult<VideoFrame> {
        let CurrentFrame {
            surface,
            texture,
            frame,
        } = self
            .current
            .take()
            .ok_or_else(|| skia_error("finish_frame without begin_frame"))?;

        // The encoder reads the pixel buffer as soon as it gets the frame, the GPU has to
        // be done with it by then.
        gpu.flush_submit_and_sync_cpu();

        drop(surface);
        drop(texture);
        unsafe { CVMetalTextureCacheFlush(self.texture_cache, 0) };

        Ok(frame)
    }
}

impl VideoToolboxFrameTarget {
    /// See [`HardwareFrameTarget::retire`].
    fn retire_with(mut self: Box<Self>, mut gpu: DirectContext) {
        self.abandon_frame(&mut gpu);
        drop(self);
        drop(gpu);
    }
}

impl Drop for VideoToolboxFrameTarget {
    fn drop(&mut self) {
        self.current = None;
        unsafe {
            CFRelease(self.texture_cache.cast_const());
            CFRelease(self.pixel_buffers.cast_const());
        }
    }
}
