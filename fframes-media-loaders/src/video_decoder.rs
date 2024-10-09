use crate::error::Result;
use crate::FFramesMediaError;
use ffmpeg_sys_fframes::*;
use std::cell::UnsafeCell;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::ptr;
use std::rc::Rc;
use std::sync::Arc;
use usvgr::PreloadedImageData;

pub struct VideoStreamInfo {
    stream_index: i32,
    codec_ctx: *mut AVCodecContext,
    width: i32,
    height: i32,
    pixel_format: AVPixelFormat,
    time_base: AVRational,
}

struct SwsContextOptions {
    target_width: i32,
    target_height: i32,
}

pub struct SwsScaler {
    sws_ctx: *mut SwsContext,
    options: SwsContextOptions,
}

pub struct FFmpegDecoder {
    frame: Arc<FFmpegFrame>,
    fmt_ctx: *mut AVFormatContext,
    video_stream_info: VideoStreamInfo,
    pkt: *mut AVPacket,
    last_decoded_pts: i64,
    custom_time_base: AVRational,
}

#[derive(Debug, Clone)]
pub struct FFmpegFrame {
    pub width: i32,
    pub height: i32,
    frame_data_length: usize,
    av_frame: *mut AVFrame,
    data_vec: Rc<UnsafeCell<Vec<u8>>>,
    linesize: [i32; 7],
    sws_ctx: *mut SwsContext,
}

impl Drop for FFmpegFrame {
    fn drop(&mut self) {
        unsafe {
            av_frame_free(&mut self.av_frame);
            sws_freeContext(self.sws_ctx);
        }
    }
}

impl FFmpegFrame {
    unsafe fn new(video_stream_info: &VideoStreamInfo) -> Result<Self> {
        let av_frame = unsafe { av_frame_alloc() };
        if av_frame.is_null() {
            return Err(FFramesMediaError::LibAVAllocationError("frame"));
        }

        let sws_ctx = sws_getContext(
            video_stream_info.width,
            video_stream_info.height,
            video_stream_info.pixel_format,
            video_stream_info.width,
            video_stream_info.height,
            AVPixelFormat::AV_PIX_FMT_RGBA,
            SWS_FAST_BILINEAR,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        );
        if sws_ctx.is_null() {
            return Err(FFramesMediaError::LibAVAllocationError("sws_context"));
        }

        // we always convert images to rgba
        const PIX_FMT_SIZE: usize = size_of::<i32>();
        let frame_data_length =
            video_stream_info.width as usize * video_stream_info.height as usize * PIX_FMT_SIZE;

        let frame_size = (frame_data_length * PIX_FMT_SIZE) as usize;

        let linesize: [i32; 7] = [
            video_stream_info.width * PIX_FMT_SIZE as i32,
            0,
            0,
            0,
            0,
            0,
            0,
        ];

        let mut data_vec = Vec::with_capacity(frame_size);
        data_vec.reserve(frame_data_length as usize);

        Ok(FFmpegFrame {
            frame_data_length,
            width: video_stream_info.width,
            height: video_stream_info.height,
            sws_ctx,
            av_frame,
            data_vec: Rc::new(UnsafeCell::new(data_vec)),
            linesize,
        })
    }

    pub unsafe fn get_image(&self) -> Arc<PreloadedImageData> {
        // Convert the frame to RGBA
        sws_scale(
            self.sws_ctx,
            (*self.av_frame).data.as_ptr() as *const *const u8,
            (*self.av_frame).linesize.as_ptr(),
            0,
            self.height,
            [(*self.data_vec.get()).as_mut_ptr()].as_ptr(),
            self.linesize.as_ptr(),
        );

        let data_vec = self
            .data_vec
            .get()
            .as_mut()
            .expect("Failed precondition: frame datavec is not allocated");
        data_vec.set_len(self.frame_data_length);

        Arc::new(PreloadedImageData::new(
            format!("<<frame_{}>>", (*self.av_frame).pts),
            self.width as u32,
            self.height as u32,
            data_vec,
        ))
    }
}

impl FFmpegDecoder {
    pub fn get_raw_frame(&self) -> Arc<FFmpegFrame> {
        self.frame.clone()
    }

    pub unsafe fn new(src_filename: &PathBuf, target_fps: usize) -> Result<Self> {
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

        Ok(FFmpegDecoder {
            last_decoded_pts: -1,
            pkt,
            fmt_ctx,
            frame: FFmpegFrame::new(&video_stream_info)?.into(),
            video_stream_info,
            custom_time_base: AVRational {
                num: 1,
                den: target_fps as i32,
            },
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
        })
    }

    pub unsafe fn seek_to_offset(&mut self, offset: i64) -> Result<()> {
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
        avcodec_flush_buffers(self.video_stream_info.codec_ctx);
        Ok(())
    }

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
