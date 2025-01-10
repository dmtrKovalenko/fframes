use crate::error::Result;
use crate::video_types::FrameConvertOptions;
use crate::{FFramesMediaError, ResizeVideoFrame};
use ffmpeg_sys_fframes::*;
use std::cell::{RefCell, UnsafeCell};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use std::rc::Rc;
use std::sync::Arc;
use usvgr::PreloadedImageData;

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
        let flags = if video_stream_info.width > target_width
            || video_stream_info.height > target_height
        {
            SWS_BICUBIC // Downscaling
        } else if video_stream_info.width < target_width || video_stream_info.height < target_height
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

    fn calculate_linesize(width: i32) -> [i32; 7] {
        let mut linesize = [0; 7];
        linesize[0] = width * PIX_FMT_SIZE as i32;
        linesize
    }

    fn new(video_stream_info: &VideoStreamInfo) -> Result<Self> {
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
                    video_stream_info,
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
            video_stream_info: *video_stream_info,
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

        // safe to unwrap because of the is_none check above
        Ok(self.sws_ctx)
    }
}

pub struct FFmpegDecoder {
    pub current_loop: i64,
    frame: Arc<FFmpegFrame>,
    fmt_ctx: *mut AVFormatContext,
    video_stream_info: VideoStreamInfo,
    pkt: *mut AVPacket,
    custom_time_base: AVRational,
    duration_in_frames: i64,
}

#[derive(Debug)]
pub struct FFmpegFrame {
    av_frame: *mut AVFrame,
    data_vec: Rc<UnsafeCell<Vec<u8>>>,
    sws_scaler: RefCell<SwsScaler>,
}

impl Drop for FFmpegFrame {
    fn drop(&mut self) {
        unsafe {
            av_frame_free(&mut self.av_frame);
        }
    }
}

impl FFmpegFrame {
    fn alloc_data_vec(ctx: &SwsScaler) -> Vec<u8> {
        let frame_data_len = ctx.frame_data_len;
        let mut data_vec = Vec::with_capacity(frame_data_len * PIX_FMT_SIZE);
        data_vec.reserve(frame_data_len);
        data_vec
    }

    unsafe fn new(video_stream_info: &VideoStreamInfo) -> Result<Self> {
        let av_frame = unsafe { av_frame_alloc() };
        if av_frame.is_null() {
            return Err(FFramesMediaError::LibAVAllocationError("frame"));
        }

        let sws_ctx = SwsScaler::new(video_stream_info)?;
        Ok(FFmpegFrame {
            av_frame,
            data_vec: Rc::new(UnsafeCell::new(Self::alloc_data_vec(&sws_ctx))),
            sws_scaler: std::cell::RefCell::new(sws_ctx),
        })
    }

    /// Reinitializes the sws context and the data vector if the options have changed
    /// # Safety
    /// This is a libav based function which involes C ffi cals
    pub unsafe fn reinit_sws_context(&self, options: &FrameConvertOptions) -> Result<()> {
        self.sws_scaler
            .borrow_mut()
            .maybe_reinit_conversion_context(options)?;

        let current_len = self.data_vec.get().as_mut().map(|v| v.len());
        let new_len = self.sws_scaler.borrow().frame_data_len;

        let realloc_vec = || {
            self.data_vec
                .get()
                .replace(Self::alloc_data_vec(&self.sws_scaler.borrow()));
        };

        match (current_len, new_len) {
            (None, _) => realloc_vec(),
            (Some(new_len), datalen) if new_len != datalen => realloc_vec(),
            _ => {}
        }

        Ok(())
    }

    pub fn get_width(&self) -> u32 {
        self.sws_scaler.borrow().width
    }

    pub fn get_height(&self) -> u32 {
        self.sws_scaler.borrow().height
    }

    /// Returns fframe image constructed from the underlying ffmpeg's frame
    /// # Safety
    /// This is a libav based function which involes C ffi cals
    /// In addition it is manually transmutes the pointer owned by the decoder to the fframes
    /// images so it should not be used outside of the rendering worker.
    pub unsafe fn get_image(&self) -> Arc<PreloadedImageData> {
        let scaler = self.sws_scaler.borrow();
        // Convert the frame to RGBA
        sws_scale(
            scaler.sws_ctx,
            (*self.av_frame).data.as_ptr() as *const *const u8,
            (*self.av_frame).linesize.as_ptr(),
            0,
            scaler.video_stream_info.height,
            [(*self.data_vec.get()).as_mut_ptr()].as_ptr(),
            scaler.linesize.as_ptr(),
        );

        let data_vec = self
            .data_vec
            .get()
            .as_mut()
            .expect("Failed precondition: frame datavec is not allocated");
        data_vec.set_len(scaler.frame_data_len);

        Arc::new(PreloadedImageData::new_blended(
            format!("<<frame_{}>>", (*self.av_frame).pts),
            scaler.width,
            scaler.height,
            // Possible unsafety here because we assume that the frame vec is allocated
            // (and it actually is) till the end of the rendering worker lifetime and the data
            // inside a datavec is always valid.
            data_vec,
        ))
    }

    /// Returns the timestamp of the frame in seconds
    /// # Safety
    /// This is a libav based function which involes C ffi cals
    pub unsafe fn timestamp_seconds(&self) -> f64 {
        let video_stream_info = self.sws_scaler.borrow().video_stream_info;
        (*self.av_frame).pts as f64 * av_q2d(video_stream_info.time_base)
    }

