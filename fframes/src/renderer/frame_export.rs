use super::encoder::EncoderOptions;
use super::pix_fmt::YuvMatrix;
use super::renderer_error::{
    FFramesRendererError, FFramesRendererResult, RenderEncodingError, RenderEncodingResult,
};
use super::{FrameRenderer, encoder::av_error_to_string, stream};
use crate::media::ffmpeg_sys_fframes::SwsFlags::SWS_BICUBIC;
use crate::media::ffmpeg_sys_fframes::*;
use crate::{Color, usvgr};
use std::ffi::{CStr, CString};
use std::path::Path;
use std::ptr::NonNull;

fn ffmpeg_error(code: i32) -> RenderEncodingError {
    RenderEncodingError::FFmpegError(code, av_error_to_string(code))
}

/// A counted reference to a libav buffer (`AVBufferRef`). Hardware device and frames contexts
/// are passed around this way.
pub struct AvBuffer(NonNull<AVBufferRef>);

unsafe impl Send for AvBuffer {}
unsafe impl Sync for AvBuffer {}

impl AvBuffer {
    /// Takes ownership of `buffer`, `None` when it is null.
    ///
    /// # Safety
    /// `buffer` must be a valid reference that nothing else unreferences.
    pub unsafe fn from_raw(buffer: *mut AVBufferRef) -> Option<Self> {
        NonNull::new(buffer).map(Self)
    }

    pub fn as_ptr(&self) -> *mut AVBufferRef {
        self.0.as_ptr()
    }

    /// The payload of the buffer, e.g. the `AVHWFramesContext` of a frames context.
    pub fn data<T>(&self) -> *mut T {
        unsafe { (*self.0.as_ptr()).data.cast() }
    }

    /// A new reference for a libav struct that unreferences it on its own.
    pub fn new_ref(&self) -> *mut AVBufferRef {
        unsafe { av_buffer_ref(self.0.as_ptr()) }
    }
}

impl Clone for AvBuffer {
    fn clone(&self) -> Self {
        Self(NonNull::new(self.new_ref()).expect("av_buffer_ref: out of memory"))
    }
}

impl Drop for AvBuffer {
    fn drop(&mut self) {
        unsafe { av_buffer_unref(&mut self.0.as_ptr()) }
    }
}

impl std::fmt::Debug for AvBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AvBuffer({:p})", self.0.as_ptr())
    }
}

/// A frame on its way to the video encoder: an owned libav `AVFrame`.
///
/// It either carries pixels in the software format of the encoder or references a hardware
/// surface (a `CVPixelBuffer`, a Vulkan image), in which case the pixels never leave the GPU.
pub struct VideoFrame(NonNull<AVFrame>);

unsafe impl Send for VideoFrame {}

impl VideoFrame {
    /// A frame without buffers (`av_frame_alloc`).
    pub fn empty() -> RenderEncodingResult<Self> {
        NonNull::new(unsafe { av_frame_alloc() })
            .map(Self)
            .ok_or_else(|| RenderEncodingError::CantAllocate("video frame".to_owned()))
    }

    /// Takes ownership of `frame`, `None` when it is null.
    ///
    /// # Safety
    /// `frame` must come from `av_frame_alloc` and must not be freed by anything else.
    pub unsafe fn from_raw(frame: *mut AVFrame) -> Option<Self> {
        NonNull::new(frame).map(Self)
    }

    /// A frame of a hardware frames pool (`av_hwframe_get_buffer`). What its `data` points
    /// to depends on the hardware format, e.g. `data[3]` is the `CVPixelBufferRef` of a
    /// `VideoToolbox` frame and `data[0]` the `AVVkFrame` of a Vulkan one.
    pub fn from_hw_frames(frames: &AvBuffer) -> RenderEncodingResult<Self> {
        let frame = Self::empty()?;
        let status = unsafe { av_hwframe_get_buffer(frames.as_ptr(), frame.as_ptr(), 0) };
        if status < 0 {
            return Err(ffmpeg_error(status));
        }
        Ok(frame)
    }

    /// Copies a hardware frame into memory (`av_hwframe_transfer_data`), in the first
    /// software format its frames context transfers to. The encoder never needs this; it
    /// is the way to look at what a hardware frame holds.
    pub fn download(&self) -> RenderEncodingResult<Self> {
        let frame = Self::empty()?;
        let status = unsafe { av_hwframe_transfer_data(frame.as_ptr(), self.as_ptr(), 0) };
        if status < 0 {
            return Err(ffmpeg_error(status));
        }
        Ok(frame)
    }

