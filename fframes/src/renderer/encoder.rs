use super::{
    FFramesLogger,
    frame_export::{EncoderInput, VideoFrame},
    renderer_error::{self, RenderEncodingError},
    stream,
    stream::Stream,
};
pub use super::{encoder_frame::EncoderFrame, renderer_error::RenderEncodingResult};
use crate::ffmpeg_sys_fframes::*;
use crate::{RenderOptions, ffmpeg_action};
use std::ops::Range;
use std::path::Path;
use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
    sync::Arc,
};

pub use crate::media::ffmpeg_sys_fframes::{AVPixelFormat, MKTAG};

#[inline(always)]
#[allow(non_snake_case)]
pub const fn FFMPEG_AVERROR(e: std::os::raw::c_int) -> std::os::raw::c_int {
    -e
}

/// Writes the container header.  With `faststart`, mp4/mov/3gp files get the
/// `moov` atom moved to the front (`movflags +faststart`, a second pass over
/// the file when the trailer is written) so browsers and NLEs can start
/// playback before the whole file is downloaded.
pub(crate) unsafe fn write_header(
    oc: *mut AVFormatContext,
    faststart: bool,
) -> Result<(), renderer_error::RenderEncodingError> {
    unsafe {
        let format_name_ptr = (*(*oc).oformat).name;
        let format_name = if format_name_ptr.is_null() {
            std::borrow::Cow::Borrowed("")
        } else {
            CStr::from_ptr(format_name_ptr).to_string_lossy()
        };

        let mut opts: *mut AVDictionary = std::ptr::null_mut();
        if faststart
            && ["mp4", "mov", "3gp"]
                .iter()
                .any(|container| format_name.contains(container))
        {
            let key = CString::new("movflags").unwrap();
            let value = CString::new("+faststart").unwrap();
            av_dict_set(&raw mut opts, key.as_ptr(), value.as_ptr(), 0);
        }

        let status = avformat_write_header(oc, &raw mut opts);
        av_dict_free(&raw mut opts);

        if status < 0 {
            return Err(renderer_error::RenderEncodingError::FFmpegError(
                status,
                av_error_to_string(status),
            ));
        }

        Ok(())
    }
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
    /// Force the codec to be used for encoding audio/video stream. If not provided the codec will
    /// be inferred from the output file extensions + preferred encoder (if specified).
    pub codec: Option<AVCodecID>,
    /// If several encoders available for the specified codec and/or container format here you can
    /// specify the ffmpeg-compatible name of the encoder that should be used. If nothing provided
    /// fallback toe the first available encoder for the specified output format.
    pub preferred_encoder: Option<&'a str>,
    /// Pixel format used to store encoded frame. By default equals to `AVPixelFormat::AV_PIX_FMT_YUV420P`
    /// If not supported by the encoder the first supported pixel format will be used (which may
    /// lead to quality of alpha channel loss)
    ///
    /// Ignored for audio streams.
    /// @default `AV_PIX_FMT_YUV420P`
    pub pixel_format: AVPixelFormat,
    /// Sample format used to store encoded audio frame. By default equals to `AvSampleFormat::AV_SAMPLE_FMT_FLTP`
    /// Ignored for video streams.
    ///
    /// @default `AV_SAMPLE_FMT_FLTP`
    pub sample_format: AVSampleFormat,
    /// Target audio bitrate in bits,
    /// For video streams sometimes may not be needed and set dynamically based on the other codec
    /// params, like crf for libx264 and libx265.
    /// If not provided 192kb used for audios streams and set dynamically for video streams.
    pub bitrate: Option<i64>,
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
    /// Specify the resulting sample rate of the audio stream (ignored for video streams)
    /// @default 44100
    pub sample_rate: usize,
    /// Dynamic set of options specific to encoder. Every encoder accepts its own purely dynamic set of options, e.g.
    /// the most popular example for h264 & h265 codecs are options like `-crf 18 -tune animation -preset ultrafast`.
    ///
    /// Intermediate segments encode in parallel and default to one codec thread each.
    /// Override this with `("threads", "N")`, or `("threads", "0")` for automatic sizing.
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
    pub tag: Option<isize>,
}