    /// Returns the duration of the stream in frames
    /// # Safety
    /// This is a libav based function which involes C ffi cals
    pub unsafe fn get_stream_duration_in_frames(&self) -> f64 {
        let video_stream_info = self.sws_scaler.borrow().video_stream_info;
        video_stream_info.duration as f64 * av_q2d(video_stream_info.time_base)
    }

    /// Returns the fps value of the stream
    /// Remember that the fps value is always a guess based on the stream timestamps
    pub fn get_stream_fps(&self) -> f64 {
        let video_stream_info = self.sws_scaler.borrow().video_stream_info;
        video_stream_info.frame_rate.num as f64 / video_stream_info.frame_rate.den as f64
    }
}

impl FFmpegDecoder {
    pub fn get_raw_frame(&self) -> Arc<FFmpegFrame> {
        self.frame.clone()
    }

    /// Creates new libav based video decoder pointing to the file at the given path
    /// # Safety
    /// This is a libav based function which involes C ffi cals
    pub unsafe fn new(src_filename: &Path, target_fps: usize) -> Result<Self> {
        let src_filename_c = CString::new(src_filename.as_os_str().as_bytes())?;

        let mut fmt_ctx: *mut AVFormatContext = ptr::null_mut();
        let ret = avformat_open_input(
            &mut fmt_ctx,
            src_filename_c.as_ptr(),
            ptr::null_mut(),
            ptr::null_mut(),
        );

        if ret < 0 {
            return Err(FFramesMediaError::LibAVAudioDecodingError((
                ret,
                "Could not open input file".to_string(),
            )));
        }

        let ret = avformat_find_stream_info(fmt_ctx, ptr::null_mut());
        if ret < 0 {
            return Err(FFramesMediaError::LibAVAudioDecodingError((
                ret,
                "Could not find stream information".to_string(),
            )));
        }

        let video_stream_info = Self::open_codec_context(fmt_ctx)?;

        let frame = av_frame_alloc();
        if frame.is_null() {
            return Err(FFramesMediaError::LibAVAllocationError("frame"));
        }

        let pkt = av_packet_alloc();
        if pkt.is_null() {
            return Err(FFramesMediaError::LibAVAllocationError("packet"));
        }

        let custom_time_base = {
            AVRational {
                num: 1,
                den: target_fps as i32,
            }
        };

        let duration_in_frames = av_rescale_q(
            video_stream_info.duration,
            video_stream_info.time_base,
            custom_time_base,
        );

        Ok(FFmpegDecoder {
            pkt,
            fmt_ctx,
            frame: FFmpegFrame::new(&video_stream_info)?.into(),
            video_stream_info,
            custom_time_base,
            duration_in_frames,
            current_loop: 0,
        })
    }

    unsafe fn open_codec_context(fmt_ctx: *mut AVFormatContext) -> Result<VideoStreamInfo> {
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

    /// Seeks the video context to the closes keyframe of the offset in target timebase frame
    /// # Safety
    /// This is a libav based function which involes C ffi cals
    pub unsafe fn seek_to_offset(&mut self, offset: i64) -> Result<()> {
        let offset = self
            .duration_in_frames
            .checked_rem(offset)
            .unwrap_or(offset);
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

        (*self.frame.av_frame).pts = -1;
        avcodec_flush_buffers(self.video_stream_info.codec_ctx);

        Ok(())
    }

    /// Adjusts the offset to the correct frame in the video stream
    /// # Safety
    /// This is a libav based function which involes C ffi cals
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
        self.seek_to_offset(new_offset)?;

        Ok(new_offset)
    }

    /// Decodes the video stream up the to target offset in the custom time base
    /// # Safety
    /// This is a libav based function which involes C ffi cals
    pub unsafe fn decode_up_to(&mut self, offset: i64) -> Result<bool> {
        let target_pts = av_rescale_q(
            offset,
            self.custom_time_base,
            self.video_stream_info.time_base,
        );

        if (*self.frame.av_frame).pts >= target_pts {
            return Ok(true);
        }

        loop {
            if av_read_frame(self.fmt_ctx, self.pkt) < 0 {
                return Ok(false);
            }

            let is_video_packet = (*self.pkt).stream_index == self.video_stream_info.stream_index;
            if is_video_packet {
                let ret = avcodec_send_packet(self.video_stream_info.codec_ctx, self.pkt);
                if ret < 0 {
                    av_packet_unref(self.pkt);
                    return Err(FFramesMediaError::LibAVAudioDecodingError((
                        ret,
                        "Error submitting packet for decoding".to_string(),
                    )));
                }

                // now we need to decode all frames in the packet in the lookup for the target frame
                loop {
                    let ret = avcodec_receive_frame(
                        self.video_stream_info.codec_ctx,
                        self.frame.av_frame,
                    );
                    match ret {
                        0 => {
                            if (*self.frame.av_frame).pts >= target_pts {
                                return Ok(true);
                            }
                        }
                        val if val == AVERROR(EAGAIN) => {
                            // We've processed all available output from the current packet
                            // Break the inner loop to read the next packet
                            break;
                        }
                        _ => {
                            av_packet_unref(self.pkt);
                            return Err(FFramesMediaError::LibAVAudioDecodingError((
                                ret,
                                "Decoding error".to_string(),
                            )));
                        }
                    }
                }

                av_packet_unref(self.pkt);
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