    pub fn as_ptr(&self) -> *mut AVFrame {
        self.0.as_ptr()
    }

    /// Hands the frame over to the caller, who has to `av_frame_free` it.
    pub fn into_raw(self) -> *mut AVFrame {
        let frame = self.0.as_ptr();
        std::mem::forget(self);
        frame
    }

    pub fn width(&self) -> i32 {
        unsafe { (*self.as_ptr()).width }
    }

    pub fn height(&self) -> i32 {
        unsafe { (*self.as_ptr()).height }
    }

    pub fn pixel_format(&self) -> AVPixelFormat {
        // set from an `AVPixelFormat` by whoever created the frame
        unsafe { std::mem::transmute::<i32, AVPixelFormat>((*self.as_ptr()).format) }
    }

    /// Start and line size of a plane, a null pointer for planes the format does not have
    /// (and for the pixel planes of hardware frames).
    pub fn plane(&self, index: usize) -> (*mut u8, i32) {
        unsafe {
            let frame = self.as_ptr();
            ((*frame).data[index], (*frame).linesize[index])
        }
    }

    /// Copies the pixels of a plane of a software frame without the line padding.
    /// `width` and `height` are the size of the plane in bytes and rows.
    pub fn copy_plane(&self, index: usize, width: usize, height: usize) -> Vec<u8> {
        let (data, linesize) = self.plane(index);
        assert!(
            !data.is_null() && linesize as usize >= width,
            "frame has no plane {index} of {width} bytes per row"
        );

        let mut pixels = Vec::with_capacity(width * height);
        for row in 0..height {
            pixels.extend_from_slice(unsafe {
                std::slice::from_raw_parts(data.add(row * linesize as usize), width)
            });
        }
        pixels
    }
}

impl Drop for VideoFrame {
    fn drop(&mut self) {
        unsafe { av_frame_free(&mut self.0.as_ptr()) }
    }
}

impl std::fmt::Debug for VideoFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "VideoFrame({:?} {}x{})",
            self.pixel_format(),
            self.width(),
            self.height()
        )
    }
}

fn pixel_format_flags(format: AVPixelFormat) -> u64 {
    unsafe {
        let descriptor = av_pix_fmt_desc_get(format);
        if descriptor.is_null() {
            0
        } else {
            (*descriptor).flags
        }
    }
}

/// `true` for formats that reference a hardware surface (`AV_PIX_FMT_VIDEOTOOLBOX`,
/// `AV_PIX_FMT_VULKAN`, ...) instead of carrying pixels.
pub fn is_hardware_pixel_format(format: AVPixelFormat) -> bool {
    pixel_format_flags(format) & AV_PIX_FMT_FLAG_HWACCEL as u64 != 0
}

/// The video encoder of a render as a backend sees it while negotiating the frames it will
/// deliver. The codec is resolved the same way `Encoder` does it, nothing is opened yet.
pub struct VideoEncoderInfo<'a> {
    codec: *const AVCodec,
    /// The container wants the codec headers out of band (`AVFMT_GLOBALHEADER`).
    global_header: bool,
    pub width: i32,
    pub height: i32,
    pub fps: i32,
    pub options: &'a EncoderOptions<'a>,
}

impl<'a> VideoEncoderInfo<'a> {
    /// The encoder that will be opened for `output` (only the extension matters).
    pub fn for_output(
        output: &Path,
        (width, height, fps): (i32, i32, i32),
        options: &'a EncoderOptions<'a>,
    ) -> RenderEncodingResult<Self> {
        let c_filename = CString::new(output.to_string_lossy().as_ref())
            .map_err(RenderEncodingError::CStringError)?;

        let (codec, global_header) = unsafe {
            let format = av_guess_format(std::ptr::null(), c_filename.as_ptr(), std::ptr::null());
            if format.is_null() {
                return Err(RenderEncodingError::UnknownExtension(output.to_owned()));
            }

            (
                stream::find_encoder(options.preferred_encoder, (*format).video_codec, false)?,
                (*format).flags & AVFMT_GLOBALHEADER != 0,
            )
        };

        Ok(Self {
            codec,
            global_header,
            width,
            height,
            fps,
            options,
        })
    }

    pub fn as_ptr(&self) -> *const AVCodec {
        self.codec
    }

