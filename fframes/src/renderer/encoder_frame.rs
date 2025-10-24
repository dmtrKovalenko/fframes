use super::renderer_error::{RenderEncodingError, RenderEncodingResult};
use super::stream;
use crate::ffmpeg_action;
use crate::media::ffmpeg_sys_fframes::*;

#[derive(Clone)]
pub(crate) struct FrameFormatConvertor {
    pub(crate) tmp_frame: *mut AVFrame,
    pub(crate) sws_ctx: *mut SwsContext,
}

impl FrameFormatConvertor {
    pub(crate) unsafe fn new(video_stream: &stream::Stream) -> RenderEncodingResult<Self> {
        unsafe {
            let sws_ctx = sws_getContext(
                (*video_stream.enc).width,
                (*video_stream.enc).height,
                AVPixelFormat::AV_PIX_FMT_RGBA,
                (*video_stream.enc).width,
                (*video_stream.enc).height,
                (*video_stream.enc).pix_fmt,
                SWS_BICUBIC,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );

            if sws_ctx.is_null() {
                return Err(RenderEncodingError::Internal(
                    "Can not allocate sws".to_owned(),
                ));
            }

            let tmp_frame = av_frame_alloc();
            // The data fro the frame is always RGBA coming directly from the pixmap
            (*tmp_frame).format = AVPixelFormat::AV_PIX_FMT_RGBA as i32;
            (*tmp_frame).width = (*video_stream.enc).width;
            (*tmp_frame).height = (*video_stream.enc).height;

            ffmpeg_action!(
                av_frame_get_buffer(tmp_frame, 0),
                RenderEncodingError::CantAllocate("converter frame buffer".to_owned())
            );

            Ok(Self { tmp_frame, sws_ctx })
        }
    }

    pub(crate) unsafe fn convert(&self, destination_frame: *mut AVFrame) -> *mut AVFrame {
        unsafe {
            sws_scale(
                self.sws_ctx,
                (*self.tmp_frame).data.as_ptr() as *const *const u8,
                (*self.tmp_frame).linesize.as_ptr(),
                0,
                (*self.tmp_frame).height,
                (*destination_frame).data.as_ptr(),
                (*destination_frame).linesize.as_ptr(),
            );
        }

        destination_frame
    }
}

impl Drop for FrameFormatConvertor {
    fn drop(&mut self) {
        unsafe {
            sws_freeContext(self.sws_ctx);
            av_frame_free(&mut self.tmp_frame);
        }
    }
}

#[derive(Clone)]
pub struct EncoderFrame {
    pub(crate) packet: *mut AVPacket,
    pub(crate) av_frame: *mut AVFrame,
    /// Used to store original yuv frame before converting it to the output pixel format.
    pub(crate) format_convertor: Option<FrameFormatConvertor>,
}

impl EncoderFrame {
    pub unsafe fn set_pts(&mut self, pts: impl Into<i64>) {
        unsafe {
            (*self.av_frame).pts = pts.into();
        }
    }

    pub unsafe fn new(stream: &stream::Stream) -> RenderEncodingResult<Self> {
        unsafe {
            let frame = av_frame_alloc();
            let packet = av_packet_alloc();
            let mut format_convertor = None;

            match stream.variant {
                stream::StreamVariant::Video => {
                    (*frame).format = (*stream.enc).pix_fmt as i32;
                    (*frame).width = (*stream.enc).width;
                    (*frame).height = (*stream.enc).height;

                    format_convertor = ((*stream.enc).pix_fmt != AVPixelFormat::AV_PIX_FMT_YUV420P)
                        .then(|| FrameFormatConvertor::new(stream))
                        .transpose()?;
                }
                stream::StreamVariant::Audio(_) => {
                    av_channel_layout_copy(&mut (*frame).ch_layout, &(*stream.enc).ch_layout);

                    (*frame).format = (*stream.enc).sample_fmt as i32;
                    (*frame).ch_layout = (*stream.enc).ch_layout;
                    (*frame).sample_rate = (*stream.enc).sample_rate;
                    (*frame).nb_samples = if ((*(*stream.enc).codec).capabilities
                        & AV_CODEC_CAP_VARIABLE_FRAME_SIZE as i32)
                        != 0
                    {
                        10000
                    } else {
                        (*stream.enc).frame_size
                    };
                }
            }

            ffmpeg_action!(
                av_frame_get_buffer(frame, 0),
                RenderEncodingError::CantAllocate("frame buffer".to_owned())
            );

            Ok(EncoderFrame {
                av_frame: frame,
                packet,
                format_convertor,
            })
        }
    }

    pub unsafe fn fill_from_audio_data(
        &mut self,
        frame_index: i64,
        audio_data: Vec<f32>,
    ) -> *mut AVFrame {
        let is_writable = unsafe { av_frame_make_writable(self.av_frame) };
        if is_writable < 0 {
            panic!("Can not reuse frame allocations");
        }

        unsafe {
            (*self.av_frame).pts = frame_index;
        }

        if audio_data.is_empty() {
            return self.av_frame;
        }

        let fltp_audio_bytes = audio_data
            .into_iter()
            .flat_map(|data| data.to_le_bytes())
            .collect::<Vec<u8>>();

        unsafe {
            // Copy data into the AVFrame's allocated buffer instead of assigning pointer
            let frame_buffer = (*self.av_frame).data[0];
            let buffer_size = (*self.av_frame).linesize[0] as usize;
            let copy_size = fltp_audio_bytes.len().min(buffer_size);

            std::ptr::copy_nonoverlapping(
                fltp_audio_bytes.as_ptr(),
                frame_buffer,
                copy_size
            );

            // Update the actual number of samples in the frame
            (*self.av_frame).nb_samples = (copy_size / 4) as i32; // 4 bytes per f32
        }

        self.av_frame
    }

    pub unsafe fn fill_from_rgba_pixmap(&mut self, rgba_pixels: &[u8]) -> *mut AVFrame {
        unsafe {
            if let Some(converter) = self.format_convertor.as_ref() {
                // in case we don't use fframes conversion path we use a temporary frame which is set
                // to the planar RGBA 8888 format and then use sws_scale to convert to the output fmt
                (*converter.tmp_frame).data[0] = rgba_pixels.as_ptr() as *mut u8; // sws_scale doesn't do any mutations when converting from
                converter.convert(self.av_frame)
            } else {
                Self::fill_yuv420_from_rgba_pixmap(self.av_frame, rgba_pixels)
            }
        }
    }

    pub unsafe fn fill_yuv420_from_rgba_pixmap(
        frame: *mut AVFrame,
        rgba_pixels: &[u8],
    ) -> *mut AVFrame {
        unsafe {
            let is_writable = av_frame_make_writable(frame);
            if is_writable < 0 {
                panic!("Can not reuse frame allocations");
            }

            super::pix_fmt::fill_yuv420_from_rgba_pixmap_accelerated(
                (*frame).width,
                (*frame).height,
                (*frame).linesize[0],
                (*frame).linesize[1],
                (*frame).linesize[2],
                rgba_pixels,
                (*frame).data[0],
                (*frame).data[1],
                (*frame).data[2],
            );

            frame
        }
    }
}

impl Drop for EncoderFrame {
    fn drop(&mut self) {
        unsafe {
            if !self.av_frame.is_null() {
                av_frame_unref(self.av_frame);
                av_frame_free(&mut self.av_frame);
            }
            if !self.packet.is_null() {
                av_packet_unref(self.packet);
                av_packet_free(&mut self.packet);
            }
        }
    }
}

unsafe impl Send for EncoderFrame {}
unsafe impl Sync for EncoderFrame {}
