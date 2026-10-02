use crate::FFramesMediaError;
use crate::error::Result;
use crate::video_types::FrameConvertOptions;
use ffmpeg_sys_fframes::SwsFlags::{SWS_BICUBIC, SWS_BILINEAR};
use ffmpeg_sys_fframes::*;
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::ffi::{CString, c_void};
use std::mem::size_of;
use std::path::Path;
use std::ptr;
use std::sync::Arc;
use usvgr::PreloadedImageData;

/// The video stream's duration in its own time base. Matroska and `WebM` leave the stream
/// duration unset (`AV_NOPTS_VALUE`) and only carry the container's, so fall back to that.
unsafe fn stream_duration(fmt_ctx: *mut AVFormatContext, stream: *mut AVStream) -> i64 {
    unsafe {
        if (*stream).duration != AV_NOPTS_VALUE || (*fmt_ctx).duration == AV_NOPTS_VALUE {
            return (*stream).duration;
        }
        av_rescale_q((*fmt_ctx).duration, AV_TIME_BASE_Q, (*stream).time_base)
    }
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
    codec_ctx: *mut AVCodecContext,
    width: i32,
    height: i32,
    pixel_format: AVPixelFormat,
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
    frame_data_len: usize,
    video_stream_info: VideoStreamInfo,
    linesize: [i32; 7],
}

impl Drop for SwsScaler {
    fn drop(&mut self) {
        unsafe {
            sws_freeContext(self.sws_ctx);
        }
    }
}

const PIX_FMT_SIZE: usize = size_of::<i32>();

impl SwsScaler {
    unsafe fn init_sws_context(
        video_stream_info: &VideoStreamInfo,
        source_pix_fmt: AVPixelFormat,
        target_width: i32,
        target_height: i32,
    ) -> *mut SwsContext {
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

            sws_getContext(
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
            )
        }
    }

    fn calculate_linesize(width: i32) -> [i32; 7] {
        let mut linesize = [0; 7];
        linesize[0] = width * PIX_FMT_SIZE as i32;
        linesize
    }

    fn new(video_stream_info: VideoStreamInfo) -> Self {
        let frame_data_len =
            video_stream_info.width as usize * video_stream_info.height as usize * PIX_FMT_SIZE;

        let linesize: [i32; 7] = Self::calculate_linesize(video_stream_info.width);
        SwsScaler {
            linesize,
            frame_data_len,
            height: video_stream_info.height,
            width: video_stream_info.width,
            sws_ctx: std::ptr::null_mut(),
            options: None,
            video_stream_info,
        }
    }

    unsafe fn reinit_sws_context(
        &mut self,
        pix_fmt: AVPixelFormat,
        options: Option<FrameConvertOptions>,
    ) {
        if !self.sws_ctx.is_null() {
            sws_freeContext(self.sws_ctx);
        }

        let new_width = options.map_or(self.video_stream_info.width, |o| o.resize.width as i32);
        let new_height = options.map_or(self.video_stream_info.height, |o| o.resize.height as i32);

        self.sws_ctx =
            Self::init_sws_context(&self.video_stream_info, pix_fmt, new_width, new_height);
        self.width = new_width;
        self.height = new_height;
        self.options = options;

        self.frame_data_len = new_width as usize * new_height as usize * PIX_FMT_SIZE;
        self.linesize = Self::calculate_linesize(new_width);
    }

    unsafe fn convert(
        &mut self,
        options: Option<FrameConvertOptions>,
        source_frame: *mut AVFrame,
        rgba_dst: &mut [u8],
    ) -> Result<()> {
        let source_pix_fmt: AVPixelFormat = std::mem::transmute((*source_frame).format);
        if self.sws_ctx.is_null() || self.options != options {
            self.reinit_sws_context(source_pix_fmt, options);
        }

        let ret = sws_scale(
            self.sws_ctx,
            (*source_frame).data.as_ptr().cast::<*const u8>(),
            (*source_frame).linesize.as_ptr(),
            0,
            self.video_stream_info.height,
            &rgba_dst.as_mut_ptr(),
            self.linesize.as_ptr(),
        );

        if ret < 0 {
            return Err(FFramesMediaError::LibAVAudioDecodingError((
                ret,
                "Error while converting/scaling frame".to_string(),
            )));
        }

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
    pkt: *mut AVPacket,
    custom_time_base: AVRational,
    duration_in_frames: i64,
    /// The offset of the previous `decode_up_to` call, in `custom_time_base` units.
    last_offset: Option<i64>,
    /// A null packet has been sent; receive delayed frames until decoder EOF.
    draining: bool,
    /// The target frame holds a frame decoded since the last seek.
    has_decoded_frame: bool,
}