    /// The libav name of the encoder, e.g. `libx264` or `hevc_videotoolbox`.
    pub fn name(&self) -> &str {
        unsafe {
            let name = (*self.codec).name;
            if name.is_null() {
                ""
            } else {
                CStr::from_ptr(name).to_str().unwrap_or("")
            }
        }
    }

    /// The encoder runs on dedicated hardware (`AV_CODEC_CAP_HARDWARE`), e.g. `h264_videotoolbox`.
    pub fn is_hardware(&self) -> bool {
        unsafe { (*self.codec).capabilities & AV_CODEC_CAP_HARDWARE as i32 != 0 }
    }

    /// `EncoderOptions::pixel_format`: what the user asked the video to be stored in.
    pub fn requested_pixel_format(&self) -> AVPixelFormat {
        self.options.pixel_format
    }

    /// The pixel formats the encoder takes, `None` when it does not tell.
    pub fn pixel_formats(&self) -> Option<&[AVPixelFormat]> {
        unsafe { stream::supported_pixel_formats(self.codec) }
    }

    pub fn supports(&self, format: AVPixelFormat) -> bool {
        self.pixel_formats()
            .is_none_or(|formats| formats.contains(&format))
    }

    /// Opens the encoder for `input` the way the render will and closes it again. This is
    /// how a backend finds out that hardware frames it could deliver are not accepted on
    /// this machine (no such encoder in the driver, unsupported size or format) while it
    /// can still offer something else.
    pub fn try_open(&self, input: &EncoderInput) -> RenderEncodingResult<()> {
        unsafe {
            let mut context = avcodec_alloc_context3(self.codec);
            if context.is_null() {
                return Err(RenderEncodingError::CantAllocate(
                    "encoding context".to_owned(),
                ));
            }

            let result = stream::open_video_encoder(
                context,
                self.codec,
                (self.width, self.height, self.fps),
                self.global_header,
                self.options,
                input,
                1,
            );
            avcodec_free_context(&raw mut context);
            result
        }
    }

    /// The encoder lists `format` explicitly. Unlike [`Self::supports`] this is `false` for
    /// encoders that do not publish their formats, use it for hardware formats.
    pub fn lists(&self, format: AVPixelFormat) -> bool {
        self.pixel_formats()
            .is_some_and(|formats| formats.contains(&format))
    }
}

/// What the video encoders of a render are opened with, the outcome of
/// `FFramesRenderBackend::negotiate_encoder_input`. Every [`VideoFrame`] submitted to the
/// render has to match it.
#[derive(Debug, Clone)]
pub struct EncoderInput {
    /// `AVCodecContext::pix_fmt`: a software format, or a hardware one together with
    /// `hw_frames_ctx`.
    pub pixel_format: AVPixelFormat,
    /// The pool the hardware frames come from (`AVCodecContext::hw_frames_ctx`).
    pub hw_frames_ctx: Option<AvBuffer>,
    /// `AVCodecContext::hw_device_ctx` for encoders that want the device only.
    pub hw_device_ctx: Option<AvBuffer>,
    /// The matrix the frames are converted to YUV with. The encoder tags the stream with it.
    pub color_matrix: YuvMatrix,
}

impl EncoderInput {
    /// Frames with pixels in memory, converted with BT.601.
    pub fn software(pixel_format: AVPixelFormat) -> Self {
        Self {
            pixel_format,
            hw_frames_ctx: None,
            hw_device_ctx: None,
            color_matrix: YuvMatrix::Bt601,
        }
    }

    /// Frames with pixels in memory in the pixel format and color matrix of `options`.
    pub fn for_options(options: &EncoderOptions<'_>) -> Self {
        Self::software(options.pixel_format).with_color_matrix(options.color_matrix)
    }

    /// The same frames converted with `color_matrix`.
    #[must_use]
    pub fn with_color_matrix(self, color_matrix: YuvMatrix) -> Self {
        Self {
            color_matrix,
            ..self
        }
    }

    /// The pixel format the user requested, as long as the encoder takes it.
    pub fn requested(encoder: &VideoEncoderInfo<'_>) -> RenderEncodingResult<Self> {
        let format = encoder.requested_pixel_format();
        if !encoder.supports(format) {
            return Err(RenderEncodingError::InvalidPixFmt(format));
        }
        Ok(Self::for_options(encoder.options))
    }

