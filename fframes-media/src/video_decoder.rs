use crate::FFramesMediaError;
use crate::error::Result;
use crate::video_types::{FrameConvertOptions, PlaybackSpeed};
use ffmpeg_sys_fframes::SwsFlags::{SWS_BICUBIC, SWS_BILINEAR};
use ffmpeg_sys_fframes::*;
use std::borrow::Cow;
use std::collections::VecDeque;
use std::ffi::{CStr, CString, c_void};
use std::mem::size_of;
use std::path::Path;
use std::ptr;
use std::sync::{Arc, Mutex};
use usvgr::PreloadedImageData;

/// The video stream's duration in its own time base. Matroska and `WebM` leave the stream
/// duration unset (`AV_NOPTS_VALUE`); their muxers write each track's length as a `DURATION`
/// tag instead, so use that. Only when the tag is missing too fall back to the container's
/// duration, which also covers any longer audio track. Any of these can be missing or 0; the
/// caller treats a non-positive duration as unknown.
unsafe fn stream_duration(fmt_ctx: *mut AVFormatContext, stream: *mut AVStream) -> i64 {
    unsafe {
        if (*stream).duration > 0 {
            return (*stream).duration;
        }
        if let Some(micros) = duration_tag(stream) {
            return av_rescale_q(micros, AV_TIME_BASE_Q, (*stream).time_base);
        }
        if (*fmt_ctx).duration <= 0 {
            return (*stream).duration;
        }
        av_rescale_q((*fmt_ctx).duration, AV_TIME_BASE_Q, (*stream).time_base)
    }
}