unsafe impl Send for FFmpegDecoder {}
unsafe impl Sync for FFmpegDecoder {}

struct ScaledFrameImage {
    pts: i64,
    data: Vec<u8>,
}

#[derive(Debug)]
pub struct FFmpegFrameBuf {
    resource_name: String,
    latest_av_frame: *mut AVFrame,
    video_stream_info: VideoStreamInfo,
    // This is not a safe operation but it gives a dramatic performance improvement
    // for the updating the underlying datavec. So we are relying on the constraint that
    // the decoder can operate (thus write the datavec) only within the frame, but
    // the frame can be sent and read outside the dnecoder
    data_buf: Arc<UnsafeCell<VecDeque<ScaledFrameImage>>>,
    sws_scaler: UnsafeCell<SwsScaler>,
}

impl Drop for FFmpegFrameBuf {
    fn drop(&mut self) {
        unsafe {
            if !self.latest_av_frame.is_null() {
                av_frame_unref(self.latest_av_frame);
                av_frame_free(&raw mut self.latest_av_frame);
            }

            if let Some(queue) = self.data_buf.get().as_mut() {
                queue.clear();
            }
        }
    }
}

/// Creates svgr preloaded image data with correctly blended color.
///
/// `rgba_data` borrows a slot of the decoder's ring buffer (with a fabricated
/// `'static` lifetime) that is recycled by later decodes, so the returned
/// image always owns a copy of the pixels.
fn create_preloaded_image(
    source_fmt: AVPixelFormat,
    resource_name: String,
    width: u32,
    height: u32,
    rgba_data: &'static [u8],
) -> Arc<PreloadedImageData> {
    let has_alpha_channel = unsafe {
        let desc = av_pix_fmt_desc_get(source_fmt);

        !desc.is_null() && (*desc).flags & AV_PIX_FMT_FLAG_ALPHA as u64 != 0
    };

    if has_alpha_channel {
        // Pre-multiplies alpha and returns an owned copy.
        Arc::new(PreloadedImageData::new(
            resource_name,
            width,
            height,
            rgba_data,
        ))
    } else {
        // Opaque pixels are already final; only the copy is needed.
        Arc::new(PreloadedImageData {
            id: resource_name,
            data: std::borrow::Cow::Owned(rgba_data.to_vec()),
            width,
            height,
        })
    }
}

unsafe impl Send for FFmpegFrameBuf {}
unsafe impl Sync for FFmpegFrameBuf {}

