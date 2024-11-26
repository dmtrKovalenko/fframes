use crate::ffmpeg_action;
use crate::renderer_error::{RenderEncodingError, RenderEncodingResult};
use crate::stream;
use ffmpeg_sys_fframes::AVPixelFormat;
use ffmpeg_sys_fframes::*;

#[derive(Clone)]
pub(crate) struct FrameFormatConvertor {
    pub(crate) tmp_frame: *mut AVFrame,
    pub(crate) sws_ctx: *mut SwsContext,
}

impl FrameFormatConvertor {
    pub(crate) unsafe fn new(video_stream: &stream::Stream) -> RenderEncodingResult<Self> {
        let sws_ctx = sws_getContext(
            (*video_stream.enc).width,
            (*video_stream.enc).height,
            AVPixelFormat::AV_PIX_FMT_YUV420P,
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
        (*tmp_frame).format = AVPixelFormat::AV_PIX_FMT_YUV420P as i32;
        (*tmp_frame).width = (*video_stream.enc).width;
        (*tmp_frame).height = (*video_stream.enc).height;

        ffmpeg_action!(
            av_frame_get_buffer(tmp_frame, 0),
            RenderEncodingError::CantAllocate("converter frame buffer".to_owned())
        );

        Ok(Self { tmp_frame, sws_ctx })
    }

    pub(crate) unsafe fn convert(&self, destination_frame: *mut AVFrame) -> *mut AVFrame {
        sws_scale(
            self.sws_ctx,
            (*self.tmp_frame).data.as_ptr() as *const *const u8,
            (*self.tmp_frame).linesize.as_ptr(),
            0,
            (*self.tmp_frame).height,
            (*destination_frame).data.as_ptr(),
            (*destination_frame).linesize.as_ptr(),
        );

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
    pub(crate) frame: *mut AVFrame,
    /// Used to store original yuv frame before converting it to the output pixel format.
    pub(crate) format_convertor: Option<FrameFormatConvertor>,
}

impl EncoderFrame {
    pub unsafe fn new(stream: &stream::Stream) -> RenderEncodingResult<Self> {
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
            frame,
            packet,
            format_convertor,
        })
    }

    #[inline(always)]
    pub(crate) fn get_rgb(pixmap: &[u8], i: usize) -> (i32, i32, i32) {
        let r = pixmap[4 * i] as i32;
        let g = pixmap[4 * i + 1] as i32;
        let b = pixmap[4 * i + 2] as i32;

        (r, g, b)
    }

    pub unsafe fn fill_from_audio_data(
        &mut self,
        frame_index: i64,
        audio_data: Vec<f32>,
    ) -> *mut AVFrame {
        let is_writable = av_frame_make_writable(self.frame);
        if is_writable < 0 {
            panic!("Can not reuse frame allocations");
        }

        (*self.frame).pts = frame_index;
        if audio_data.is_empty() {
            return self.frame;
        }

        let mut fltp_audio_data = audio_data
            .into_iter()
            .flat_map(|data| data.to_le_bytes())
            .collect::<Vec<u8>>();

        (*self.frame).data[0] = fltp_audio_data.as_mut_ptr();

        self.frame
    }

    pub unsafe fn fill_from_rgba_pixmap(
        &mut self,
        frame_index: i64,
        rgba_pixels: &[u8],
    ) -> *mut AVFrame {
        if let Some(converter) = self.format_convertor.as_ref() {
            Self::fill_yuv420_from_rgba_pixmap(converter.tmp_frame, frame_index, rgba_pixels);
            converter.convert(self.frame)
        } else {
            Self::fill_yuv420_from_rgba_pixmap(self.frame, frame_index, rgba_pixels)
        }
    }

    /// We support only yuv420 format as for now so we can pretty efficiently convert the bitmap buffer.
    /// yuv420 represented by y per each pixel and uv (cb and cr) per each 2x2 pixel block.
    #[allow(clippy::precedence)]
    pub unsafe fn fill_yuv420_from_rgba_pixmap(
        av_frame: *mut AVFrame,
        frame_index: i64,
        rgba_pixels: &[u8],
    ) -> *mut AVFrame {
        let is_writable = av_frame_make_writable(av_frame);
        if is_writable < 0 {
            panic!("Can not reuse frame allocations");
        }

        let height = (*av_frame).height as usize;
        let width = (*av_frame).width as usize;

        // an important note that linesize here can be different from the width of an image so it is required to fill the buffer correctly.
        let frame_size: usize = height * (*av_frame).linesize[0] as usize + width;

        let y_pixels = std::slice::from_raw_parts_mut((*av_frame).data[0], frame_size);
        let cb_pixels = std::slice::from_raw_parts_mut((*av_frame).data[1], frame_size / 2);
        let cr_pixels = std::slice::from_raw_parts_mut((*av_frame).data[2], frame_size / 2);

        for y in 0..height {
            for x in 0..width {
                let (r, g, b) = EncoderFrame::get_rgb(rgba_pixels, y * width + x);

                // use a linesize to get the correct index for the pixel as it can differ for different dimensions.
                y_pixels[y * (*av_frame).linesize[0] as usize + x] =
                    (16 + (66 * r + 129 * g + 25 * b) >> 8) as u8;

                if y % 2 == 0 && x % 2 == 0 {
                    // the bounds are 1/4 of the image size
                    let x = x / 2;
                    let y = y / 2;

                    cb_pixels[y * (*av_frame).linesize[1] as usize + x] =
                        (128 + (-38 * r - 74 * g + 112 * b >> 8)) as u8;
                    cr_pixels[y * (*av_frame).linesize[2] as usize + x] =
                        (128 + (112 * r - 94 * g - 18 * b >> 8)) as u8;
                }
            }
        }
        (*av_frame).pts = frame_index;
        av_frame
    }
}

impl Drop for EncoderFrame {
    fn drop(&mut self) {
        unsafe {
            if !self.frame.is_null() {
                av_frame_unref(self.frame);
                av_frame_free(&mut self.frame);
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