    /// Hardware frames of `format` holding `sw_format` pixels, allocated by libav on `device`
    /// (see [`hardware_device`]). Get the frames with [`VideoFrame::from_hw_frames`].
    /// They are converted with BT.601 unless [`Self::with_color_matrix`] says otherwise.
    ///
    /// `configure` runs before the pool is initialized and may adjust the API specific
    /// context (`AVHWFramesContext::hwctx`).
    pub fn hardware_frames(
        device: &AvBuffer,
        format: AVPixelFormat,
        sw_format: AVPixelFormat,
        (width, height): (i32, i32),
        configure: impl FnOnce(&mut AVHWFramesContext),
    ) -> RenderEncodingResult<Self> {
        unsafe {
            let frames = AvBuffer::from_raw(av_hwframe_ctx_alloc(device.as_ptr()))
                .ok_or_else(|| RenderEncodingError::CantAllocate("hardware frames".to_owned()))?;

            let context = &mut *frames.data::<AVHWFramesContext>();
            context.format = format;
            context.sw_format = sw_format;
            context.width = width;
            context.height = height;
            configure(context);

            let status = av_hwframe_ctx_init(frames.as_ptr());
            if status < 0 {
                return Err(ffmpeg_error(status));
            }

            Ok(Self {
                pixel_format: format,
                hw_frames_ctx: Some(frames),
                hw_device_ctx: None,
                color_matrix: YuvMatrix::Bt601,
            })
        }
    }

    pub fn is_hardware(&self) -> bool {
        is_hardware_pixel_format(self.pixel_format)
    }

    /// The format of the pixels: `pixel_format` itself, or what the hardware frames hold.
    pub fn software_format(&self) -> AVPixelFormat {
        match &self.hw_frames_ctx {
            Some(frames) => unsafe { (*frames.data::<AVHWFramesContext>()).sw_format },
            None => self.pixel_format,
        }
    }
}

/// Opens a libav hardware device (`av_hwdevice_ctx_create`). Fails when `FFmpeg` was built
/// without it or the machine has no such device.
pub fn hardware_device(device_type: AVHWDeviceType) -> RenderEncodingResult<AvBuffer> {
    unsafe {
        let mut device = std::ptr::null_mut();
        let status = av_hwdevice_ctx_create(
            &raw mut device,
            device_type,
            std::ptr::null(),
            std::ptr::null_mut(),
            0,
        );
        if status < 0 {
            return Err(ffmpeg_error(status));
        }

        AvBuffer::from_raw(device)
            .ok_or_else(|| RenderEncodingError::CantAllocate("hardware device".to_owned()))
    }
}

/// Where the planes of a software frame are inside its pixel buffer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FrameLayout {
    /// Size of the pixel buffer in bytes.
    pub size: usize,
    /// `(offset, linesize)` of every plane the format has.
    pub planes: [Option<(usize, i32)>; 4],
}

impl FrameLayout {
    /// The layout libav uses for frames of `format`: lines padded for SIMD access.
    pub fn of(format: AVPixelFormat, width: i32, height: i32) -> RenderEncodingResult<Self> {
        const ALIGN: i32 = 64;

        unsafe {
            let mut linesizes = [0_i32; 4];
            let status = av_image_fill_linesizes(linesizes.as_mut_ptr(), format, width);
            if status < 0 {
                return Err(ffmpeg_error(status));
            }
            for linesize in &mut linesizes {
                *linesize = (*linesize + ALIGN - 1) & !(ALIGN - 1);
            }

            // encoders may read whole macroblocks
            let padded_height = (height + 31) & !31;
            let wide_linesizes = linesizes.map(|linesize| linesize as isize);
            let mut sizes = [0_usize; 4];
            let status = av_image_fill_plane_sizes(
                sizes.as_mut_ptr(),
                format,
                padded_height,
                wide_linesizes.as_ptr(),
            );
            if status < 0 {
                return Err(ffmpeg_error(status));
            }

            let mut layout = Self::default();
            for (plane, (&size, &linesize)) in sizes.iter().zip(&linesizes).enumerate() {
                if size > 0 {
                    layout.planes[plane] = Some((layout.size, linesize));
                    layout.size += size;
                }
            }
            layout.size += ALIGN as usize;

            Ok(layout)
        }
    }
}

/// Hands out software [`VideoFrame`]s of one format and size. The pixel buffers go back to
/// the pool once the encoder is done with a frame, so a render does not allocate per frame.
pub struct FramePool {
    pool: *mut AVBufferPool,
    format: AVPixelFormat,
    width: i32,
    height: i32,
    layout: FrameLayout,
}

