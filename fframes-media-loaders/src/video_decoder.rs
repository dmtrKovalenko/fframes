use crate::error::Result;
use crate::video_types::FrameConvertOptions;
use crate::{FFramesMediaError, ResizeVideoFrame};
use ffmpeg_sys_fframes::*;
use std::cell::UnsafeCell;
use std::collections::VecDeque;
use std::ffi::CString;
use std::mem::size_of;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use std::sync::Arc;
use usvgr::PreloadedImageData;

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
    s.push_str(".jpg");
    s
}

pub fn decode_video_resource(s: &str) -> Option<(&str, i64)> {
    let s = s.strip_suffix(".jpg")?;
    let (resource, pts_str) = s.split_once(FFRAMES_VIDEO_PATH_TAG)?;
    Some((resource, pts_str.parse().ok()?))
}

#[derive(Debug, Clone, Copy)]
struct VideoStreamInfo {
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
    width: u32,
    height: u32,
    sws_ctx: *mut SwsContext,
    options: FrameConvertOptions,
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
        target_width: i32,
        target_height: i32,
    ) -> Result<*mut SwsContext> {
        unsafe {
            let flags = if video_stream_info.width > target_width
                || video_stream_info.height > target_height
            {
                SWS_BICUBIC // Downscaling
            } else if video_stream_info.width < target_width
                || video_stream_info.height < target_height
            {
                SWS_BILINEAR // Upscaling
            } else {
                0 // No scaling needed
            };

            Ok(sws_getContext(
                video_stream_info.width,
                video_stream_info.height,
                video_stream_info.pixel_format,
                target_width,
                target_height,
                AVPixelFormat::AV_PIX_FMT_RGBA,
                flags,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
            ))
        }
    }

    fn calculate_linesize(width: i32) -> [i32; 7] {
        let mut linesize = [0; 7];
        linesize[0] = width * PIX_FMT_SIZE as i32;
        linesize
    }

    fn new(video_stream_info: VideoStreamInfo) -> Result<Self> {
        let frame_data_len =
            video_stream_info.width as usize * video_stream_info.height as usize * PIX_FMT_SIZE;

        let linesize: [i32; 7] = Self::calculate_linesize(video_stream_info.width);
        Ok(SwsScaler {
            linesize,
            frame_data_len,
            height: video_stream_info.height as u32,
            width: video_stream_info.width as u32,
            sws_ctx: unsafe {
                SwsScaler::init_sws_context(
                    &video_stream_info,
                    video_stream_info.width,
                    video_stream_info.height,
                )?
            },
            options: FrameConvertOptions {
                resize: ResizeVideoFrame {
                    width: video_stream_info.width as u32,
                    height: video_stream_info.height as u32,
                },
            },
            video_stream_info,
        })
    }

    fn maybe_reinit_conversion_context(
        &mut self,
        options: &FrameConvertOptions,
    ) -> Result<*mut SwsContext> {
        if self.options != *options {
            unsafe {
                sws_freeContext(self.sws_ctx);

                self.sws_ctx = Self::init_sws_context(
                    &self.video_stream_info,
                    options.resize.width as i32,
                    options.resize.height as i32,
                )?;

                self.width = options.resize.width;
                self.height = options.resize.height;
                self.frame_data_len = self.width as usize * self.height as usize * PIX_FMT_SIZE;

                self.options = *options;
                self.linesize = Self::calculate_linesize(self.width as i32);
            }
        }

        Ok(self.sws_ctx)
    }
}

#[derive(Debug)]
pub struct FFmpegDecoder {
    pub current_loop: i64,
    frame_buf: Arc<FFmpegFrameBuf>,
    fmt_ctx: *mut AVFormatContext,
    video_stream_info: VideoStreamInfo,
    pkt: *mut AVPacket,
    custom_time_base: AVRational,
    duration_in_frames: i64,
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
    // This is not a safe operation but it gives a dramatic performance improvement
    // for the updating the underlying datavec. So we are relying on the constraint that
    // the decoder can operate (thus write the datavec) only within the frame, but
    // the frame can be sent and read outside the decoder
    data_buf: Arc<UnsafeCell<VecDeque<ScaledFrameImage>>>,
    sws_scaler: UnsafeCell<SwsScaler>,
}

impl Drop for FFmpegFrameBuf {
    fn drop(&mut self) {
        unsafe {
            if !self.latest_av_frame.is_null() {
                av_frame_unref(self.latest_av_frame);
                av_frame_free(&mut self.latest_av_frame);
            }

            if let Some(queue) = self.data_buf.get().as_mut() {
                queue.clear();
            }
        }
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
        let av_frame = unsafe { av_frame_alloc() };
        if av_frame.is_null() {
            return Err(FFramesMediaError::LibAVAllocationError("frame"));
        }

        let sws_ctx = SwsScaler::new(video_stream_info)?;
        Ok(FFmpegFrameBuf {
            latest_av_frame: av_frame,
            resource_name,
            #[allow(clippy::arc_with_non_send_sync)]
            data_buf: Arc::new(UnsafeCell::new(VecDeque::with_capacity(buf_size))),
            sws_scaler: UnsafeCell::new(sws_ctx),
        })
    }

