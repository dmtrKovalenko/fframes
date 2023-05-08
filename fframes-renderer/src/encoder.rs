use ffmpeg_next::sys::*;
pub use ffmpeg_next::sys::{AVPixelFormat, AVSampleFormat};
use std::{ffi::CString, os::raw::c_char, path::PathBuf, sync::Arc};

use crate::{
    ffmpeg_action,
    renderer_error::{self, AVError, AVResult},
    stream, FFramesLogger,
};

#[inline(always)]
#[allow(non_snake_case)]
pub const fn FFMPEG_AVERROR(e: std::os::raw::c_int) -> std::os::raw::c_int {
    -e
}

#[allow(dead_code)]
extern "C" {
    pub fn av_error_to_string(err: i32) -> *mut c_char;
    pub fn make_stereo_layout_channel(c: *mut AVCodecContext) -> i32;
    pub fn log_packet(fmt_ctx: *mut AVStream, packet: *mut AVPacket);
    pub fn validate_sample_rate_fits_codec(codec: *const AVCodec, sample_rate: i32) -> i32;
}

#[derive(Debug, Clone)]
pub struct EncoderOptions<'a> {
    /// If several codecs available for specified format output here you can specify the ffmpeg compatible name of the video codec that should be used to encode.
    pub preferred_video_codec: Option<&'a str>,
    /// If several codecs available for specified format output here you can specify the ffmpeg compatible name of the audio codec that should be used to encode.
    pub preferred_audio_codec: Option<&'a str>,
    /// Pixel format used to store encoded frame. By default equals to AVPixelFormat::AV_PIX_FMT_YUV420P
    pub pixel_format: AVPixelFormat,
    /// Sample format used to store encoded audio frame. By default equals to AvSampleFormat::AV_SAMPLE_FMT_FLTP
    pub sample_format: AVSampleFormat,
    /// Audio bitrate in bytes, if not provided 192k used.
    pub audio_bitrate: Option<i64>,
    /// Video bitrate, sometimes may not be needed and inferred from other codec params, like crf for libx264 and libx265
    pub video_bitrate: Option<i64>,
    /// Number of bits the bitstream is allowed to diverge from the reference. the reference can be CBR (for CBR pass1) or VBR (for pass2)
    pub bitrate_tolerance: i32,
    /// Minimum quantizer
    pub qmin: i32,
    /// Maximum quantizer
    pub qmax: i32,
    ///  amount of qscale change between easy & hard scenes (0.0-1.0)
    pub qcompress: f32,
    /// maximum quantizer difference between frames
    pub max_qdiff: i32,
    /// Size of group of picture
    pub gop_size: i32,
    /// Output sample rate of the final video file
    pub sample_rate: Option<i32>,
    /// Directory used to store temporary files and artifacts generated for rendering and encoding.
    pub tmp_files_directory: Option<&'a PathBuf>,
    /// Dynamic set of options specific to encoder. Every encoder accepts its own purely dynamic set of options, e.g.
    /// the most popular example for h264 & h265 codecs are options like `-crf 18 -tune animation -preset ultrafast`.
    ///
    /// You can pass this set of options like:
    /// ```rust
    /// codec_params: &HashMap::from([("crf", "18"), ("tune", "animation"), ("preset", "ultrafast")])
    /// ```
    ///
    /// Find available set of options for your codec using
    /// ```sh
    /// ffmpeg -h encoder={your_codec_name} -v quiet
    /// ```
    ///
    /// ## Safety
    /// **Libav can throw segmentation fault if some options are invalid or the value is not correct.**
    pub codec_params: Option<&'a [(&'a str, &'a str)]>,
}

impl<'a> Default for EncoderOptions<'a> {
    fn default() -> Self {
        Self {
            audio_bitrate: None,
            preferred_video_codec: None,
            preferred_audio_codec: None,
            sample_rate: None,
            tmp_files_directory: None,
            pixel_format: AVPixelFormat::AV_PIX_FMT_YUV420P,
            video_bitrate: None,
            bitrate_tolerance: 0,
            qmin: 10,
            qmax: 51,
            qcompress: 0.6,
            max_qdiff: 4,
            gop_size: 12,
            sample_format: AVSampleFormat::AV_SAMPLE_FMT_FLTP,
            codec_params: None,
        }
    }
}

pub struct Encoder {
    pub(crate) video_stream: stream::Stream,
    pub(crate) audio_stream: Option<stream::Stream>,
    pub(crate) b_frames_count: i32,
    pub(crate) oc: *mut AVFormatContext,
}

impl Encoder {
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn with_output<T, F: FnMut(&mut Encoder) -> T>(
        width: i32,
        height: i32,
        fps: i32,
        filename: &str,
        encoder_options: &EncoderOptions,
        logger: &Arc<dyn FFramesLogger>,
        inner_fn: &mut F,
    ) -> AVResult<T> {
        av_log_set_level(logger.get_libav_log_level());

        let c_filename = CString::new(filename).unwrap();
        let mut oc: *mut AVFormatContext = std::ptr::null_mut();

        ffmpeg_action!(
            avformat_alloc_output_context2(
                &mut oc,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                c_filename.as_ptr(),
            ),
            AVError::UnknownExtension(filename.to_owned())
        );

        let video_stream = stream::Stream::make_video(width, height, fps, oc, encoder_options)?;

        let mut encoder = Encoder {
            b_frames_count: 0,
            video_stream,
            oc,
            audio_stream: None,
        };

        if logger.should_dump_format_info() {
            av_dump_format(oc, 0, c_filename.as_ptr(), 1);
        }

        ffmpeg_action!(
            avio_open(&mut (*oc).pb, c_filename.as_ptr(), 2),
            AVError::CantOpenFile(filename.to_owned())
        );

        avformat_write_header(oc, std::ptr::null_mut());

        let res = inner_fn(&mut encoder);

        avcodec_send_frame(video_stream.enc, std::ptr::null_mut());
        if let Some(audio_stream) = encoder.audio_stream {
            avcodec_send_frame(audio_stream.enc, std::ptr::null_mut());
        }

        av_write_trailer(oc);
        video_stream.free();

        if let Some(audio_stream) = encoder.audio_stream {
            avcodec_close(audio_stream.enc);
            audio_stream.free();
        }

        avio_closep(&mut (*oc).pb);
        avformat_free_context(oc);

        Ok(res)
    }

