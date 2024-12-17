use crate::{encoder_frame::EncoderFrame, renderer_error::RenderEncodingResult};
use crate::{
    ffmpeg_action,
    renderer_error::{self, RenderEncodingError},
    stream, FFramesLogger,
};
use ffmpeg_sys_fframes::*;
use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
    path::PathBuf,
    sync::Arc,
};

pub use ffmpeg_sys_fframes::{AVPixelFormat, AVSampleFormat, MKBETAG, MKTAG};

#[inline(always)]
#[allow(non_snake_case)]
pub const fn FFMPEG_AVERROR(e: std::os::raw::c_int) -> std::os::raw::c_int {
    -e
}

pub fn av_error_to_string(errnum: i32) -> String {
    let mut errbuf = [0 as c_char; AV_ERROR_MAX_STRING_SIZE];
    unsafe {
        if av_strerror(errnum, errbuf.as_mut_ptr(), AV_ERROR_MAX_STRING_SIZE) < 0 {
            return "Unknown error".to_string();
        }

        CStr::from_ptr(errbuf.as_ptr())
            .to_string_lossy()
            .to_string()
    }
}

#[derive(Debug, Clone)]
pub struct EncoderOptions<'a> {
    /// If several codecs available for specified format output here you can specify the libav (ffmpeg) compatible name of the video codec that should be used to encode.
    pub preferred_video_codec: Option<&'a str>,
    /// If several codecs available for specified format output here you can specify the libav (ffmpeg) compatible name of the audio codec that should be used to encode.
    /// If not provided or the name is invalid or the codec is not compatible with the output
    /// container format fallback to the first available codec for the specified output format.
    pub preferred_audio_codec: Option<&'a str>,
    /// Pixel format used to store encoded frame. By default equals to AVPixelFormat::AV_PIX_FMT_YUV420P
    /// @default AV_PIX_FMT_YUV420P
    pub pixel_format: AVPixelFormat,
    /// Sample format used to store encoded audio frame. By default equals to AvSampleFormat::AV_SAMPLE_FMT_FLTP
    /// @default AV_SAMPLE_FMT_FLTP
    pub sample_format: AVSampleFormat,
    /// Audio bitrate in bytes, if not provided 192k used.
    pub audio_bitrate: Option<i64>,
    /// Video bitrate, sometimes may not be needed and inferred from other codec params, like crf for libx264 and libx265
    pub video_bitrate: Option<i64>,
    /// Number of bits the bitstream is allowed to diverge from the reference.
    /// @default 0
    pub bitrate_tolerance: i32,
    /// Minimum quantizer
    /// @default 10
    pub qmin: i32,
    /// Maximum quantizer
    /// @default 51
    pub qmax: i32,
    ///  amount of qscale change between easy & hard scenes (0.0-1.0)
    pub qcompress: f32,
    /// maximum quantizer difference between frames
    /// @default 4
    pub max_qdiff: i32,
    /// Size of group of picture
    /// @default 12
    pub gop_size: i32,
    /// Output sample rate of the final video file
    /// @default 44100
    pub sample_rate: usize,
    /// Directory used to store temporary files and artifacts generated for rendering and encoding.
    pub tmp_files_directory: Option<&'a PathBuf>,
    /// Dynamic set of options specific to encoder. Every encoder accepts its own purely dynamic set of options, e.g.
    /// the most popular example for h264 & h265 codecs are options like `-crf 18 -tune animation -preset ultrafast`.
    ///
    /// You can pass this set of options like:
    /// ```rust
    /// let encoder_options = fframes_renderer::EncoderOptions { codec_params: Some(&[("crf", "23"), ("tune", "animation"), ("preset", "ultrafast")]),
    ///     ..Default::default()
    /// };
    /// ```
    ///
    /// Find available set of options for your codec using
    /// ```sh
    /// ffmpeg -h encoder={your_codec_name} -v quiet
    /// ```
    ///
    /// ## Safety
    /// ### Libav can have segmentation fault if some options are invalid or the value is not correct.
    /// ### So it is very important to validate the function parameters before usage
    /// ### because segfaults **won't be caught** by fframes.
    pub codec_params: Option<&'a [(&'a str, &'a str)]>,
    /// Tag used to identify video stream in the output file.
    /// to create a tag use the `MKTAG!` macro:
    ///
    /// ```rust
    /// use fframes_renderer::MKTAG;
    /// let video_tag = MKTAG!('h', 'v', 'c', '1');
    /// ```
    pub video_tag: Option<isize>,
    /// Tag used to identify audio stream in the output file
    /// to create a tag use the `MKTAG!` macro:
    ///
    /// ```rust
    /// use fframes_renderer::MKTAG;
    /// let video_tag = MKTAG!('h', 'v', 'c', '1');
    /// ```
    pub audio_tag: Option<isize>,
}

impl Default for EncoderOptions<'_> {
    fn default() -> Self {
        Self {
            audio_bitrate: None,
            bitrate_tolerance: 0,
            codec_params: Some(&[
                ("crf", "23"),
                ("tune", "animation"),
                ("preset", "ultrafast"),
                ("bframes", "5"),
            ]),
            gop_size: 24,
            max_qdiff: 4,
            pixel_format: AVPixelFormat::AV_PIX_FMT_YUV420P,
            preferred_audio_codec: None,
            preferred_video_codec: None,
            qcompress: 0.6,
            qmax: 60,
            qmin: 15,
            sample_format: AVSampleFormat::AV_SAMPLE_FMT_FLTP,
            sample_rate: 44100,
            tmp_files_directory: None,
            video_bitrate: None,
            video_tag: None,
            audio_tag: None,
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
    pub unsafe fn with_output<T, F: FnMut(&mut Encoder) -> RenderEncodingResult<T>>(
        width: i32,
        height: i32,
        fps: i32,
        filename: &str,
        encoder_options: &EncoderOptions,
        logger: &Arc<dyn FFramesLogger>,
        inner_fn: &mut F,
    ) -> RenderEncodingResult<T> {
        av_log_set_level(logger.get_libav_log_level());

        let c_filename = CString::new(filename).map_err(RenderEncodingError::CStringError)?;
        let mut oc: *mut AVFormatContext = std::ptr::null_mut();

        ffmpeg_action!(
            avformat_alloc_output_context2(
                &mut oc,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                c_filename.as_ptr(),
            ),
            RenderEncodingError::UnknownExtension(filename.to_owned())
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
            RenderEncodingError::CantOpenFile(filename.to_owned())
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

        res
    }

    pub unsafe fn send_customizable_frame_packet<F: Fn(*mut AVPacket) -> i32>(
        &mut self,
        stream: &stream::Stream,
        EncoderFrame { frame, .. }: &EncoderFrame,
        customize_frame: F,
    ) -> RenderEncodingResult<()> {
        let avcodec_send_frame = avcodec_send_frame(stream.enc, *frame);
        let mut status = avcodec_send_frame;

        if status < 0 {
            let error_description = av_error_to_string(status);

            return Err(renderer_error::RenderEncodingError::CantWriteFrame(
                error_description,
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
        frame: &EncoderFrame,
    ) -> RenderEncodingResult<()> {
        let oc = self.oc;

        self.send_customizable_frame_packet(stream, frame, |packet| {
            av_packet_rescale_ts(packet, (*stream.enc).time_base, (*stream.st).time_base);

            (*packet).stream_index = (*stream.st).index;
            av_interleaved_write_frame(oc, packet)
        })
    }
}

unsafe impl Send for Encoder {}
unsafe impl Sync for Encoder {}