impl EncoderOptions<'_> {
    /// Split video for concurrent rendering taking into account the GOP size
    /// to make sure that individual chunks are never less than 2xGOP size.
    /// The shortest part of a video the renderers encode separately. Every part starts
    /// with a keyframe, so it is never shorter than a GOP or than a second of video,
    /// whichever is smaller.
    pub fn min_segment_frames(&self, fps: usize) -> usize {
        (self.gop_size.max(1) as usize).min(fps.max(1))
    }

    pub fn split_gop_chunks(
        &self,
        duration_in_frames: usize,
        concurrency: usize,
    ) -> Vec<Range<usize>> {
        let gop_size = self.gop_size as usize;
        let min_chunk = 2 * gop_size;

        if duration_in_frames < min_chunk {
            #[allow(clippy::single_range_in_vec_init)]
            return vec![0..duration_in_frames];
        }

        let max_possible_chunks = duration_in_frames / min_chunk;
        let actual_chunks = concurrency.min(max_possible_chunks).max(1);
        let chunk_size = duration_in_frames.div_ceil(actual_chunks);

        let mut chunks = vec![];
        let mut prev_chunk = 0;

        while prev_chunk < duration_in_frames {
            let remaining = duration_in_frames - prev_chunk;
            if remaining > chunk_size {
                chunks.push(prev_chunk..prev_chunk + chunk_size);
                prev_chunk += chunk_size;
            } else {
                chunks.push(prev_chunk..prev_chunk + remaining);
                prev_chunk += remaining;
            }
        }
        chunks
    }
}

impl Default for EncoderOptions<'_> {
    fn default() -> Self {
        Self {
            bitrate: None,
            bitrate_tolerance: 0,
            codec: None,
            codec_params: None,
            gop_size: 24,
            max_qdiff: 4,
            pixel_format: AVPixelFormat::AV_PIX_FMT_YUV420P,
            preferred_encoder: None,
            qcompress: 0.6,
            qmax: 60,
            qmin: 15,
            sample_format: AVSampleFormat::AV_SAMPLE_FMT_FLTP,
            sample_rate: 44100,
            tag: None,
        }
    }
}

pub struct Encoder {
    pub video_stream: stream::Stream,
    pub audio_stream: Option<stream::Stream>,
    pub(crate) oc: *mut AVFormatContext,
}

impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe {
            if !self.oc.is_null() {
                // Flush any remaining packets first
                if !self.video_stream.enc.is_null() {
                    avcodec_flush_buffers(self.video_stream.enc);
                }

                match self.audio_stream {
                    Some(ref audio_stream) if audio_stream.enc.is_null() => {
                        avcodec_flush_buffers(audio_stream.enc);
                    }
                    _ => {}
                }

                av_write_trailer(self.oc);
                self.video_stream.free();

                if let Some(mut audio_stream) = self.audio_stream.take() {
                    audio_stream.free();
                }

                if !(*self.oc).metadata.is_null() {
                    av_dict_free(&raw mut (*self.oc).metadata);
                }

                if !(*self.oc).pb.is_null() {
                    avio_closep(&raw mut (*self.oc).pb);
                }

                avformat_free_context(self.oc);
            }
        }
    }
}

/// What an [`Encoder`] writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderOutput {
    /// The file the user asked for.
    Final { with_audio: bool },
    /// A per-thread chunk that is concatenated into the final file later.
    /// It never carries audio and skips the `faststart` rewrite pass.
    IntermediateChunk,
}

impl Encoder {
    pub unsafe fn new(
        output: EncoderOutput,
        width: i32,
        height: i32,
        fps: i32,
        filename: &Path,
        render_options: &RenderOptions,
        logger: &Arc<dyn FFramesLogger>,
    ) -> RenderEncodingResult<Self> {
        unsafe {
            Self::new_with_input(
                output,
                width,
                height,
                fps,
                filename,
                render_options,
                &EncoderInput::software(render_options.video_encoder_options.pixel_format),
                logger,
            )
        }
    }