    pub unsafe fn send_customizeable_frame_packet<F: Fn(*mut AVPacket) -> i32>(
        &mut self,
        stream: &stream::Stream,
        frame: EncoderFrame,
        customize_frame: F,
    ) -> AVResult<()> {
        let frame = frame.0;
        let avcodec_send_frame = avcodec_send_frame(stream.enc, frame);
        let mut status = avcodec_send_frame;

        if status < 0 {
            let error_description = av_error_to_string(status);

            return Err(renderer_error::AVError::CantWriteFrame(
                CString::from_raw(error_description)
                    .to_str()
                    .unwrap_or("Unknown libav error.")
                    .to_owned(),
            ));
        }

        let packet = av_packet_alloc();
        while status >= 0 {
            status = avcodec_receive_packet(stream.enc, packet);

            match status {
                status if status == AVERROR_EOF => break,
                status if status == FFMPEG_AVERROR(EAGAIN) => {
                    self.b_frames_count += 1;
                    break;
                }
                _ => status = customize_frame(packet),
            }
        }

        Ok(())
    }

    pub unsafe fn send_frame(
        &mut self,
        stream: &stream::Stream,
        frame: EncoderFrame,
    ) -> AVResult<()> {
        let oc = self.oc;

        self.send_customizeable_frame_packet(stream, frame, |packet| {
            av_packet_rescale_ts(packet, (*stream.enc).time_base, (*stream.st).time_base);

            (*packet).stream_index = (*stream.st).index;
            av_interleaved_write_frame(oc, packet)
        })
    }
}

#[derive(Clone, Copy)]
pub struct EncoderFrame(pub(crate) *mut AVFrame);

impl EncoderFrame {
    pub fn make(stream: &stream::Stream) -> Self {
        unsafe {
            let mut frame = av_frame_alloc();

            match stream.variant {
                stream::StreamVariant::Video => {
                    (*frame).format = (*stream.enc).pix_fmt as i32;
                    (*frame).width = (*stream.enc).width;
                    (*frame).height = (*stream.enc).height;
                }
                stream::StreamVariant::Audio(_) => {
                    av_channel_layout_copy(&mut (*frame).ch_layout, &(*stream.enc).ch_layout);

                    (*frame).format = (*stream.enc).sample_fmt as i32;
                    (*frame).channel_layout = (*stream.enc).channel_layout;
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

            let response = av_frame_get_buffer(frame, 0);
            assert!(response >= 0, "Could not allocate frame data");

            EncoderFrame(frame)
        }
    }

    #[inline(always)]
    fn get_rgb(pixmap: &[u8], i: usize) -> (i32, i32, i32) {
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
        let is_writable = av_frame_make_writable(self.0);
        if is_writable < 0 {
            panic!("Can not reuse frame allocations");
        }

        (*self.0).pts = frame_index;
        if audio_data.is_empty() {
            return self.0;
        }

        let mut fltp_audio_data = audio_data
            .into_iter()
            .flat_map(|data| data.to_le_bytes())
            .collect::<Vec<u8>>();

        (*self.0).data[0] = fltp_audio_data.as_mut_ptr();

        self.0
    }

    /// We support only yuv420 format as for now so we can pretty efficiently convert the bitmap buffer.
    /// yuv420 represented by y per each pixel and uv (cb and cr) per each 2x2 pixel block.
    #[allow(clippy::precedence)]
    pub unsafe fn fill_from_rgba_pixmap(
        &mut self,
        frame_index: i64,
        rgb_pixels: &[u8],
    ) -> *mut AVFrame {
        let is_writable = av_frame_make_writable(self.0);
        if is_writable < 0 {
            panic!("Can not reuse frame allocations");
        }

        let height = (*self.0).height as usize;
        let width = (*self.0).width as usize;

        let av_frame = self.0;
        // an important note that linesize here can be different from the width of an image so it is required to fill the buffer correctly.
        let frame_size: usize = height * (*av_frame).linesize[0] as usize + width;

        let y_pixels = std::slice::from_raw_parts_mut((*av_frame).data[0], frame_size);
        let cb_pixels = std::slice::from_raw_parts_mut((*av_frame).data[1], frame_size / 2);
        let cr_pixels = std::slice::from_raw_parts_mut((*av_frame).data[2], frame_size / 2);

        for y in 0..height {
            for x in 0..width {
                let (r, g, b) = EncoderFrame::get_rgb(rgb_pixels, y * width + x);

                // use a linesize to get the correct index for the pixel as it can differ for different dimensions.
                y_pixels[(y * (*av_frame).linesize[0] as usize + x)] =
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

    pub fn free(&mut self) {
        unsafe {
            av_frame_free(&mut self.0);
        }
    }
}

unsafe impl Send for Encoder {}
unsafe impl Sync for Encoder {}
unsafe impl Send for EncoderFrame {}
unsafe impl Sync for EncoderFrame {}
