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
        let audio_stream =
            Stream::make_audio(output_format_ctx, &render_options.audio_encoder_options)?;

        let encoder = Encoder {
            video_stream: Stream {
                st: output_video_stream,
                enc: std::ptr::null_mut(),
                variant: StreamVariant::Video,
            },
            audio_stream: Some(audio_stream),
            oc: output_format_ctx,
        };

        avcodec_parameters_copy(
            (*output_video_stream).codecpar,
            (*input_video_stream).codecpar,
        );
        (*encoder.video_stream.st).time_base = (*input_video_stream).time_base;

        avformat_close_input(&mut input_format_ctx);
        avio_open(
            &mut (*output_format_ctx).pb,
            output_file.as_ptr(),
            AVIO_FLAG_WRITE,
        );

        avformat_write_header(encoder.oc, std::ptr::null_mut());

        Ok(encoder)
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
    ) -> Result<(), RenderEncodingError> {
        unsafe {
            if let (Some(audio_map), Some(audio_stream)) = (audio_map, self.audio_stream.as_ref()) {
                let stream_duration_in_samples =
                    AudioTimelineSamples::from_frames(ctx.duration_in_frames, &ctx.time_base);

                let mut audio_frame = EncoderFrame::new(audio_stream)?;
                let mut audio_frame_pts = 0usize;
                let frame_size = (*audio_stream.enc).frame_size as usize;

                while audio_frame_pts <= stream_duration_in_samples.as_usize() {
                    let audio_data = ctx.get_mixed_audio_data_in_fltp(
                        audio_map,
                        AudioTimelineSamples::from_usize(audio_frame_pts),
                        frame_size,
                    );

                    audio_frame.fill_from_audio_data(audio_frame_pts as i64, audio_data);
                    self.send_frame(audio_stream, &audio_frame)?;

                    audio_frame_pts += frame_size;
                }
            }

            Ok(())
        }
    }

    unsafe fn fill_video_stream_from_files(
        &self,
        files: &[PathBuf],
    ) -> Result<(), RenderEncodingError> {
        unsafe {
            let mut last_mux_dts: Option<i64> = None;
            let mut packet = AvPacketAutoFree::new();

            for file in files.iter() {
                let mut input_format_ctx = std::ptr::null_mut();

                let input_video_stream =
                    open_file_stream(file, &mut input_format_ctx, AVMediaType::AVMEDIA_TYPE_VIDEO)?;

                loop {
                    let res = av_read_frame(input_format_ctx, packet.get());
                    if res < 0 {
                        break;
                    }

                    packet.get_mut().flags |= AV_PKT_FLAG_KEY;

                    if let Some(last_mux_dts) = last_mux_dts.as_mut() {
                        validate_non_monotous_dts(packet.get(), last_mux_dts, self.oc)?;
                    }

                    last_mux_dts = Some((*packet.get()).dts);

                    av_packet_rescale_ts(
                        packet.get(),
                        (*input_video_stream).time_base,
                        (*self.video_stream.st).time_base,
                    );
                    av_interleaved_write_frame(self.oc, packet.get());
                }

                avformat_close_input(&mut input_format_ctx);
            }

            Ok(())
        }
    }
}

pub unsafe fn concat_video_files_with_audio(
    files: &[PathBuf],
    output: &Path,
    concurrency: i32,
    audio_map: Option<&ResolvedAudioMap<AudioTimelineSamples>>,
    render_options: &RenderOptions,
    ctx: &FFramesContext,
) -> Result<(), RenderEncodingError> {
    unsafe {
        let encoder = create_encoder_copy_from_file(&files[0], output, render_options)?;

        encoder.fill_video_stream_from_files(files)?;
        if let Some(audio_stream) = &encoder.audio_stream {
            (*audio_stream.enc).thread_count = concurrency;
            encoder.fill_audio_stream(audio_map, ctx)?;
        }

        Ok(())
    }
}