unsafe impl Send for FramePool {}
unsafe impl Sync for FramePool {}

impl FramePool {
    pub fn new(format: AVPixelFormat, width: i32, height: i32) -> RenderEncodingResult<Self> {
        Self::with_layout(
            format,
            width,
            height,
            FrameLayout::of(format, width, height)?,
        )
    }

    /// A pool with a custom plane layout, for sources that produce all planes in one buffer.
    pub fn with_layout(
        format: AVPixelFormat,
        width: i32,
        height: i32,
        layout: FrameLayout,
    ) -> RenderEncodingResult<Self> {
        let pool = unsafe { av_buffer_pool_init(layout.size, None) };
        if pool.is_null() {
            return Err(RenderEncodingError::CantAllocate("frame pool".to_owned()));
        }

        Ok(Self {
            pool,
            format,
            width,
            height,
            layout,
        })
    }

    pub fn layout(&self) -> &FrameLayout {
        &self.layout
    }

    /// A writable frame. Its pixels are whatever the previous user of the buffer left.
    pub fn get(&self) -> RenderEncodingResult<VideoFrame> {
        let frame = VideoFrame::empty()?;

        unsafe {
            let buffer = av_buffer_pool_get(self.pool);
            if buffer.is_null() {
                return Err(RenderEncodingError::CantAllocate("frame buffer".to_owned()));
            }

            let raw = &mut *frame.as_ptr();
            raw.format = self.format as i32;
            raw.width = self.width;
            raw.height = self.height;
            raw.buf[0] = buffer;
            for (plane, placed) in self.layout.planes.iter().enumerate() {
                if let Some((offset, linesize)) = *placed {
                    raw.data[plane] = (*buffer).data.add(offset);
                    raw.linesize[plane] = linesize;
                }
            }
        }

        Ok(frame)
    }
}

impl Drop for FramePool {
    fn drop(&mut self) {
        // buffers that are still referenced by frames keep the pool alive
        unsafe { av_buffer_pool_uninit(&raw mut self.pool) }
    }
}

/// Converts RGBA8 pixels into frames of a software pixel format on the CPU: the built-in
/// SIMD converter for yuv420p, swscale for every other format.
pub struct RgbaFrameConverter {
    pool: FramePool,
    sws: *mut SwsContext,
    matrix: YuvMatrix,
}

unsafe impl Send for RgbaFrameConverter {}

impl RgbaFrameConverter {
    /// Converts with BT.601, see [`Self::with_matrix`].
    pub fn new(format: AVPixelFormat, width: i32, height: i32) -> RenderEncodingResult<Self> {
        Self::with_matrix(format, YuvMatrix::Bt601, width, height)
    }