impl FFmpegFrameBuf {
    fn alloc_data_vec(ctx: &SwsScaler) -> Vec<u8> {
        let frame_data_len = ctx.frame_data_len;
        let mut data_vec = Vec::with_capacity(frame_data_len * PIX_FMT_SIZE);
        data_vec.reserve(frame_data_len);
        data_vec.resize(frame_data_len, 0);
        data_vec
    }

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
                resource_name,
                #[allow(clippy::arc_with_non_send_sync)]
                data_buf: Arc::new(UnsafeCell::new(VecDeque::with_capacity(buf_size))),
                sws_scaler: UnsafeCell::new(sws_ctx),
            })
        }
    }

    unsafe fn write_new_frame(&self) -> Option<&'static mut ScaledFrameImage> {
        unsafe {
            let queue = self.data_buf.get().as_mut()?;
            let scaler = self.sws_scaler.get().as_ref()?;
            let latest_pts = (*self.latest_av_frame).pts;

            // fast path for the cpu renderer which will always have capacity 1
            if queue.capacity() == 1 && queue.len() == 1 {
                // the pts identifies the image (and its renderer cache entries), it has to
                // follow the pixels
                let image = queue.get_mut(0)?;
                image.pts = latest_pts;
                return Some(image);
            }

            if queue.len() == queue.capacity() {
                let mut last_buffer = queue.pop_front()?;

                if last_buffer.data.len() != scaler.frame_data_len {
                    last_buffer.data.set_len(scaler.frame_data_len);
                }

                last_buffer.pts = latest_pts;
                queue.push_back(last_buffer);
            } else {
                queue.push_back(ScaledFrameImage {
                    pts: latest_pts,
                    data: Self::alloc_data_vec(scaler),
                });
            }

            queue.get_mut(queue.len() - 1)
        }
    }

    pub fn get_stream_width(&self) -> u32 {
        unsafe { self.sws_scaler.get().as_ref() }
            .expect("Critical mememroy error: SWS scale is not allocated")
            .video_stream_info
            .width as u32
    }

    pub fn get_stream_height(&self) -> u32 {
        unsafe { self.sws_scaler.get().as_ref() }
            .expect("Critical mememroy error: SWS scale is not allocated")
            .video_stream_info
            .height as u32
    }

    pub fn get_width(&self) -> u32 {
        unsafe { self.sws_scaler.get().as_ref() }
            .expect("Critical mememroy error: SWS scale is not allocated")
            .width as u32
    }

    pub fn get_height(&self) -> u32 {
        unsafe { self.sws_scaler.get().as_ref() }
            .expect("Critical mememroy error: SWS scale is not allocated")
            .height as u32
    }

    /// Returns fframe image constructed from the underlying ffmpeg's frame
    /// # Safety
    /// This is a libav based function which involves C ffi cals
    /// In addition it is manually transmutes the pointer owned by the decoder to the fframes
    /// images so it should not be used outside of the rendering worker.
    pub unsafe fn convert_last_decoded_frame_into_svg_image(
        &self,
        options: Option<&FrameConvertOptions>,
    ) -> Result<Arc<PreloadedImageData>> {
        unsafe {
            let scaler = self.sws_scaler.get().as_mut().unwrap();
            // fast path if we have the image already decoded and converted last time
            // e.g. the same video frame is requested from multiple scenes
            match self.data_buf.get().as_ref().and_then(|q| q.back()) {
                Some(image) if image.pts == (*self.latest_av_frame).pts => {
                    return Ok(create_preloaded_image(
                        scaler.video_stream_info.pixel_format,
                        encode_video_resource(&self.resource_name, image.pts),
                        self.get_width(),
                        self.get_height(),
                        &image.data,
                    ));
                }
                _ => {}
            }

            let image = self.write_new_frame().unwrap();
            scaler.convert(options.copied(), self.latest_av_frame, &mut image.data)?;

            Ok(create_preloaded_image(
                scaler.video_stream_info.pixel_format,
                encode_video_resource(&self.resource_name, image.pts),
                scaler.width as u32,
                scaler.height as u32,
                &image.data,
            ))
        }
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

    pub fn get_decoded_image_in_buf(&self, pts: i64) -> Option<Arc<PreloadedImageData>> {
        let buf = unsafe { self.frame_buf.data_buf.get().as_ref() }?;
        let image = buf.iter().find(|i| i.pts == pts)?;

        Some(create_preloaded_image(
            self.frame_buf.video_stream_info.pixel_format,
            encode_video_resource(&self.frame_buf.resource_name, image.pts),
            self.frame_buf.get_width(),
            self.frame_buf.get_height(),
            &image.data,
        ))
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

            let video_stream_info = Self::open_codec_context(fmt_ctx)?;
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

            let duration_in_frames = av_rescale_q(
                video_stream_info.duration,
                video_stream_info.time_base,
                custom_time_base,
            );

            Ok(FFmpegDecoder {
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
                duration_in_frames,
                current_loop: 0,
                last_offset: None,
                draining: false,
                has_decoded_frame: false,
            })
        }
    }

    unsafe fn open_codec_context(fmt_ctx: *mut AVFormatContext) -> Result<VideoStreamInfo> {
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

            Ok(VideoStreamInfo {
                hw_pix_fmt,
                stream_index: video_stream_idx,
                codec_ctx: video_dec_ctx,
                width: (*video_dec_ctx).width,
                height: (*video_dec_ctx).height,
                pixel_format: (*video_dec_ctx).pix_fmt,
                time_base: (*stream).time_base,
                duration: stream_duration(fmt_ctx, stream),
                frame_rate: (*stream).r_frame_rate,
            })
        }
    }

    /// Seeks to the specified offset in the video stream (to the nearest keyframe)
    /// # Safety
    /// Generally safe but uses libav functions
    pub unsafe fn seek_to_offset(&mut self, offset: i64) -> Result<()> {
        unsafe {
            let timestamp = av_rescale_q(
                offset,
                self.custom_time_base,
                self.video_stream_info.time_base,
            );

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
            avcodec_flush_buffers(self.video_stream_info.codec_ctx);
            self.draining = false;
            self.has_decoded_frame = false;
            av_packet_unref(self.pkt);

            Ok(())
        }
    }

    /// Moves offset to the start of the video if need to loop
    /// # Safety
    /// Generally safe but uses libav functions
    pub unsafe fn adjust_offset_for_looping(&mut self, offset: i64) -> Result<i64> {
        if offset < self.duration_in_frames {
            return Ok(offset);
        }

        let new_loop_index = offset / self.duration_in_frames;
        let new_offset = offset % self.duration_in_frames;
        if new_loop_index == self.current_loop {
            return Ok(new_offset);
        }

        self.current_loop = new_loop_index;
        unsafe {
            self.seek_to_offset(new_offset)?;
        }

        Ok(new_offset)
    }

    /// If we are actually using hardware accelerated frames it is required to correctly
    /// move the data from the hardware surface to the software frame
    unsafe fn transfer_hardware_surface_data(&self, target_frame: *mut AVFrame) -> Result<()> {
        match self.video_stream_info.hw_pix_fmt {
            Some(hw_pix_fmt) if hw_pix_fmt as i32 == (*target_frame).format => {
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
            // holds the pixels.
            _ if target_frame != self.frame_buf.latest_av_frame => {
                av_frame_unref(self.frame_buf.latest_av_frame);
                av_frame_move_ref(self.frame_buf.latest_av_frame, target_frame);
            }
            _ => (),
        }

        Ok(())
    }

    /// Decodes the video stream up to the specified offset
    /// # Safety
    /// Generally safe but uses libav functions
    pub unsafe fn decode_up_to(&mut self, offset: i64) -> Result<bool> {
        unsafe {
            // Going back needs a seek, otherwise the newer frame that was already decoded
            // would be returned. Far jumps forward seek to the closest keyframe instead of
            // decoding every frame in between (frames can be requested out of order by
            // parallel renderers).
            let seek_ahead_frames = 2 * i64::from(self.custom_time_base.den.max(1));
            let needs_seek = self.last_offset.is_some_and(|last_offset| {
                offset < last_offset || offset - last_offset > seek_ahead_frames
            });
            self.last_offset = Some(offset);
            if needs_seek {
                self.seek_to_offset(offset)?;
            }

            let target_pts = av_rescale_q(
                offset,
                self.custom_time_base,
                self.video_stream_info.time_base,
            );

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
                let ret = avcodec_receive_frame(self.video_stream_info.codec_ctx, self.recv_frame);
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
                        let shows_last_frame =
                            self.has_decoded_frame && offset < self.duration_in_frames;
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
                let ret = avcodec_send_packet(self.video_stream_info.codec_ctx, packet);
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
            avcodec_free_context(&raw mut self.video_stream_info.codec_ctx);
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
