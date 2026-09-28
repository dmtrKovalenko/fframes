use super::{
    encoder::Encoder,
    encoder_frame::EncoderFrame,
    renderer_error::{RenderEncodingError, RenderEncodingResult},
    stream::Stream,
    stream::StreamVariant,
};
pub use crate::ffmpeg_action;
use crate::{AudioTimelineSamples, AudioTimelineUnit, FFramesContext, ResolvedAudioMap};
use crate::{RenderOptions, ffmpeg_sys_fframes::*};
use std::{
    ffi::CString,
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct AvPacketAutoFree {
    av_packet: *mut AVPacket,
}

impl Default for AvPacketAutoFree {
    fn default() -> Self {
        Self::new()
    }
}

impl AvPacketAutoFree {
    pub fn new() -> Self {
        unsafe {
            AvPacketAutoFree {
                av_packet: av_packet_alloc(),
            }
        }
    }

    pub fn get(&mut self) -> *mut AVPacket {
        self.av_packet
    }

    pub fn get_mut(&mut self) -> &mut AVPacket {
        unsafe { &mut *self.av_packet }
    }
}

impl Drop for AvPacketAutoFree {
    fn drop(&mut self) {
        unsafe {
            av_packet_free(&mut self.av_packet);
        }
    }
}

unsafe fn open_file_stream(
    filename: &Path,
    input_format_ctx: &mut *mut AVFormatContext,
    codec_type: AVMediaType,
) -> RenderEncodingResult<*mut AVStream> {
    unsafe {
        let input_file = CString::new(filename.to_string_lossy().as_ref())
            .map_err(RenderEncodingError::CStringError)?;

        ffmpeg_action!(
            avformat_open_input(
                input_format_ctx,
                input_file.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ),
            RenderEncodingError::CantOpenFile(filename.to_owned())
        );

        ffmpeg_action!(
            avformat_find_stream_info(*input_format_ctx, std::ptr::null_mut()),
            RenderEncodingError::CantOpenFile(filename.to_owned())
        );

        let streams = std::slice::from_raw_parts_mut(
            (*(*input_format_ctx)).streams,
            (*(*input_format_ctx)).nb_streams as usize,
        );

        let mut input_stream = std::ptr::null_mut();
        for stream in streams {
            let codec = (*stream.to_owned()).codecpar;

            if (*codec).codec_type == codec_type {
                input_stream = *stream;
                break;
            }
        }

        if input_stream.is_null() {
            Err(RenderEncodingError::MissingVideoStreamInFile(
                filename.to_owned(),
            ))
        } else {
            Ok(input_stream)
        }
    }
}

unsafe fn create_encoder_copy_from_file(
    file: &Path,
    output: &Path,
    render_options: &RenderOptions,
) -> Result<Encoder, RenderEncodingError> {
    unsafe {
        let mut input_format_ctx: *mut AVFormatContext = std::ptr::null_mut();
        let mut output_format_ctx: *mut AVFormatContext = std::ptr::null_mut();

        let input_video_stream =
            open_file_stream(file, &mut input_format_ctx, AVMediaType::AVMEDIA_TYPE_VIDEO)?;

        let output_file = CString::new(output.to_string_lossy().as_ref())
            .map_err(RenderEncodingError::CStringError)?;
        avformat_alloc_output_context2(
            &mut output_format_ctx,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            output_file.as_ptr(),
        );

        let output_video_stream = avformat_new_stream(output_format_ctx, std::ptr::null_mut());
        let mut audio_stream =
            match Stream::make_audio(output_format_ctx, &render_options.audio_encoder_options) {
                Ok(audio_stream) => audio_stream,
                Err(err) => {
                    avformat_close_input(&mut input_format_ctx);
                    avformat_free_context(output_format_ctx);
                    return Err(err);
                }
            };

        avcodec_parameters_copy(
            (*output_video_stream).codecpar,
            (*input_video_stream).codecpar,
        );
        (*output_video_stream).time_base = (*input_video_stream).time_base;

        avformat_close_input(&mut input_format_ctx);
        avio_open(
            &mut (*output_format_ctx).pb,
            output_file.as_ptr(),
            AVIO_FLAG_WRITE,
        );

        // `Encoder::drop` writes the trailer, which libavformat only allows
        // after a successful header write, so the encoder is only built once
        // the header is out and a failed header releases the context by hand.
        if let Err(err) = crate::renderer::encoder::write_header(output_format_ctx, true) {
            audio_stream.free();
            if !(*output_format_ctx).pb.is_null() {
                avio_closep(&mut (*output_format_ctx).pb);
            }
            avformat_free_context(output_format_ctx);
            return Err(err);
        }

        Ok(Encoder {
            video_stream: Stream {
                st: output_video_stream,
                enc: std::ptr::null_mut(),
                variant: StreamVariant::Video,
            },
            audio_stream: Some(audio_stream),
            oc: output_format_ctx,
        })
    }
}

// This function is replicating the logic of validating non-monotous dts from ffmpeg
// https://github.com/FFmpeg/FFmpeg/blob/ea3d24bbe3c58b171e55fe2151fc7ffaca3ab3d2/fftools/ffmpeg_mux.c#L108-L126
//
// Make sure that logic is basically adding 1 to the max decoding timestamp which is the last muxed
// packet dts. Which is likely not safe enough.
unsafe fn validate_non_monotous_dts(
    packet: *mut AVPacket,
    last_mux_dts: &mut i64,
    av_format_context: *mut AVFormatContext,
) -> Result<(), RenderEncodingError> {
    unsafe {
        let max: i64 = *last_mux_dts
            + match (*(*av_format_context).oformat).flags & AVFMT_TS_NONSTRICT == 0 {
                true => 1,
                false => 0,
            };

        if (*packet).dts < max {
            if (*packet).pts >= (*packet).dts {
                (*packet).pts = (*packet).pts.max(max);
            }

            (*packet).dts = max;
        }

        Ok(())
    }
}

impl Encoder {
    pub unsafe fn fill_audio_stream(
        &self,
        audio_map: Option<&ResolvedAudioMap<AudioTimelineSamples>>,
        ctx: &FFramesContext,
        frame_range: std::ops::Range<usize>,
        mix_options: crate::AudioMixOptions,
        logger: &Arc<dyn super::fframes_logger::FFramesLogger>,
    ) -> Result<(), RenderEncodingError> {
        unsafe {
            if let (Some(audio_map), Some(audio_stream)) = (audio_map, self.audio_stream.as_ref()) {
                // The encoder may run at another rate than requested (Opus is 48 kHz only);
                // mix at the rate it actually encodes.
                let output_time_base = crate::TimeBase {
                    fps: ctx.time_base.fps,
                    sample_rate: (*audio_stream.enc).sample_rate as usize,
                };
                let first_sample =
                    AudioTimelineSamples::from_frames(frame_range.start, &output_time_base)
                        .as_usize();
                let end_sample =
                    AudioTimelineSamples::from_frames(frame_range.end, &output_time_base)
                        .as_usize();
                let total_samples =
                    AudioTimelineSamples::from_frames(ctx.duration_in_frames, &output_time_base)
                        .as_usize();
                let stream_samples = end_sample - first_sample;

                let mut mixer = crate::AudioMixer::new_rescaled(
                    Some(audio_map),
                    ctx.time_base.sample_rate,
                    ctx.media_source,
                    output_time_base.sample_rate,
                    first_sample..end_sample,
                    total_samples,
                    mix_options,
                );
                for file in mixer.missing_files() {
                    logger.warn(&format!(
                        "audio \"{file}\" is in the audio map but not in the media provider"
                    ));
                }

                let mut audio_frame = EncoderFrame::new(audio_stream)?;
                let capabilities = (*(*audio_stream.enc).codec).capabilities;
                let variable_frame_size =
                    capabilities & AV_CODEC_CAP_VARIABLE_FRAME_SIZE as i32 != 0;
                let small_last_frame =
                    variable_frame_size || capabilities & AV_CODEC_CAP_SMALL_LAST_FRAME as i32 != 0;
                // Variable frame size encoders (pcm) report 0, the frame buffer holds 10000.
                let frame_size = match (*audio_stream.enc).frame_size as usize {
                    0 => 4096,
                    size => size,
                };

                // Progress reporting must never abort the encoding itself.
                let _ = logger.init_audio_encoding(stream_samples.div_ceil(frame_size));

                let mut left = vec![0.; frame_size];
                let mut right = vec![0.; frame_size];
                let mut audio_frame_pts = 0usize;
                while audio_frame_pts < stream_samples {
                    let remaining = stream_samples - audio_frame_pts;
                    let samples = if small_last_frame {
                        remaining.min(frame_size)
                    } else {
                        frame_size
                    };

                    left.fill(0.);
                    right.fill(0.);
                    let mixed = remaining.min(samples);
                    mixer.render(
                        first_sample + audio_frame_pts,
                        &mut left[..mixed],
                        &mut right[..mixed],
                    );

                    audio_frame.fill_from_stereo(
                        audio_frame_pts as i64,
                        &left[..samples],
                        &right[..samples],
                    )?;
                    self.send_frame(audio_stream, &audio_frame)?;

                    logger.log_audio_frame();
                    audio_frame_pts += samples;
                }
                self.flush_stream(audio_stream)?;

                // The encoder holds back its last frames (AAC has a 1024 sample delay).
                self.flush_stream(audio_stream)?;
                logger.finish_audio_encoding();
            }

            Ok(())
        }
    }

    unsafe fn fill_streams_from_files(&self, files: &[PathBuf]) -> Result<(), RenderEncodingError> {
        unsafe {
            let mut last_video_mux_dts: Option<i64> = None;
            let mut next_video_start = 0i64;
            let mut last_audio_mux_dts: Option<i64> = None;
            let mut packet = AvPacketAutoFree::new();

            for file in files.iter() {
                let mut input_format_ctx = std::ptr::null_mut();

                // Open the file once and find both video and audio streams
                let input_file = CString::new(file.to_string_lossy().as_ref())
                    .map_err(RenderEncodingError::CStringError)?;

                ffmpeg_action!(
                    avformat_open_input(
                        &mut input_format_ctx,
                        input_file.as_ptr(),
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                    ),
                    RenderEncodingError::CantOpenFile(file.to_owned())
                );

                ffmpeg_action!(
                    avformat_find_stream_info(input_format_ctx, std::ptr::null_mut()),
                    RenderEncodingError::CantOpenFile(file.to_owned())
                );

                let streams = std::slice::from_raw_parts_mut(
                    (*input_format_ctx).streams,
                    (*input_format_ctx).nb_streams as usize,
                );

                let mut input_video_stream = std::ptr::null_mut();
                let mut input_audio_stream = std::ptr::null_mut();

                // Find video and audio streams
                for stream in streams {
                    let codec = (*stream.to_owned()).codecpar;
                    match (*codec).codec_type {
                        AVMediaType::AVMEDIA_TYPE_VIDEO => input_video_stream = *stream,
                        AVMediaType::AVMEDIA_TYPE_AUDIO => input_audio_stream = *stream,
                        _ => {}
                    }
                }

                if input_video_stream.is_null() {
                    avformat_close_input(&mut input_format_ctx);
                    return Err(RenderEncodingError::MissingVideoStreamInFile(
                        file.to_owned(),
                    ));
                }

                let mut video_shift: Option<i64> = None;
                let mut file_video_end = next_video_start;

                loop {
                    let res = av_read_frame(input_format_ctx, packet.get());
                    if res < 0 {
                        break;
                    }

                    let packet_stream_index = (*packet.get()).stream_index;
                    let input_streams = std::slice::from_raw_parts(
                        (*input_format_ctx).streams,
                        (*input_format_ctx).nb_streams as usize,
                    );
                    let input_stream = input_streams[packet_stream_index as usize];
                    let codec_type = (*(*input_stream).codecpar).codec_type;

                    match codec_type {
                        AVMediaType::AVMEDIA_TYPE_VIDEO => {
                            // Handle video packet (preserve original keyframe flags)
                            packet.get_mut().stream_index = (*self.video_stream.st).index;

                            av_packet_rescale_ts(
                                packet.get(),
                                (*input_video_stream).time_base,
                                (*self.video_stream.st).time_base,
                            );

                            // Every file continues exactly where the previous one ended.
                            // Its own start is not reliable: mp4 stores a non zero start
                            // in an edit list with millisecond precision.
                            let shift = *video_shift.get_or_insert_with(|| {
                                let start = if (*input_video_stream).start_time == AV_NOPTS_VALUE {
                                    (*packet.get()).pts
                                } else {
                                    av_rescale_q(
                                        (*input_video_stream).start_time,
                                        (*input_video_stream).time_base,
                                        (*self.video_stream.st).time_base,
                                    )
                                };
                                next_video_start - start
                            });
                            packet.get_mut().pts += shift;
                            packet.get_mut().dts += shift;
                            file_video_end =
                                file_video_end.max((*packet.get()).pts + (*packet.get()).duration);

                            if let Some(last_mux_dts) = last_video_mux_dts.as_mut() {
                                validate_non_monotous_dts(packet.get(), last_mux_dts, self.oc)?;
                            }
                            last_video_mux_dts = Some((*packet.get()).dts);

                            let ret = av_interleaved_write_frame(self.oc, packet.get());
                            if ret < 0 {
                                avformat_close_input(&mut input_format_ctx);
                                let error_description =
                                    crate::renderer::encoder::av_error_to_string(ret);
                                return Err(RenderEncodingError::CantWriteFrame(error_description));
                            }
                        }
                        AVMediaType::AVMEDIA_TYPE_AUDIO => {
                            // Handle audio packet if we have an audio stream
                            if !input_audio_stream.is_null()
                                && let Some(audio_stream) = self.audio_stream.as_ref()
                            {
                                packet.get_mut().stream_index = (*audio_stream.st).index;

                                // Apply DTS validation for audio packets too
                                if let Some(last_mux_dts) = last_audio_mux_dts.as_mut() {
                                    validate_non_monotous_dts(packet.get(), last_mux_dts, self.oc)?;
                                }
                                last_audio_mux_dts = Some((*packet.get()).dts);

                                av_packet_rescale_ts(
                                    packet.get(),
                                    (*input_audio_stream).time_base,
                                    (*audio_stream.st).time_base,
                                );
                                let ret = av_interleaved_write_frame(self.oc, packet.get());
                                if ret < 0 {
                                    avformat_close_input(&mut input_format_ctx);
                                    let error_description =
                                        crate::renderer::encoder::av_error_to_string(ret);

                                    return Err(RenderEncodingError::CantWriteFrame(
                                        error_description,
                                    ));
                                }
                            }
                        }
                        _ => {
                            // Skip other types of packets
                        }
                    }
                }

                avformat_close_input(&mut input_format_ctx);
                next_video_start = file_video_end;
            }

            Ok(())
        }
    }
}

pub unsafe fn concat_video_files_with_audio(
    files: &[PathBuf],
    output: &Path,
    audio_map: Option<&ResolvedAudioMap<AudioTimelineSamples>>,
    render_options: &RenderOptions,
    ctx: &FFramesContext,
    logger: &Arc<dyn super::fframes_logger::FFramesLogger>,
) -> Result<(), RenderEncodingError> {
    unsafe {
        let encoder = create_encoder_copy_from_file(&files[0], output, render_options)?;

        encoder.fill_streams_from_files(files)?;

        if encoder.audio_stream.is_some() {
            encoder.fill_audio_stream(
                audio_map,
                ctx,
                render_options.output_frame_range(ctx.duration_in_frames),
                render_options.audio_mix,
                logger,
            )?;
        }

        Ok(())
    }
}
