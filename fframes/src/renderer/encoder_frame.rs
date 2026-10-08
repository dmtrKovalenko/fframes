use super::frame_export::set_sws_output_matrix;
use super::pix_fmt::YuvMatrix;
use super::renderer_error::{RenderEncodingError, RenderEncodingResult};
use super::stream;
use crate::ffmpeg_action;
use crate::media::ffmpeg_sys_fframes::AVSampleFormat::*;
use crate::media::ffmpeg_sys_fframes::SwsFlags::SWS_BICUBIC;
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
                SWS_BICUBIC as i32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );

            if sws_ctx.is_null() {
                return Err(RenderEncodingError::Internal(
                    "Can not allocate sws".to_owned(),
                ));
            }
            if let Err(err) = set_sws_output_matrix(sws_ctx, stream_matrix(video_stream)) {
                sws_freeContext(sws_ctx);
                return Err(err);
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
                (*self.tmp_frame).data.as_ptr().cast::<*const u8>(),
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
            av_frame_free(&raw mut self.tmp_frame);
        }
    }
}

#[derive(Clone)]
pub struct EncoderFrame {
    pub(crate) packet: *mut AVPacket,
    pub(crate) av_frame: *mut AVFrame,
    /// Used to store original yuv frame before converting it to the output pixel format.
    pub(crate) format_convertor: Option<FrameFormatConvertor>,
    /// The matrix the stream is tagged with, which RGBA pixels are converted with.
    pub(crate) matrix: YuvMatrix,
}

/// The matrix the encoder of `stream` tagged the stream with.
unsafe fn stream_matrix(stream: &stream::Stream) -> YuvMatrix {
    if unsafe { (*stream.enc).colorspace } == AVColorSpace::AVCOL_SPC_BT709 {
        YuvMatrix::Bt709
    } else {
        YuvMatrix::Bt601
    }
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
                    av_channel_layout_copy(
                        &raw mut (*frame).ch_layout,
                        &raw const (*stream.enc).ch_layout,
                    );

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
                matrix: stream_matrix(stream),
            })
        }
    }

    /// Writes one block of stereo audio in the encoder's sample format and channel count.
    // FFmpeg allocates the sample planes with `av_frame_get_buffer` alignment.
    #[allow(clippy::cast_ptr_alignment)]
    pub unsafe fn fill_from_stereo(
        &mut self,
        pts: i64,
        left: &[f32],
        right: &[f32],
    ) -> RenderEncodingResult<*mut AVFrame> {
        unsafe {
            if av_frame_make_writable(self.av_frame) < 0 {
                return Err(RenderEncodingError::CantAllocate(
                    "writable audio frame".to_owned(),
                ));
            }

            let frame = self.av_frame;
            let n = left.len().min(right.len());
            (*frame).pts = pts;
            (*frame).nb_samples = n as i32;
            let channels = (*frame).ch_layout.nb_channels.max(1) as usize;
            let sample = |channel: usize, i: usize| match channels {
                1 => f32::midpoint(left[i], right[i]),
                _ if channel == 0 => left[i],
                _ if channel == 1 => right[i],
                // Additional channels of a larger layout stay silent.
                _ => 0.,
            };
            let to_i16 = |v: f32| (v.clamp(-1., 1.) * 32767.).round() as i16;
            let to_i32 = |v: f32| (f64::from(v.clamp(-1., 1.)) * 2_147_483_647.).round() as i32;

            let format: AVSampleFormat = std::mem::transmute((*frame).format);
            match format {
                AV_SAMPLE_FMT_FLTP => {
                    for c in 0..channels {
                        let plane = std::slice::from_raw_parts_mut(
                            (*frame).extended_data.add(c).read().cast::<f32>(),
                            n,
                        );
                        (0..n).for_each(|i| plane[i] = sample(c, i));
                    }
                }
                AV_SAMPLE_FMT_FLT => {
                    let data = std::slice::from_raw_parts_mut(
                        (*frame).data[0].cast::<f32>(),
                        n * channels,
                    );
                    (0..n).for_each(|i| {
                        (0..channels).for_each(|c| data[i * channels + c] = sample(c, i));
                    });
                }
                AV_SAMPLE_FMT_S16P => {
                    for c in 0..channels {
                        let plane = std::slice::from_raw_parts_mut(
                            (*frame).extended_data.add(c).read().cast::<i16>(),
                            n,
                        );
                        (0..n).for_each(|i| plane[i] = to_i16(sample(c, i)));
                    }
                }
                AV_SAMPLE_FMT_S16 => {
                    let data = std::slice::from_raw_parts_mut(
                        (*frame).data[0].cast::<i16>(),
                        n * channels,
                    );
                    (0..n).for_each(|i| {
                        (0..channels).for_each(|c| data[i * channels + c] = to_i16(sample(c, i)));
                    });
                }
                AV_SAMPLE_FMT_S32P => {
                    for c in 0..channels {
                        let plane = std::slice::from_raw_parts_mut(
                            (*frame).extended_data.add(c).read().cast::<i32>(),
                            n,
                        );
                        (0..n).for_each(|i| plane[i] = to_i32(sample(c, i)));
                    }
                }
                AV_SAMPLE_FMT_S32 => {
                    let data = std::slice::from_raw_parts_mut(
                        (*frame).data[0].cast::<i32>(),
                        n * channels,
                    );
                    (0..n).for_each(|i| {
                        (0..channels).for_each(|c| data[i * channels + c] = to_i32(sample(c, i)));
                    });
                }
                format => {
                    return Err(RenderEncodingError::Internal(format!(
                        "audio sample format {format:?} is not supported, use fltp, flt, s16 or s32"
                    )));
                }
            }

            Ok(frame)
        }
    }

    pub unsafe fn fill_from_rgba_pixmap(&mut self, rgba_pixels: &[u8]) -> *mut AVFrame {
        unsafe {
            if let Some(converter) = self.format_convertor.as_ref() {
                // in case we don't use fframes conversion path we use a temporary frame which is set
                // to the planar RGBA 8888 format and then use sws_scale to convert to the output fmt
                (*converter.tmp_frame).data[0] = rgba_pixels.as_ptr().cast_mut(); // sws_scale doesn't do any mutations when converting from
                converter.convert(self.av_frame)
            } else {
                Self::fill_yuv420_from_rgba_pixmap(self.av_frame, self.matrix, rgba_pixels)
            }
        }
    }

    pub unsafe fn fill_yuv420_from_rgba_pixmap(
        frame: *mut AVFrame,
        matrix: YuvMatrix,
        rgba_pixels: &[u8],
    ) -> *mut AVFrame {
        unsafe {
            let is_writable = av_frame_make_writable(frame);
            assert!(is_writable >= 0, "Can not reuse frame allocations");

            super::pix_fmt::fill_yuv420_from_rgba_pixmap_accelerated(
                matrix,
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
                av_frame_free(&raw mut self.av_frame);
            }
            if !self.packet.is_null() {
                av_packet_unref(self.packet);
                av_packet_free(&raw mut self.packet);
            }
        }
    }
}

unsafe impl Send for EncoderFrame {}
unsafe impl Sync for EncoderFrame {}