    unsafe fn write_new_frame(&self) -> Option<&'static mut ScaledFrameImage> {
        unsafe {
            let queue = self.data_buf.get().as_mut()?;
            let scaler = self.sws_scaler.get().as_ref()?;
            let latest_pts = (*self.latest_av_frame).pts;

            // fast path for the cpu renderer which will always have capacity 1
            if queue.capacity() == 1 && queue.len() == 1 {
                return queue.get_mut(0);
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

    /// Reallocates the sws convertor and frame data vector if the options have changed
    /// # Safety
    /// Generally safe but uses libav functions
    pub unsafe fn reinit_sws_context(&self, options: &FrameConvertOptions) -> Result<()> {
        let scaler = unsafe {
            self.sws_scaler.get().as_mut().ok_or_else(|| {
                FFramesMediaError::LibAVAudioDecodingError((
                    0,
                    "Failed precondition: SwsScaler is not allocated".to_string(),
                ))
            })?
        };
        scaler.maybe_reinit_conversion_context(options)?;

        Ok(())
    }

    pub fn get_width(&self) -> u32 {
        unsafe { self.sws_scaler.get().as_ref() }
            .expect("Critical mememroy error: SWS scale is not allocated")
            .width
    }

    pub fn get_height(&self) -> u32 {
        unsafe { self.sws_scaler.get().as_ref() }
            .expect("Critical mememroy error: SWS scale is not allocated")
            .height
    }

    /// Returns fframe image constructed from the underlying ffmpeg's frame
    /// # Safety
    /// This is a libav based function which involes C ffi cals
    /// In addition it is manually transmutes the pointer owned by the decoder to the fframes
    /// images so it should not be used outside of the rendering worker.
    pub unsafe fn scale_and_return_latest_frame(&self) -> Arc<PreloadedImageData> {
        unsafe {
            let scaler = self.sws_scaler.get().as_ref().unwrap();
            match self.data_buf.get().as_ref().and_then(|q| q.back()) {
                Some(image) if image.pts == (*self.latest_av_frame).pts => {
                    return Arc::new(PreloadedImageData::new_blended(
                        encode_video_resource(&self.resource_name, image.pts),
                        self.get_width(),
                        self.get_height(),
                        &image.data,
                    ));
                }
                _ => {}
            }

            let image = self.write_new_frame().unwrap();
            sws_scale(
                scaler.sws_ctx,
                (*self.latest_av_frame).data.as_ptr() as *const *const u8,
                (*self.latest_av_frame).linesize.as_ptr(),
                0,
                scaler.video_stream_info.height,
                &image.data.as_mut_ptr(),
                scaler.linesize.as_ptr(),
            );

            Arc::new(PreloadedImageData::new_blended(
                encode_video_resource(&self.resource_name, image.pts),
                scaler.width,
                scaler.height,
                // Possible unsafety here because we assume that the frame vec is allocated
                // (and it actually is) till the end of the rendering worker lifetime and the data
                // inside a datavec is always valid.
                &image.data,
            ))
        }
    }

    /// Returns the timestamp of the frame in the native frame timebase
    /// # Safety
    /// This is a libav based function which involes C ffi cals
    pub unsafe fn get_pts(&self) -> i64 {
        unsafe { (*self.latest_av_frame).pts }
    }

    /// Returns the timestamp of the frame in seconds
    /// # Safety
    /// This is a libav based function which involes C ffi cals
    pub unsafe fn timestamp_seconds(&self) -> f64 {
        unsafe {
            let video_stream_info = &self.sws_scaler.get().as_ref().unwrap().video_stream_info;
            (*self.latest_av_frame).pts as f64 * av_q2d(video_stream_info.time_base)
        }
    }

    /// Returns the duration of the stream in frames
    /// # Safety
    /// This is a libav based function which involes C ffi cals
    pub unsafe fn get_stream_duration_in_frames(&self) -> f64 {
        unsafe {
            let video_stream_info = &self.sws_scaler.get().as_ref().unwrap().video_stream_info;
            video_stream_info.duration as f64 * av_q2d(video_stream_info.time_base)
        }
    }

    /// Returns the fps value of the stream
    /// Remember that the fps value is always a guess based on the stream timestamps
    pub fn get_stream_fps(&self) -> f64 {
        unsafe {
            let video_stream_info = &self.sws_scaler.get().as_ref().unwrap().video_stream_info;
            video_stream_info.frame_rate.num as f64 / video_stream_info.frame_rate.den as f64
        }
    }
}

impl FFmpegDecoder {
    pub fn get_raw_frame(&self) -> Arc<FFmpegFrameBuf> {
        Arc::clone(&self.frame_buf)
    }

    pub fn get_decoded_image_in_buf(&self, pts: i64) -> Option<Arc<PreloadedImageData>> {
        let buf = unsafe { self.frame_buf.data_buf.get().as_ref() }?;
        let image = buf.iter().find(|i| i.pts == pts)?;

        Some(Arc::new(PreloadedImageData::new_blended(
            encode_video_resource(&self.frame_buf.resource_name, image.pts),
            self.frame_buf.get_width(),
            self.frame_buf.get_height(),
            &image.data,
        )))
    }

    /// Creates a new decoder for the video file at the specified path
    /// # Safety
    /// Generally safe but uses libav functions
    pub unsafe fn new(path: &Path, target_fps: usize, buffer_size: usize) -> Result<Self> {
        unsafe {
            let filename = path
                .file_name()
                .ok_or_else(|| FFramesMediaError::MediaDirectoryProvided)?;
            let full_path_cstr = CString::new(path.as_os_str().as_bytes())?;

            let mut fmt_ctx: *mut AVFormatContext = ptr::null_mut();
            let ret = avformat_open_input(
                &mut fmt_ctx,
                full_path_cstr.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
            );

            if ret < 0 {
                if !fmt_ctx.is_null() {
                    avformat_close_input(&mut fmt_ctx);
                }
                return Err(FFramesMediaError::LibAVAudioDecodingError((
                    ret,
                    "Could not open input file".to_string(),
                )));
            }

            let ret = avformat_find_stream_info(fmt_ctx, ptr::null_mut());
            if ret < 0 {
                avformat_close_input(&mut fmt_ctx);
                return Err(FFramesMediaError::LibAVAudioDecodingError((
                    ret,
                    "Could not find stream information".to_string(),
                )));
            }

            let video_stream_info = Self::open_codec_context(fmt_ctx)?;
            let pkt = av_packet_alloc();
            if pkt.is_null() {
                avformat_close_input(&mut fmt_ctx);
                return Err(FFramesMediaError::LibAVAllocationError("packet"));
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
                frame_buf: Arc::new(FFmpegFrameBuf::new(
                    filename.to_string_lossy().to_string(),
                    video_stream_info,
                    buffer_size,
                )?),
                video_stream_info,
                custom_time_base,
                duration_in_frames,
                current_loop: 0,
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
                    "Could not find video stream information".to_string(),
                )));
            }
            let video_stream_idx = ret;

            let stream = *(*fmt_ctx).streams.offset(video_stream_idx as isize);
            let dec = avcodec_find_decoder((*(*stream).codecpar).codec_id);
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
                stream_index: video_stream_idx,
                codec_ctx: video_dec_ctx,
                width: (*video_dec_ctx).width,
                height: (*video_dec_ctx).height,
                pixel_format: (*video_dec_ctx).pix_fmt,
                time_base: (*stream).time_base,
                duration: (*stream).duration,
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

    /// Decodes the video stream up to the specified offset
    /// # Safety
    /// Generally safe but uses libav functions
    pub unsafe fn decode_up_to(&mut self, offset: i64) -> Result<bool> {
        unsafe {
            let target_pts = av_rescale_q(
                offset,
                self.custom_time_base,
                self.video_stream_info.time_base,
            );

            if (*self.frame_buf.latest_av_frame).pts >= target_pts {
                return Ok(true);
            }

            loop {
                // Clear packet before reading new frame
                av_packet_unref(self.pkt);

                let read_result = av_read_frame(self.fmt_ctx, self.pkt);
                if read_result < 0 {
                    return Ok(false);
                }

                // Use scope to ensure packet is always unreferenced
                // it is important to unref packet every time after av_read_frame is done
                let decoder_result: Result<_> = {
                    if (*self.pkt).stream_index == self.video_stream_info.stream_index {
                        let ret = avcodec_send_packet(self.video_stream_info.codec_ctx, self.pkt);
                        if ret < 0 {
                            return Err(FFramesMediaError::LibAVAudioDecodingError((
                                ret,
                                "Error submitting packet for decoding".to_string(),
                            )));
                        }

                        loop {
                            // Unref previous frame before receiving new one
                            // av_frame_unref(self.frame_buf.latest_av_frame);
                            let ret = avcodec_receive_frame(
                                self.video_stream_info.codec_ctx,
                                self.frame_buf.latest_av_frame,
                            );

                            match ret {
                                0 => {
                                    if (*self.frame_buf.latest_av_frame).pts >= target_pts {
                                        return Ok(true);
                                    }
                                }
                                val if val == AVERROR(EAGAIN) => {
                                    break;
                                }
                                _ => {
                                    av_frame_unref(self.frame_buf.latest_av_frame);
                                    av_packet_unref(self.pkt);

                                    return Err(FFramesMediaError::LibAVAudioDecodingError((
                                        ret,
                                        "Decoding error".to_string(),
                                    )));
                                }
                            }
                        }
                    }

                    Ok(false)
                };

                // Always unref packet after processing
                av_packet_unref(self.pkt);

                if let Ok(true) = decoder_result {
                    return Ok(true);
                }
            }
        }
    }
}

impl Drop for FFmpegDecoder {
    fn drop(&mut self) {
        unsafe {
            avcodec_free_context(&mut self.video_stream_info.codec_ctx);
            avformat_close_input(&mut self.fmt_ctx);
            av_packet_free(&mut self.pkt);
        }
    }
}