/// The stream's `DURATION` tag (`HH:MM:SS.nnnnnnnnn`) in `AV_TIME_BASE` units.
unsafe fn duration_tag(stream: *mut AVStream) -> Option<i64> {
    let entry = unsafe { av_dict_get((*stream).metadata, c"DURATION".as_ptr(), ptr::null(), 0) };
    if entry.is_null() {
        return None;
    }
    let value = unsafe { CStr::from_ptr((*entry).value) }.to_str().ok()?;
    let mut parts = value.trim().splitn(3, ':');
    let hours: i64 = parts.next()?.parse().ok()?;
    let minutes: i64 = parts.next()?.parse().ok()?;
    let seconds = parts.next()?;
    let (seconds, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
    let seconds: i64 = seconds.parse().ok()?;
    // Keep the first 6 fractional digits (microseconds), right-padded with zeros.
    let micros = fraction
        .bytes()
        .chain(std::iter::repeat(b'0'))
        .take(6)
        .try_fold(0i64, |acc, digit| {
            digit
                .is_ascii_digit()
                .then(|| acc * 10 + i64::from(digit - b'0'))
        })?;
    // A malformed tag can be arbitrarily large; treat an overflow like a missing tag.
    let total = hours
        .checked_mul(60)?
        .checked_add(minutes)?
        .checked_mul(60)?
        .checked_add(seconds)?
        .checked_mul(1_000_000)?
        .checked_add(micros)?;
    (total > 0).then_some(total)
}

const FFRAMES_VIDEO_PATH_TAG: &str = "___fframes_internal_video_frame_pts___";
// just a little bit faster than the format! macro
pub fn encode_video_resource(resource: &str, pts: i64) -> String {
    let mut buffer = itoa::Buffer::new(); // it will always be on the stack
    let pts_str = buffer.format(pts);

    let mut s =
        String::with_capacity(resource.len() + FFRAMES_VIDEO_PATH_TAG.len() + pts_str.len() + 4);

    s.push_str(resource);
    s.push_str(FFRAMES_VIDEO_PATH_TAG);
    s.push_str(pts_str);
    s.push_str(".png");
    s
}

pub fn decode_video_resource(s: &str) -> Option<(&str, i64)> {
    let s = s.strip_suffix(".png")?;
    let (resource, pts_str) = s.split_once(FFRAMES_VIDEO_PATH_TAG)?;
    Some((resource, pts_str.parse().ok()?))
}

#[derive(Debug, Clone, Copy)]
struct VideoStreamInfo {
    hw_pix_fmt: Option<AVPixelFormat>,
    stream_index: i32,
    width: i32,
    height: i32,
    display_width: i32,
    display_height: i32,
    pixel_format: AVPixelFormat,
    color_space: AVColorSpace,
    color_range: AVColorRange,
    time_base: AVRational,
    frame_rate: AVRational,
    duration: i64,
}

#[derive(Debug)]
struct SwsScaler {
    width: i32,
    height: i32,
    sws_ctx: *mut SwsContext,
    options: Option<FrameConvertOptions>,
    source_format: Option<AVPixelFormat>,
    source_has_alpha: bool,
    source_color: Option<(AVColorSpace, AVColorRange)>,
    frame_data_len: usize,
    video_stream_info: VideoStreamInfo,
    linesize: [i32; 4],
}

// The scaler exclusively owns its SwsContext. All conversions use it under one mutex.
unsafe impl Send for SwsScaler {}

impl Drop for SwsScaler {
    fn drop(&mut self) {
        unsafe {
            sws_freeContext(self.sws_ctx);
        }
    }
}

const PIX_FMT_SIZE: usize = size_of::<i32>();
const MAX_PIXEL_STRETCH: i64 = 4;

/// Bytes `sws_scale` may write past the end of an RGBA image whose rows are `width * 4` bytes
/// apart. The x86 YUV to RGB converter of `FFmpeg` (`yuv2rgb_fn` in
/// `libswscale/x86/yuv_2_rgb.asm`) stores whole blocks of 16 pixels (64 bytes), at least one
/// per row, so it can write up to 60 bytes past the last row. Frames allocated by `FFmpeg`
/// have padding there.
const SWS_DST_PADDING: usize = 64;

impl SwsScaler {
    unsafe fn init_sws_context(
        video_stream_info: &VideoStreamInfo,
        source_pix_fmt: AVPixelFormat,
        source_color: (AVColorSpace, AVColorRange),
        target_width: i32,
        target_height: i32,
    ) -> Result<*mut SwsContext> {
        unsafe {
            let flags = if video_stream_info.width > target_width
                || video_stream_info.height > target_height
            {
                SWS_BICUBIC as libc::c_int // Downscaling
            } else if video_stream_info.width < target_width
                || video_stream_info.height < target_height
            {
                SWS_BILINEAR as libc::c_int // Upscaling
            } else {
                0
            };

            let ctx = sws_getContext(
                video_stream_info.width,
                video_stream_info.height,
                source_pix_fmt,
                target_width,
                target_height,
                AVPixelFormat::AV_PIX_FMT_RGBA,
                flags,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            );
            if ctx.is_null() {
                return Err(FFramesMediaError::LibAVAllocationError("scaling context"));
            }

            // sws_getContext defaults to BT.601, even for tagged BT.709 input.
            // Keep its pixel-format defaults (notably full-range YUVJ) when the
            // source does not specify a range. RGBA output always uses full range.
            let mut input_table = ptr::null_mut();
            let mut output_table = ptr::null_mut();
            let mut input_range = 0;
            let mut output_range = 0;
            let mut brightness = 0;
            let mut contrast = 0;
            let mut saturation = 0;
            let ret = sws_getColorspaceDetails(
                ctx,
                &raw mut input_table,
                &raw mut input_range,
                &raw mut output_table,
                &raw mut output_range,
                &raw mut brightness,
                &raw mut contrast,
                &raw mut saturation,
            );
            if ret < 0 {
                sws_freeContext(ctx);
                return Err(FFramesMediaError::LibAVAudioDecodingError((
                    ret,
                    "Could not read scaling color settings".to_string(),
                )));
            }

            let coefficients = match source_color.0 {
                AVColorSpace::AVCOL_SPC_BT709 => SWS_CS_ITU709,
                AVColorSpace::AVCOL_SPC_FCC => SWS_CS_FCC,
                AVColorSpace::AVCOL_SPC_BT470BG | AVColorSpace::AVCOL_SPC_SMPTE170M => {
                    SWS_CS_ITU601
                }
                AVColorSpace::AVCOL_SPC_SMPTE240M => SWS_CS_SMPTE240M,
                AVColorSpace::AVCOL_SPC_BT2020_NCL => SWS_CS_BT2020,
                _ => SWS_CS_DEFAULT,
            };
            match source_color.1 {
                AVColorRange::AVCOL_RANGE_JPEG => input_range = 1,
                AVColorRange::AVCOL_RANGE_MPEG => input_range = 0,
                _ => {}
            }
            // This selects the YUV matrix and range, not transfer-function or
            // color-primary conversion (for example HDR tone mapping).
            let ret = sws_setColorspaceDetails(
                ctx,
                sws_getCoefficients(coefficients),
                input_range,
                output_table,
                1,
                brightness,
                contrast,
                saturation,
            );
            if ret < 0 {
                sws_freeContext(ctx);
                return Err(FFramesMediaError::LibAVAudioDecodingError((
                    ret,
                    "Could not configure scaling color settings".to_string(),
                )));
            }

            Ok(ctx)
        }
    }

    fn calculate_linesize(width: i32) -> [i32; 4] {
        let mut linesize = [0; 4];
        linesize[0] = width * PIX_FMT_SIZE as i32;
        linesize
    }

    fn new(video_stream_info: VideoStreamInfo) -> Self {
        let (width, height) = (
            video_stream_info.display_width,
            video_stream_info.display_height,
        );
        let frame_data_len = width as usize * height as usize * PIX_FMT_SIZE;

        let linesize: [i32; 4] = Self::calculate_linesize(width);
        SwsScaler {
            linesize,
            frame_data_len,
            height,
            width,
            sws_ctx: std::ptr::null_mut(),
            options: None,
            source_format: None,
            source_has_alpha: false,
            source_color: None,
            video_stream_info,
        }
    }

    unsafe fn reinit_sws_context(
        &mut self,
        pix_fmt: AVPixelFormat,
        source_color: (AVColorSpace, AVColorRange),
        options: Option<FrameConvertOptions>,
    ) -> Result<()> {
        let new_width = options.map_or(self.video_stream_info.display_width, |o| {
            o.resize.width as i32
        });
        let new_height = options.map_or(self.video_stream_info.display_height, |o| {
            o.resize.height as i32
        });

        let ctx = Self::init_sws_context(
            &self.video_stream_info,
            pix_fmt,
            source_color,
            new_width,
            new_height,
        )?;
        sws_freeContext(self.sws_ctx);
        self.sws_ctx = ctx;
        self.source_format = Some(pix_fmt);
        let desc = unsafe { av_pix_fmt_desc_get(pix_fmt) };
        self.source_has_alpha =
            !desc.is_null() && unsafe { (*desc).flags } & AV_PIX_FMT_FLAG_ALPHA as u64 != 0;
        self.source_color = Some(source_color);
        self.width = new_width;
        self.height = new_height;
        self.options = options;

        self.frame_data_len = new_width as usize * new_height as usize * PIX_FMT_SIZE;
        self.linesize = Self::calculate_linesize(new_width);
        Ok(())
    }

    unsafe fn convert(
        &mut self,
        options: Option<FrameConvertOptions>,
        source_frame: *mut AVFrame,
        rgba_dst: &mut Vec<u8>,
    ) -> Result<()> {
        let source_pix_fmt: AVPixelFormat = std::mem::transmute((*source_frame).format);
        // Decoded frame metadata takes precedence over the stream defaults and
        // survives hardware transfers through av_frame_copy_props.
        let color_space = match (*source_frame).colorspace {
            AVColorSpace::AVCOL_SPC_UNSPECIFIED => self.video_stream_info.color_space,
            color_space => color_space,
        };
        let color_range = match (*source_frame).color_range {
            AVColorRange::AVCOL_RANGE_UNSPECIFIED => self.video_stream_info.color_range,
            color_range => color_range,
        };
        let source_color = (color_space, color_range);
        if self.sws_ctx.is_null()
            || self.options != options
            || self.source_format != Some(source_pix_fmt)
            || self.source_color != Some(source_color)
        {
            self.reinit_sws_context(source_pix_fmt, source_color, options)?;
        }

        // Reserve the SIMD tail without zeroing a full frame before swscale fills it.
        let capacity = self.frame_data_len + SWS_DST_PADDING;
        rgba_dst.reserve(capacity.saturating_sub(rgba_dst.len()));

        // sws_scale reads four plane pointers even for packed RGBA.
        let mut planes = [ptr::null_mut(); 4];
        planes[0] = rgba_dst.as_mut_ptr();
        let ret = sws_scale(
            self.sws_ctx,
            (*source_frame).data.as_ptr().cast::<*const u8>(),
            (*source_frame).linesize.as_ptr(),
            0,
            self.video_stream_info.height,
            planes.as_ptr(),
            self.linesize.as_ptr(),
        );

        if ret != self.height {
            return Err(FFramesMediaError::LibAVAudioDecodingError((
                ret,
                format!(
                    "Expected {} output rows from sws_scale, got {ret}",
                    self.height
                ),
            )));
        }
        // SAFETY: sws_scale initialized every RGBA row with width * 4 stride.
        // The tail padding lies inside the allocation but outside the image.
        unsafe { rgba_dst.set_len(self.frame_data_len) };

        Ok(())
    }
}

#[derive(Debug)]
pub struct FFmpegDecoder {
    pub current_loop: i64,
    hw_frame: *mut AVFrame,
    /// Frames are received here and moved into the target frame, so the newest decoded frame
    /// survives `avcodec_receive_frame` returning EOF (which unrefs the frame it is given).
    recv_frame: *mut AVFrame,
    frame_buf: Arc<FFmpegFrameBuf>,
    fmt_ctx: *mut AVFormatContext,
    video_stream_info: VideoStreamInfo,
    codec_ctx: *mut AVCodecContext,
    pkt: *mut AVPacket,
    /// Offsets are counted in this time base: `speed / target_fps` seconds per offset.
    custom_time_base: AVRational,
    target_fps: i32,
    /// The stream position of the previous decode or seek, in the stream's time base.
    last_pts: Option<i64>,
    /// A null packet has been sent; receive delayed frames until decoder EOF.
    draining: bool,
    /// The target frame holds a frame decoded since the last seek.
    has_decoded_frame: bool,
}

unsafe impl Send for FFmpegDecoder {}
unsafe impl Sync for FFmpegDecoder {}

#[derive(Debug)]
struct ScaledFrameImage {
    pts: i64,
    options: Option<FrameConvertOptions>,
    image: Arc<PreloadedImageData>,
}

#[derive(Debug)]
struct ConvertedFrames {
    scaler: SwsScaler,
    images: VecDeque<ScaledFrameImage>,
}

#[derive(Debug)]
pub struct FFmpegFrameBuf {
    resource_name: Arc<str>,
    latest_av_frame: *mut AVFrame,
    video_stream_info: VideoStreamInfo,
    converted: Arc<Mutex<ConvertedFrames>>,
}

impl Drop for FFmpegFrameBuf {
    fn drop(&mut self) {
        unsafe { av_frame_free(&raw mut self.latest_av_frame) };
    }
}

fn premultiply_alpha(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = u16::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
}

// The decoder owns writes to the AVFrame. Its unsafe readers require that it
// stays unchanged while they run. The conversion ring and scaler share a lock.
unsafe impl Send for FFmpegFrameBuf {}
unsafe impl Sync for FFmpegFrameBuf {}

impl FFmpegFrameBuf {
    unsafe fn new(
        resource_name: String,
        video_stream_info: VideoStreamInfo,
        buf_size: usize,
    ) -> Result<Self> {
        unsafe {
            let av_frame = av_frame_alloc();
            if av_frame.is_null() {
                return Err(FFramesMediaError::LibAVAllocationError("frame"));
            }

            let sws_ctx = SwsScaler::new(video_stream_info);
            Ok(FFmpegFrameBuf {
                video_stream_info,
                latest_av_frame: av_frame,
                resource_name: resource_name.into(),
                converted: Arc::new(Mutex::new(ConvertedFrames {
                    scaler: sws_ctx,
                    images: VecDeque::with_capacity(buf_size.max(1)),
                })),
            })
        }
    }

    pub fn get_stream_width(&self) -> u32 {
        self.video_stream_info.display_width as u32
    }

    pub fn get_stream_height(&self) -> u32 {
        self.video_stream_info.display_height as u32
    }

    pub fn get_width(&self) -> u32 {
        self.converted.lock().unwrap().scaler.width as u32
    }

    pub fn get_height(&self) -> u32 {
        self.converted.lock().unwrap().scaler.height as u32
    }

    /// Converts the current frame to an immutable, premultiplied RGBA image.
    /// # Safety
    /// The decoder must not replace the underlying `AVFrame` during conversion.
    pub unsafe fn convert_last_decoded_frame_into_svg_image(
        &self,
        options: Option<&FrameConvertOptions>,
    ) -> Result<Arc<PreloadedImageData>> {
        let pts = unsafe { (*self.latest_av_frame).pts };
        let options = options.copied();
        let mut converted = self.converted.lock().unwrap();
        let ConvertedFrames { scaler, images } = &mut *converted;
        if let Some(last) = images.back()
            && last.pts == pts
            && last.options == options
        {
            return Ok(Arc::clone(&last.image));
        }

        // A queued frame may still own the evicted image.
        let mut pixels = if images.len() == images.capacity() {
            Arc::try_unwrap(images.pop_front().unwrap().image)
                .map(|image| image.data.into_owned())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        unsafe { scaler.convert(options, self.latest_av_frame, &mut pixels)? };
        if scaler.source_has_alpha {
            premultiply_alpha(&mut pixels);
        }
        let image = Arc::new(PreloadedImageData {
            id: encode_video_resource(&self.resource_name, pts),
            width: scaler.width as u32,
            height: scaler.height as u32,
            data: Cow::Owned(pixels),
        });
        images.push_back(ScaledFrameImage {
            pts,
            options,
            image: Arc::clone(&image),
        });
        Ok(image)
    }

    /// Returns the timestamp of the frame in the native frame timebase
    /// # Safety
    /// This is a libav based function which involves C ffi cals
    pub unsafe fn get_pts(&self) -> i64 {
        unsafe { (*self.latest_av_frame).pts }
    }

    /// Returns the timestamp of the frame in seconds
    /// # Safety
    /// This is a libav based function which involves C ffi cals
    pub unsafe fn timestamp_seconds(&self) -> f32 {
        unsafe {
            let timestamp =
                (*self.latest_av_frame).pts as f64 * av_q2d(self.video_stream_info.time_base);

            timestamp as f32
        }
    }

    /// Returns the duration of the stream in frames
    /// # Safety
    /// This is a libav based function which involves C ffi cals
    pub unsafe fn get_stream_duration_in_frames(&self) -> f32 {
        unsafe {
            let fps =
                self.video_stream_info.duration as f64 * av_q2d(self.video_stream_info.time_base);

            fps as f32
        }
    }

    /// Returns the fps value of the stream
    /// Remember that the fps value is always a guess based on the stream timestamps
    pub fn get_stream_fps(&self) -> f32 {
        self.video_stream_info.frame_rate.num as f32 / self.video_stream_info.frame_rate.den as f32
    }
}

impl FFmpegDecoder {
    pub fn get_raw_frame(&self) -> Arc<FFmpegFrameBuf> {
        Arc::clone(&self.frame_buf)
    }

    /// Retains the decoded planes so this frame can be read after the decoder advances
    /// or seeks. `FFmpeg` clones the frame header and reference-counts the pixel buffers.
    pub fn retained_frame(&self) -> Result<Arc<FFmpegFrameBuf>> {
        let frame = &self.frame_buf;
        let latest_av_frame = unsafe { av_frame_clone(frame.latest_av_frame) };
        if latest_av_frame.is_null() {
            return Err(FFramesMediaError::LibAVAllocationError("frame"));
        }
        Ok(Arc::new(FFmpegFrameBuf {
            latest_av_frame,
            resource_name: Arc::clone(&frame.resource_name),
            video_stream_info: self.video_stream_info,
            converted: Arc::clone(&frame.converted),
        }))
    }

    pub fn get_decoded_image_in_buf(&self, pts: i64) -> Option<Arc<PreloadedImageData>> {
        self.frame_buf
            .converted
            .lock()
            .unwrap()
            .images
            .iter()
            .rev()
            .find(|slot| slot.pts == pts)
            .map(|slot| Arc::clone(&slot.image))
    }

    /// Plays the stream `speed` times faster: offset `n` shows the frame at
    /// `n * speed / target_fps` seconds of the stream.
    pub fn with_playback_speed(mut self, speed: PlaybackSpeed) -> Self {
        self.custom_time_base = AVRational {
            num: speed.thousandths(),
            den: self.target_fps * PlaybackSpeed::SCALE,
        };
        self.last_pts = None;
        self.current_loop = 0;
        self
    }

    /// The stream position of the offset, in the stream's time base.
    fn offset_pts(&self, offset: i64) -> i64 {
        unsafe {
            av_rescale_q(
                offset,
                self.custom_time_base,
                self.video_stream_info.time_base,
            )
        }
    }

    /// The exact stream duration, in the stream's time base. `None` if it is unknown.
    fn stream_duration(&self) -> Option<i64> {
        let duration = self.video_stream_info.duration;
        (duration > 0).then_some(duration)
    }

    /// Creates a new decoder for the video file at the specified path
    /// # Safety
    /// Generally safe but uses libav functions
    pub unsafe fn new(path: &Path, target_fps: usize, buffer_size: usize) -> Result<Self> {
        unsafe {
            let filename = path
                .file_name()
                .ok_or_else(|| FFramesMediaError::MediaDirectoryProvided)?;
            // libavformat expects UTF-8 paths on every platform (it converts them to
            // wide strings itself on Windows), so raw OS bytes are not portable here.
            let full_path_cstr = CString::new(path.to_string_lossy().as_ref())?;

            let mut fmt_ctx: *mut AVFormatContext = ptr::null_mut();
            let ret = avformat_open_input(
                &raw mut fmt_ctx,
                full_path_cstr.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
            );

            if ret < 0 {
                if !fmt_ctx.is_null() {
                    avformat_close_input(&raw mut fmt_ctx);
                }
                return Err(FFramesMediaError::LibAVAudioDecodingError((
                    ret,
                    "Could not open input file".to_string(),
                )));
            }

            let ret = avformat_find_stream_info(fmt_ctx, ptr::null_mut());
            if ret < 0 {
                avformat_close_input(&raw mut fmt_ctx);
                return Err(FFramesMediaError::LibAVAudioDecodingError((
                    ret,
                    "Could not find stream information".to_string(),
                )));
            }

            let (codec_ctx, video_stream_info) = Self::open_codec_context(fmt_ctx)?;
            let pkt = av_packet_alloc();
            if pkt.is_null() {
                avformat_close_input(&raw mut fmt_ctx);
                return Err(FFramesMediaError::LibAVAllocationError("packet"));
            }

            let hw_frame = if video_stream_info.hw_pix_fmt.is_some() {
                let hw_frame = av_frame_alloc();
                if hw_frame.is_null() {
                    avformat_close_input(&raw mut fmt_ctx);
                    return Err(FFramesMediaError::LibAVAllocationError("av_frame"));
                }

                (*hw_frame).width = video_stream_info.width;
                (*hw_frame).height = video_stream_info.height;
                (*hw_frame).format = video_stream_info.pixel_format as i32;

                hw_frame
            } else {
                ptr::null_mut()
            };

            let recv_frame = av_frame_alloc();
            if recv_frame.is_null() {
                avformat_close_input(&raw mut fmt_ctx);
                return Err(FFramesMediaError::LibAVAllocationError("av_frame"));
            }

            let custom_time_base = AVRational {
                num: 1,
                den: target_fps as i32,
            };

            Ok(FFmpegDecoder {
                codec_ctx,
                pkt,
                fmt_ctx,
                hw_frame,
                recv_frame,
                frame_buf: Arc::new(FFmpegFrameBuf::new(
                    filename.to_string_lossy().to_string(),
                    video_stream_info,
                    buffer_size,
                )?),
                video_stream_info,
                custom_time_base,
                target_fps: target_fps as i32,
                current_loop: 0,
                last_pts: None,
                draining: false,
                has_decoded_frame: false,
            })
        }
    }

    unsafe fn open_codec_context(
        fmt_ctx: *mut AVFormatContext,
    ) -> Result<(*mut AVCodecContext, VideoStreamInfo)> {
        unsafe {
            let ret = av_find_best_stream(
                fmt_ctx,
                AVMediaType::AVMEDIA_TYPE_VIDEO,
                -1,
                -1,
                ptr::null_mut(),
                0,
            );
            if ret < 0 {
                return Err(FFramesMediaError::LibAVAudioDecodingError((
                    ret,
                    "Could not find video stream info".to_string(),
                )));
            }
            let video_stream_idx = ret;

            let stream = *(*fmt_ctx).streams.offset(video_stream_idx as isize);
            let dec = avcodec_find_decoder((*(*stream).codecpar).codec_id);

            let mut hw_device_ctx: *mut AVBufferRef = ptr::null_mut();
            let hw_pix_fmt = find_hw_accelleleration_for_codec(&mut hw_device_ctx, dec);

            if dec.is_null() {
                return Err(FFramesMediaError::LibAVAudioDecodingError((
                    ret,
                    "Could not find decoder".to_string(),
                )));
            }

            let video_dec_ctx = avcodec_alloc_context3(dec);
            if video_dec_ctx.is_null() {
                return Err(FFramesMediaError::LibAVAllocationError("avcocdec_context"));
            }

            if let Some(hw_pix_fmt) = hw_pix_fmt {
                (*video_dec_ctx).opaque = hw_pix_fmt as i32 as *mut c_void;
                (*video_dec_ctx).get_format = Some(get_hw_format);
                (*video_dec_ctx).hw_device_ctx = av_buffer_ref(hw_device_ctx);
            }

            if avcodec_parameters_to_context(video_dec_ctx, (*stream).codecpar) < 0 {
                return Err(FFramesMediaError::LibAVAudioDecodingError((
                    ret,
                    "Failed to copy video codec parameters to decoder context".to_string(),
                )));
            }

            if avcodec_open2(video_dec_ctx, dec, ptr::null_mut()) < 0 {
                return Err(FFramesMediaError::LibAVAudioDecodingError((
                    ret,
                    "Failed to open video codec".to_string(),
                )));
            }

            let (width, height) = ((*video_dec_ctx).width, (*video_dec_ctx).height);
            let (display_width, display_height) = match [
                (*stream).sample_aspect_ratio,
                (*video_dec_ctx).sample_aspect_ratio,
            ]
            .into_iter()
            .find(|ratio| {
                let (num, den) = (i64::from(ratio.num), i64::from(ratio.den));
                num > 0
                    && den > 0
                    && num <= MAX_PIXEL_STRETCH * den
                    && den <= MAX_PIXEL_STRETCH * num
            }) {
                Some(AVRational { num, den }) => {
                    let ratio = f64::from(num) / f64::from(den);
                    (
                        (f64::from(width) * ratio.max(1.0)).round() as i32,
                        (f64::from(height) / ratio.min(1.0)).round() as i32,
                    )
                }
                None => (width, height),
            };
            Ok((
                video_dec_ctx,
                VideoStreamInfo {
                    hw_pix_fmt,
                    stream_index: video_stream_idx,
                    width,
                    height,
                    display_width,
                    display_height,
                    pixel_format: (*video_dec_ctx).pix_fmt,
                    color_space: (*video_dec_ctx).colorspace,
                    color_range: (*video_dec_ctx).color_range,
                    time_base: (*stream).time_base,
                    duration: stream_duration(fmt_ctx, stream),
                    frame_rate: (*stream).r_frame_rate,
                },
            ))
        }
    }

    /// Seeks to the specified offset in the video stream (to the nearest keyframe)
    /// # Safety
    /// Generally safe but uses libav functions
    pub unsafe fn seek_to_offset(&mut self, offset: i64) -> Result<()> {
        unsafe { self.seek_to_pts(self.offset_pts(offset)) }
    }

    unsafe fn seek_to_pts(&mut self, timestamp: i64) -> Result<()> {
        unsafe {
            let ret = av_seek_frame(
                self.fmt_ctx,
                self.video_stream_info.stream_index,
                timestamp,
                AVSEEK_FLAG_BACKWARD,
            );

            if ret < 0 {
                return Err(FFramesMediaError::LibAVAudioDecodingError((
                    ret,
                    "Error seeking to offset".to_string(),
                )));
            }

            (*self.frame_buf.latest_av_frame).pts = -1;
            avcodec_flush_buffers(self.codec_ctx);
            self.draining = false;
            self.has_decoded_frame = false;
            self.last_pts = Some(timestamp);
            av_packet_unref(self.pkt);

            Ok(())
        }
    }

    /// Decodes the frame at the offset of the video played in a loop. The position in
    /// the loop comes from the exact stream duration, so loops at a fractional speed
    /// do not drift and do not end early.
    /// # Safety
    /// Generally safe but uses libav functions
    pub unsafe fn decode_looping(&mut self, offset: i64) -> Result<bool> {
        unsafe {
            let pts = self.offset_pts(offset);
            // An unknown duration can't be looped over (and would divide by zero below).
            let Some(duration) = self.stream_duration() else {
                return self.decode_up_to_pts(pts);
            };

            let loop_index = pts.div_euclid(duration);
            let pts = pts.rem_euclid(duration);
            if loop_index != self.current_loop {
                self.current_loop = loop_index;
                self.seek_to_pts(pts)?;
            }

            if self.decode_up_to_pts(pts)? {
                return Ok(true);
            }

            // The frames end before the stream's duration: start the next loop.
            self.current_loop += 1;
            self.seek_to_pts(0)?;
            self.decode_up_to_pts(0)
        }
    }

    /// If we are actually using hardware accelerated frames it is required to correctly
    /// move the data from the hardware surface to the software frame
    unsafe fn transfer_hardware_surface_data(&self, target_frame: *mut AVFrame) -> Result<()> {
        match self.video_stream_info.hw_pix_fmt {
            Some(hw_pix_fmt) if hw_pix_fmt as i32 == (*target_frame).format => {
                // A retained frame can still own the last transfer's software planes.
                let output = self.frame_buf.latest_av_frame;
                if !(*output).buf[0].is_null() && av_frame_is_writable(output) == 0 {
                    av_frame_unref(output);
                }
                if (*self.frame_buf.latest_av_frame).format
                    == (AVPixelFormat::AV_PIX_FMT_NONE as i32)
                {
                    // if the target pix fmt is not initialized we are negotiating the format
                    // to get the fastest conversion into the target fframes format (RGBA)
                    if let Some(best_hw_out_format) = find_hw_out_source_format(target_frame) {
                        (*self.frame_buf.latest_av_frame).format = best_hw_out_format as i32;
                    }
                }

                let ret =
                    av_hwframe_transfer_data(self.frame_buf.latest_av_frame, self.hw_frame, 0);

                if ret < 0 {
                    return Err(FFramesMediaError::LibAVAudioDecodingError((
                        ret,
                        "Error transferring data from GPU surface".to_string(),
                    )));
                }

                av_frame_copy_props(self.frame_buf.latest_av_frame, self.hw_frame);
            }
            // The decoder fell back to software decoding: the frame it returned already
            // holds the pixels. Keep a reference in the target too, since EOF may need
            // to present it again when the output frame rate exceeds the clip's rate.
            _ if target_frame != self.frame_buf.latest_av_frame => {
                av_frame_unref(self.frame_buf.latest_av_frame);
                let ret = av_frame_ref(self.frame_buf.latest_av_frame, target_frame);
                if ret < 0 {
                    return Err(FFramesMediaError::LibAVAudioDecodingError((
                        ret,
                        "Error retaining software decoded frame".to_string(),
                    )));
                }
            }
            _ => (),
        }

        Ok(())
    }

    /// Decodes the video stream up to the specified offset
    /// # Safety
    /// Generally safe but uses libav functions
    pub unsafe fn decode_up_to(&mut self, offset: i64) -> Result<bool> {
        unsafe { self.decode_up_to_pts(self.offset_pts(offset)) }
    }

    unsafe fn decode_up_to_pts(&mut self, target_pts: i64) -> Result<bool> {
        unsafe {
            // Going back needs a seek, otherwise the newer frame that was already decoded
            // would be returned. Far jumps forward (more than 2 seconds of the stream) seek
            // to the closest keyframe instead of decoding every frame in between (frames can
            // be requested out of order by parallel renderers). A new decoder is at the start.
            let seek_ahead = av_rescale_q(
                2,
                AVRational { num: 1, den: 1 },
                self.video_stream_info.time_base,
            );
            let last_pts = self.last_pts.unwrap_or(0);
            if target_pts < last_pts || target_pts - last_pts > seek_ahead {
                self.seek_to_pts(target_pts)?;
            }
            self.last_pts = Some(target_pts);

            if (*self.frame_buf.latest_av_frame).pts >= target_pts {
                return Ok(true);
            }

            let target_frame = if self.hw_frame.is_null() {
                self.frame_buf.latest_av_frame
            } else {
                self.hw_frame
            };

            loop {
                // A previous call may have returned with more decoded frames queued.
                // Consume them before sending another packet (which could return EAGAIN).
                let ret = avcodec_receive_frame(self.codec_ctx, self.recv_frame);
                match ret {
                    0 => {
                        av_frame_unref(target_frame);
                        av_frame_move_ref(target_frame, self.recv_frame);
                        self.has_decoded_frame = true;
                        if (*target_frame).pts >= target_pts {
                            self.transfer_hardware_surface_data(target_frame)?;
                            return Ok(true);
                        }
                        continue;
                    }
                    AVERROR_EOF => {
                        // The target lies after the last frame's timestamp but before the end of
                        // the stream (e.g. a 24 fps clip sampled at 30 fps): the last frame is
                        // still the one on screen, so return it instead of reporting the end.
                        let shows_last_frame = self.has_decoded_frame
                            && self
                                .stream_duration()
                                .is_some_and(|duration| target_pts < duration);
                        if shows_last_frame {
                            self.transfer_hardware_surface_data(target_frame)?;
                        }
                        return Ok(shows_last_frame);
                    }
                    val if val == AVERROR(EAGAIN) && !self.draining => {}
                    _ => {
                        return Err(FFramesMediaError::LibAVAudioDecodingError((
                            ret,
                            "Error receiving decoded video frame".to_string(),
                        )));
                    }
                }

                // The decoder needs more input. Skip packets from other streams and
                // signal demuxer EOF exactly once so delayed B-frames can be received.
                let read_result = loop {
                    av_packet_unref(self.pkt);
                    let ret = av_read_frame(self.fmt_ctx, self.pkt);
                    if ret < 0 || (*self.pkt).stream_index == self.video_stream_info.stream_index {
                        break ret;
                    }
                };
                if read_result < 0 && read_result != AVERROR_EOF {
                    av_packet_unref(self.pkt);
                    return Err(FFramesMediaError::LibAVAudioDecodingError((
                        read_result,
                        "Error reading video packet".to_string(),
                    )));
                }

                let packet = if read_result == AVERROR_EOF {
                    ptr::null()
                } else {
                    self.pkt.cast_const()
                };
                let ret = avcodec_send_packet(self.codec_ctx, packet);
                av_packet_unref(self.pkt);
                if ret < 0 {
                    return Err(FFramesMediaError::LibAVAudioDecodingError((
                        ret,
                        "Error submitting packet for decoding".to_string(),
                    )));
                }
                self.draining = read_result == AVERROR_EOF;
            }
        }
    }
}

impl Drop for FFmpegDecoder {
    fn drop(&mut self) {
        unsafe {
            av_frame_free(&raw mut self.recv_frame);
            av_frame_free(&raw mut self.hw_frame);
            avcodec_free_context(&raw mut self.codec_ctx);
            avformat_close_input(&raw mut self.fmt_ctx);
            av_packet_free(&raw mut self.pkt);
        }
    }
}

unsafe fn find_hw_accelleleration_for_codec(
    hw_device_ctx: &mut *mut AVBufferRef,
    codec: *const AVCodec,
) -> Option<AVPixelFormat> {
    unsafe {
        let hw_types = [
            #[cfg(feature = "vaapi")]
            AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
            #[cfg(feature = "direct3d11")]
            AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA,
            #[cfg(feature = "direct3d9")]
            AVHWDeviceType::AV_HWDEVICE_TYPE_DXVA2,
            #[cfg(feature = "videotoolbox")]
            AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
            #[cfg(feature = "nvidia")]
            AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA,
            #[cfg(feature = "qsv")]
            AVHWDeviceType::AV_HWDEVICE_TYPE_QSV,
            #[cfg(feature = "mediacodec")]
            AVHWDeviceType::AV_HWDEVICE_TYPE_MEDIACODEC,
            #[cfg(feature = "vulkan")]
            AVHWDeviceType::AV_HWDEVICE_TYPE_VULKAN,
        ];

        for &hw_type in &hw_types {
            let create_result =
                av_hwdevice_ctx_create(hw_device_ctx, hw_type, ptr::null(), ptr::null_mut(), 0);

            if create_result < 0 {
                continue;
            }

            let mut hw_pix_fmt = AVPixelFormat::AV_PIX_FMT_NONE;

            let mut i = 0;
            loop {
                // Check if decoder supports this hardware if yes we return the available pix fmt
                let config = avcodec_get_hw_config(codec, i);
                if config.is_null() {
                    break;
                }

                if (*config).device_type == hw_type
                    && ((*config).methods & AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32) != 0
                {
                    hw_pix_fmt = (*config).pix_fmt;
                    break;
                }

                i += 1;
            }

            if hw_pix_fmt != AVPixelFormat::AV_PIX_FMT_NONE {
                return Some(hw_pix_fmt);
            }

            av_buffer_unref(hw_device_ctx);
        }
    }

    None
}

/// Callback called from the C world that negotiating the available pixel format
/// from the encoder stream and sets the desired pix format for the hardware acceleration
unsafe extern "C" fn get_hw_format(
    ctx: *mut AVCodecContext,
    pix_fmts: *const AVPixelFormat,
) -> AVPixelFormat {
    unsafe {
        let hw_pix_fmt: AVPixelFormat = std::mem::transmute((*ctx).opaque as i32);

        let mut p = pix_fmts;
        while !p.is_null() && *p != AVPixelFormat::AV_PIX_FMT_NONE {
            if *p == hw_pix_fmt {
                return *p;
            }
            p = p.add(1);
        }

        // The device has no decoder for this stream (a driver without the codec, an
        // unsupported profile). The formats that are left decode in software.
        eprintln!("Failed to get HW surface format, falling back to software decoding");
        let mut p = pix_fmts;
        while !p.is_null() && *p != AVPixelFormat::AV_PIX_FMT_NONE {
            let desc = av_pix_fmt_desc_get(*p);
            if !desc.is_null() && (*desc).flags & AV_PIX_FMT_FLAG_HWACCEL as u64 == 0 {
                return *p;
            }
            p = p.add(1);
        }

        AVPixelFormat::AV_PIX_FMT_NONE
    }
}

unsafe fn find_hw_out_source_format(frame: *mut AVFrame) -> Option<AVPixelFormat> {
    let mut formats: *mut AVPixelFormat = ptr::null_mut();
    let ret = av_hwframe_transfer_get_formats(
        (*frame).hw_frames_ctx,
        AVHWFrameTransferDirection::AV_HWFRAME_TRANSFER_DIRECTION_FROM,
        &raw mut formats,
        0,
    );

    if ret < 0 {
        return None;
    }

    let mut fmt = formats;
    loop {
        match *fmt {
            AVPixelFormat::AV_PIX_FMT_YUV420P
            | AVPixelFormat::AV_PIX_FMT_RGBA
            | AVPixelFormat::AV_PIX_FMT_BGRA => {
                let res = *fmt;
                av_freep((&raw mut formats).cast::<c_void>());

                return Some(res);
            }
            AVPixelFormat::AV_PIX_FMT_NONE => {
                break;
            }
            _ => {}
        }

        fmt = fmt.add(1);
    }

    let first_format = *formats;
    av_freep((&raw mut formats).cast::<c_void>());

    Some(first_format)
}

#[cfg(test)]
mod color_tests {
    use super::*;
    use crate::video_types::ResizeVideoFrame;

    const WIDTH: i32 = 32;
    const HEIGHT: i32 = 16;

    #[test]
    fn in_place_alpha_matches_the_renderer_for_every_channel_and_alpha() {
        let mut pixels = Vec::new();
        for alpha in 0..=255u8 {
            for color in 0..=255u8 {
                pixels.extend_from_slice(&[color, 255 - color, color, alpha]);
            }
        }
        let expected = PreloadedImageData::blend_rgba_slice(&pixels);
        premultiply_alpha(&mut pixels);
        assert_eq!(pixels, expected);
    }

    struct TestFrame(*mut AVFrame);

    impl TestFrame {
        fn new(format: AVPixelFormat) -> Self {
            unsafe {
                let frame = av_frame_alloc();
                assert!(!frame.is_null());
                (*frame).width = WIDTH;
                (*frame).height = HEIGHT;
                (*frame).format = format as i32;
                assert_eq!(av_frame_get_buffer(frame, 32), 0);
                Self(frame)
            }
        }

        fn fill(&mut self, yuv: [u8; 3], space: AVColorSpace, range: AVColorRange) {
            unsafe {
                (*self.0).colorspace = space;
                (*self.0).color_range = range;
                for (plane, value) in yuv.into_iter().enumerate() {
                    for row in 0..HEIGHT {
                        ptr::write_bytes(
                            (*self.0).data[plane]
                                .offset((row * (*self.0).linesize[plane]) as isize),
                            value,
                            WIDTH as usize,
                        );
                    }
                }
            }
        }
    }

    impl Drop for TestFrame {
        fn drop(&mut self) {
            unsafe { av_frame_free(&raw mut self.0) }
        }
    }

    fn scaler(space: AVColorSpace, range: AVColorRange) -> SwsScaler {
        SwsScaler::new(VideoStreamInfo {
            hw_pix_fmt: None,
            stream_index: 0,
            width: WIDTH,
            height: HEIGHT,
            display_width: WIDTH,
            display_height: HEIGHT,
            pixel_format: AVPixelFormat::AV_PIX_FMT_YUV444P,
            color_space: space,
            color_range: range,
            time_base: AVRational { num: 1, den: 30 },
            frame_rate: AVRational { num: 30, den: 1 },
            duration: 1,
        })
    }

    // Independent Y'CbCr equations; tolerate libswscale's integer rounding.
    fn expected_rgb(yuv: [u8; 3], bt709: bool, full_range: bool) -> [u8; 4] {
        let (kr, kb) = if bt709 {
            (0.2126, 0.0722)
        } else {
            (0.299, 0.114)
        };
        let kg = 1.0 - kr - kb;
        let (y, cb, cr) = if full_range {
            (
                f64::from(yuv[0]),
                f64::from(yuv[1]) - 128.0,
                f64::from(yuv[2]) - 128.0,
            )
        } else {
            (
                (f64::from(yuv[0]) - 16.0) * 255.0 / 219.0,
                (f64::from(yuv[1]) - 128.0) * 255.0 / 224.0,
                (f64::from(yuv[2]) - 128.0) * 255.0 / 224.0,
            )
        };
        let rgb = [
            y + 2.0 * (1.0 - kr) * cr,
            y - 2.0 * kb * (1.0 - kb) / kg * cb - 2.0 * kr * (1.0 - kr) / kg * cr,
            y + 2.0 * (1.0 - kb) * cb,
        ];
        [
            rgb[0].round().clamp(0.0, 255.0) as u8,
            rgb[1].round().clamp(0.0, 255.0) as u8,
            rgb[2].round().clamp(0.0, 255.0) as u8,
            255,
        ]
    }

    fn assert_conversion(
        scaler: &mut SwsScaler,
        frame: &TestFrame,
        options: Option<FrameConvertOptions>,
        expected: [u8; 4],
    ) {
        let width = options.map_or(WIDTH as usize, |o| o.resize.width as usize);
        let height = options.map_or(HEIGHT as usize, |o| o.resize.height as usize);
        let mut rgba = vec![0; width * height * 4];
        unsafe { scaler.convert(options, frame.0, &mut rgba).unwrap() };
        for pixel in rgba.as_chunks::<4>().0 {
            for (actual, expected) in pixel.iter().zip(expected) {
                assert!(actual.abs_diff(expected) <= 2, "{pixel:?} != {expected:?}");
            }
            assert_eq!(pixel[3], 255);
        }
    }

    #[test]
    fn converts_bt709_and_bt601_with_limited_and_full_range() {
        let mut scaler = scaler(
            AVColorSpace::AVCOL_SPC_UNSPECIFIED,
            AVColorRange::AVCOL_RANGE_UNSPECIFIED,
        );
        let mut frame = TestFrame::new(AVPixelFormat::AV_PIX_FMT_YUV444P);
        for space in [
            AVColorSpace::AVCOL_SPC_BT709,
            AVColorSpace::AVCOL_SPC_SMPTE170M,
        ] {
            for range in [
                AVColorRange::AVCOL_RANGE_MPEG,
                AVColorRange::AVCOL_RANGE_JPEG,
            ] {
                for yuv in [
                    [81, 90, 240],
                    [145, 54, 34],
                    [41, 240, 110],
                    [16, 128, 128],
                    [235, 128, 128],
                ] {
                    frame.fill(yuv, space, range);
                    assert_conversion(
                        &mut scaler,
                        &frame,
                        None,
                        expected_rgb(
                            yuv,
                            space == AVColorSpace::AVCOL_SPC_BT709,
                            range == AVColorRange::AVCOL_RANGE_JPEG,
                        ),
                    );
                }
            }
        }
    }

    #[test]
    fn frame_metadata_overrides_stream_defaults_and_survives_resize() {
        let mut scaler = scaler(
            AVColorSpace::AVCOL_SPC_BT709,
            AVColorRange::AVCOL_RANGE_JPEG,
        );
        let mut frame = TestFrame::new(AVPixelFormat::AV_PIX_FMT_YUV444P);
        let yuv = [100, 90, 180];
        for (space, range, bt709, full_range) in [
            (
                AVColorSpace::AVCOL_SPC_UNSPECIFIED,
                AVColorRange::AVCOL_RANGE_UNSPECIFIED,
                true,
                true,
            ),
            (
                AVColorSpace::AVCOL_SPC_SMPTE170M,
                AVColorRange::AVCOL_RANGE_MPEG,
                false,
                false,
            ),
            (
                AVColorSpace::AVCOL_SPC_BT709,
                AVColorRange::AVCOL_RANGE_MPEG,
                true,
                false,
            ),
            (
                AVColorSpace::AVCOL_SPC_UNSPECIFIED,
                AVColorRange::AVCOL_RANGE_UNSPECIFIED,
                true,
                true,
            ),
        ] {
            frame.fill(yuv, space, range);
            for options in [
                None,
                Some(FrameConvertOptions {
                    resize: ResizeVideoFrame {
                        width: 16,
                        height: 8,
                    },
                }),
                None,
            ] {
                assert_conversion(
                    &mut scaler,
                    &frame,
                    options,
                    expected_rgb(yuv, bt709, full_range),
                );
            }
        }
    }

    #[test]
    fn rgb_and_rgba_keep_pixel_values_and_alpha() {
        let mut scaler = scaler(
            AVColorSpace::AVCOL_SPC_UNSPECIFIED,
            AVColorRange::AVCOL_RANGE_UNSPECIFIED,
        );
        for (format, channels) in [
            (AVPixelFormat::AV_PIX_FMT_RGB24, 3),
            (AVPixelFormat::AV_PIX_FMT_RGBA, 4),
            (AVPixelFormat::AV_PIX_FMT_RGB24, 3),
        ] {
            let frame = TestFrame::new(format);
            let mut expected = Vec::new();
            for row in 0..HEIGHT {
                for column in 0..WIDTH {
                    let pixel = [
                        (column * 8) as u8,
                        (row * 16) as u8,
                        (255 - column * 8) as u8,
                        if channels == 4 { (row * 16) as u8 } else { 255 },
                    ];
                    expected.extend_from_slice(&pixel);
                    unsafe {
                        ptr::copy_nonoverlapping(
                            pixel.as_ptr(),
                            (*frame.0).data[0].offset(
                                (row * (*frame.0).linesize[0] + column * channels) as isize,
                            ),
                            channels as usize,
                        );
                    }
                }
            }
            for (space, range) in [
                (
                    AVColorSpace::AVCOL_SPC_UNSPECIFIED,
                    AVColorRange::AVCOL_RANGE_UNSPECIFIED,
                ),
                (AVColorSpace::AVCOL_SPC_RGB, AVColorRange::AVCOL_RANGE_JPEG),
                (
                    AVColorSpace::AVCOL_SPC_BT709,
                    AVColorRange::AVCOL_RANGE_MPEG,
                ),
            ] {
                let mut actual = vec![0; expected.len()];
                unsafe {
                    (*frame.0).colorspace = space;
                    (*frame.0).color_range = range;
                    scaler.convert(None, frame.0, &mut actual).unwrap();
                }
                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    fn unspecified_metadata_keeps_bt601_and_pixel_format_range_defaults() {
        let mut scaler = scaler(
            AVColorSpace::AVCOL_SPC_UNSPECIFIED,
            AVColorRange::AVCOL_RANGE_UNSPECIFIED,
        );
        let yuv = [100, 90, 180];
        for format in [
            AVPixelFormat::AV_PIX_FMT_YUV444P,
            AVPixelFormat::AV_PIX_FMT_YUVJ444P,
            AVPixelFormat::AV_PIX_FMT_YUV444P,
        ] {
            let mut frame = TestFrame::new(format);
            // First apply explicit metadata, then ensure unspecified metadata
            // resets the cached matrix/range rather than retaining that state.
            frame.fill(
                yuv,
                AVColorSpace::AVCOL_SPC_BT709,
                AVColorRange::AVCOL_RANGE_JPEG,
            );
            assert_conversion(&mut scaler, &frame, None, expected_rgb(yuv, true, true));
            frame.fill(
                yuv,
                AVColorSpace::AVCOL_SPC_UNSPECIFIED,
                AVColorRange::AVCOL_RANGE_UNSPECIFIED,
            );
            assert_conversion(
                &mut scaler,
                &frame,
                None,
                expected_rgb(yuv, false, format == AVPixelFormat::AV_PIX_FMT_YUVJ444P),
            );
        }
    }
}