    /// Like [`Self::new`] with the video encoder opened for the frames a rendering backend
    /// negotiated (`FFramesRenderBackend::negotiate_encoder_input`).
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn new_with_input(
        output: EncoderOutput,
        width: i32,
        height: i32,
        fps: i32,
        filename: &Path,
        render_options: &RenderOptions,
        input: &EncoderInput,
        logger: &Arc<dyn FFramesLogger>,
    ) -> RenderEncodingResult<Self> {
        unsafe {
            av_log_set_level(logger.get_libav_log_level());

            let c_filename = CString::new(filename.to_string_lossy().as_ref())
                .map_err(RenderEncodingError::CStringError)?;
            let mut oc: *mut AVFormatContext = std::ptr::null_mut();

            ffmpeg_action!(
                avformat_alloc_output_context2(
                    &raw mut oc,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    c_filename.as_ptr(),
                ),
                RenderEncodingError::UnknownExtension(filename.to_owned())
            );

            let video_stream = stream::Stream::make_video(
                width,
                height,
                fps,
                oc,
                &render_options.video_encoder_options,
                input,
                // Segments already encode in parallel. Letting each encoder auto-size
                // its own thread pool multiplies both threads and buffered frames.
                i32::from(matches!(output, EncoderOutput::IntermediateChunk)),
            )?;
            if logger.should_dump_format_info() {
                av_dump_format(oc, 0, c_filename.as_ptr(), 1);
            }

            ffmpeg_action!(
                avio_open(&raw mut (*oc).pb, c_filename.as_ptr(), 2),
                RenderEncodingError::CantOpenFile(filename.to_owned())
            );

            let audio_stream = matches!(output, EncoderOutput::Final { with_audio: true })
                .then(|| Stream::make_audio(oc, &render_options.audio_encoder_options))
                .transpose()?;

            write_header(oc, matches!(output, EncoderOutput::Final { .. }))?;

            Ok(Encoder {
                video_stream,
                audio_stream,
                oc,
            })
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub unsafe fn with_output<T, F: FnMut(&mut Encoder) -> RenderEncodingResult<T>>(
        output: EncoderOutput,
        width: i32,
        height: i32,
        fps: i32,
        filename: &Path,
        render_options: &RenderOptions,
        logger: &Arc<dyn FFramesLogger>,
        inner_fn: &mut F,
    ) -> RenderEncodingResult<T> {
        unsafe {
            let mut encoder =
                Encoder::new(output, width, height, fps, filename, render_options, logger)?;

            inner_fn(&mut encoder)
            // Encoder::drop() will be called here
        }
    }

    pub unsafe fn send_customizable_frame_packet<F: Fn(*mut AVPacket) -> i32>(
        &self,
        stream: &stream::Stream,
        EncoderFrame {
            av_frame: frame,
            packet,
            ..
        }: &EncoderFrame,
        customize_frame: F,
    ) -> RenderEncodingResult<()> {
        unsafe { Self::send_raw_frame(stream, *frame, *packet, customize_frame) }
    }

    unsafe fn send_raw_frame<F: Fn(*mut AVPacket) -> i32>(
        stream: &stream::Stream,
        frame: *mut AVFrame,
        packet: *mut AVPacket,
        customize_frame: F,
    ) -> RenderEncodingResult<()> {
        unsafe {
            let mut status = avcodec_send_frame(stream.enc, frame);

            if status < 0 {
                let error_description = av_error_to_string(status);

                return Err(renderer_error::RenderEncodingError::CantEncodeFrame {
                    error: error_description,
                    pts: Some((*frame).pts),
                });
            }

            while status >= 0 {
                status = avcodec_receive_packet(stream.enc, packet);

                if status == AVERROR_EOF || status == FFMPEG_AVERROR(EAGAIN) {
                    break;
                }

                if status < 0 {
                    let error_description = av_error_to_string(status);
                    return Err(renderer_error::RenderEncodingError::CantEncodeFrame {
                        error: format!("avcodec_receive_packet failed: {error_description}"),
                        pts: Some((*frame).pts),
                    });
                }

                let write_status = customize_frame(packet);
                if write_status < 0 {
                    let error_description = av_error_to_string(write_status);
                    return Err(renderer_error::RenderEncodingError::CantWriteFrame(
                        error_description,
                    ));
                }
                status = write_status;
            }

            Ok(())
        }
    }

    pub unsafe fn send_frame(
        &self,
        stream: &stream::Stream,
        frame: &EncoderFrame,
    ) -> RenderEncodingResult<()> {
        unsafe {
            let oc = self.oc;

            self.send_customizable_frame_packet(stream, frame, |packet| {
                av_packet_rescale_ts(packet, (*stream.enc).time_base, (*stream.st).time_base);

                (*packet).stream_index = (*stream.st).index;
                av_interleaved_write_frame(oc, packet)
            })
        }
    }

    /// Encodes a frame of the video stream with `pts` in frames and writes the packets
    /// that are ready into the file. `packet` is scratch space that is reused between calls.
    pub unsafe fn send_video_frame(
        &self,
        frame: &VideoFrame,
        pts: i64,
        packet: *mut AVPacket,
    ) -> RenderEncodingResult<()> {
        unsafe {
            let stream = &self.video_stream;
            let oc = self.oc;
            (*frame.as_ptr()).pts = pts;

            Self::send_raw_frame(stream, frame.as_ptr(), packet, |packet| {
                // VideoToolbox can return packets without a duration. Our video time
                // base is one frame, including the final packet: otherwise MP4 can
                // end its edit list at that frame's PTS and discard the last picture.
                if (*packet).duration == 0 {
                    (*packet).duration = 1;
                }
                av_packet_rescale_ts(packet, (*stream.enc).time_base, (*stream.st).time_base);

                (*packet).stream_index = (*stream.st).index;
                av_interleaved_write_frame(oc, packet)
            })
        }
    }

    /// Signals the end of the stream to the codec and writes every packet it still
    /// holds (lookahead, frame threads, B-frames). The stream can not receive frames
    /// afterwards.
    pub unsafe fn flush_stream(&self, stream: &stream::Stream) -> RenderEncodingResult<()> {
        unsafe {
            let status = avcodec_send_frame(stream.enc, std::ptr::null());
            if status < 0 && status != AVERROR_EOF {
                return Err(renderer_error::RenderEncodingError::CantEncodeFrame {
                    error: av_error_to_string(status),
                    pts: None,
                });
            }

            let mut packet = av_packet_alloc();
            let result = loop {
                let status = avcodec_receive_packet(stream.enc, packet);
                if status == AVERROR_EOF || status == FFMPEG_AVERROR(EAGAIN) {
                    break Ok(());
                }
                if status < 0 {
                    break Err(renderer_error::RenderEncodingError::CantEncodeFrame {
                        error: format!(
                            "avcodec_receive_packet failed: {}",
                            av_error_to_string(status)
                        ),
                        pts: None,
                    });
                }

                if matches!(stream.variant, stream::StreamVariant::Video) && (*packet).duration == 0
                {
                    (*packet).duration = 1;
                }
                av_packet_rescale_ts(packet, (*stream.enc).time_base, (*stream.st).time_base);
                (*packet).stream_index = (*stream.st).index;
                let status = av_interleaved_write_frame(self.oc, packet);
                if status < 0 {
                    break Err(renderer_error::RenderEncodingError::CantWriteFrame(
                        av_error_to_string(status),
                    ));
                }
            };
            av_packet_free(&raw mut packet);

            result
        }
    }

    /// Finishes the video stream, see [`Self::flush_stream`].
    pub unsafe fn submit_leftover_b_frames(
        &self,
        _frame: &EncoderFrame,
        stream: &stream::Stream,
        _expected_frames_in_stream: usize,
    ) -> RenderEncodingResult<()> {
        unsafe { self.flush_stream(stream) }
    }
}

unsafe impl Send for Encoder {}
unsafe impl Sync for Encoder {}

#[cfg(all(test, feature = "h264"))]
mod tests {
    use super::*;
    use crate::{FFramesLoggerVariant, renderer::fframes_logger::make_logger};

    #[test]
    fn parallel_segments_limit_codec_threads_and_allow_overrides() {
        let directory =
            std::env::temp_dir().join(format!("fframes-threads-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let logger = make_logger(FFramesLoggerVariant::Silent);

        for (index, (params, expected)) in [
            (vec![("preset", "veryfast")], 1),
            (vec![("preset", "veryfast"), ("threads", "2")], 2),
        ]
        .into_iter()
        .enumerate()
        {
            let options = RenderOptions {
                video_encoder_options: EncoderOptions {
                    preferred_encoder: Some("libx264"),
                    codec_params: Some(&params),
                    ..Default::default()
                },
                ..Default::default()
            };
            let encoder = unsafe {
                Encoder::new(
                    EncoderOutput::IntermediateChunk,
                    64,
                    64,
                    30,
                    &directory.join(format!("{index}.mp4")),
                    &options,
                    &logger,
                )
            }
            .unwrap_or_else(|err| panic!("failed to open test encoder: {err}"));
            assert_eq!(
                unsafe { (*encoder.video_stream.enc).thread_count },
                expected
            );
        }

        std::fs::remove_dir_all(directory).unwrap();
    }
}