    /// Converts to YUV formats with `matrix`.
    pub fn with_matrix(
        format: AVPixelFormat,
        matrix: YuvMatrix,
        width: i32,
        height: i32,
    ) -> RenderEncodingResult<Self> {
        if is_hardware_pixel_format(format) {
            return Err(RenderEncodingError::Internal(format!(
                "RGBA pixels can not be converted into {format:?} hardware frames"
            )));
        }

        let pool = FramePool::new(format, width, height)?;
        let sws = if format == AVPixelFormat::AV_PIX_FMT_YUV420P {
            std::ptr::null_mut()
        } else {
            let sws = unsafe {
                sws_getContext(
                    width,
                    height,
                    AVPixelFormat::AV_PIX_FMT_RGBA,
                    width,
                    height,
                    format,
                    SWS_BICUBIC as i32,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            if sws.is_null() {
                return Err(RenderEncodingError::Internal(
                    "Can not allocate sws".to_owned(),
                ));
            }
            if let Err(err) = unsafe { set_sws_output_matrix(sws, matrix) } {
                unsafe { sws_freeContext(sws) };
                return Err(err);
            }
            sws
        };

        Ok(Self { pool, sws, matrix })
    }

    /// For the frames `input` asks for.
    pub fn for_input(input: &EncoderInput, width: i32, height: i32) -> RenderEncodingResult<Self> {
        Self::with_matrix(input.pixel_format, input.color_matrix, width, height)
    }

    /// `rgba` is `width * height * 4` bytes without line padding.
    pub fn convert(&mut self, rgba: &[u8]) -> RenderEncodingResult<VideoFrame> {
        let (width, height) = (self.pool.width, self.pool.height);
        if rgba.len() < width as usize * height as usize * 4 {
            return Err(RenderEncodingError::Internal(format!(
                "{} bytes of RGBA do not make a {width}x{height} frame",
                rgba.len()
            )));
        }

        let frame = self.pool.get()?;
        unsafe {
            let raw = &*frame.as_ptr();
            if self.sws.is_null() {
                super::pix_fmt::fill_yuv420_from_rgba_pixmap_accelerated(
                    self.matrix,
                    width,
                    height,
                    raw.linesize[0],
                    raw.linesize[1],
                    raw.linesize[2],
                    rgba,
                    raw.data[0],
                    raw.data[1],
                    raw.data[2],
                );
            } else {
                let source = [
                    rgba.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null(),
                ];
                let source_linesize = [width * 4, 0, 0, 0];
                sws_scale(
                    self.sws,
                    source.as_ptr(),
                    source_linesize.as_ptr(),
                    0,
                    height,
                    raw.data.as_ptr(),
                    raw.linesize.as_ptr(),
                );
            }
        }

        Ok(frame)
    }
}

/// Makes `sws` convert RGB to YUV with `matrix`. swscale converts with BT.601 unless told
/// otherwise. The ranges stay what swscale chose for the formats.
///
/// # Safety
/// `sws` must be a valid context converting from an RGB format.
pub(crate) unsafe fn set_sws_output_matrix(
    sws: *mut SwsContext,
    matrix: YuvMatrix,
) -> RenderEncodingResult<()> {
    let coefficients = match matrix {
        YuvMatrix::Bt601 => SWS_CS_ITU601,
        YuvMatrix::Bt709 => SWS_CS_ITU709,
    };
    unsafe {
        let (mut input_table, mut output_table) = (std::ptr::null_mut(), std::ptr::null_mut());
        let (mut input_range, mut output_range) = (0, 0);
        let (mut brightness, mut contrast, mut saturation) = (0, 0, 0);
        let status = sws_getColorspaceDetails(
            sws,
            &raw mut input_table,
            &raw mut input_range,
            &raw mut output_table,
            &raw mut output_range,
            &raw mut brightness,
            &raw mut contrast,
            &raw mut saturation,
        );
        if status < 0 {
            return Err(ffmpeg_error(status));
        }
        // For RGB input only the output table (the YUV side) takes part.
        let status = sws_setColorspaceDetails(
            sws,
            input_table,
            input_range,
            sws_getCoefficients(coefficients),
            output_range,
            brightness,
            contrast,
            saturation,
        );
        if status < 0 {
            return Err(ffmpeg_error(status));
        }
    }
    Ok(())
}

impl Drop for RgbaFrameConverter {
    fn drop(&mut self) {
        unsafe { sws_freeContext(self.sws) }
    }
}

/// Rasterizes frames straight into what the video encoder takes. One per rendering thread;
/// implementations keep their surfaces, caches and converters between frames.
///
/// The CPU backend rasterizes into RGBA and converts with [`RgbaFrameConverter`]. A GPU
/// backend can convert on the GPU or return hardware frames that are never read back.
pub trait EncoderFrameRenderer {
    /// Renders `tree` scaled to the output size over `background`.
    fn render_tree(
        &mut self,
        tree: &usvgr::Tree,
        background: Color,
    ) -> FFramesRendererResult<VideoFrame>;
}

/// An [`EncoderFrameRenderer`] for any [`FrameRenderer`]: renders RGBA and converts it on
/// the CPU.
pub struct RgbaEncoderFrameRenderer<R: FrameRenderer> {
    renderer: R,
    converter: RgbaFrameConverter,
    width: u32,
    height: u32,
}

impl<R: FrameRenderer> RgbaEncoderFrameRenderer<R> {
    pub fn new(
        renderer: R,
        input: &EncoderInput,
        width: u32,
        height: u32,
    ) -> RenderEncodingResult<Self> {
        Ok(Self {
            renderer,
            converter: RgbaFrameConverter::for_input(input, width as i32, height as i32)?,
            width,
            height,
        })
    }
}

impl<R: FrameRenderer> EncoderFrameRenderer for RgbaEncoderFrameRenderer<R> {
    fn render_tree(
        &mut self,
        tree: &usvgr::Tree,
        background: Color,
    ) -> FFramesRendererResult<VideoFrame> {
        let frame = self
            .renderer
            .render_tree(tree, background, self.width, self.height)?;

        self.converter
            .convert(&frame.pixels)
            .map_err(|err| FFramesRendererError::from_chunk(0, err))
    }
}
