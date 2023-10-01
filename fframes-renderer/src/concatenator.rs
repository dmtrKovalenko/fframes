use ffmpeg_next::sys::*;
use fframes::{AudioTimelineSamples, AudioTimelineUnit, FFramesContext, ResolvedAudioMap};
use std::ffi::CString;

use crate::{
    encoder::Encoder,
    encoder_frame::EncoderFrame,
    ffmpeg_action,
    renderer_error::{RenderEncodingError, RenderEncodingResult},
    stream::Stream,
    stream::StreamVariant,
    EncoderOptions,
};

unsafe fn open_file_stream(
    filename: &str,
    input_format_ctx: &mut *mut AVFormatContext,
    codec_type: AVMediaType,
) -> RenderEncodingResult<*mut AVStream> {
    let input_file = CString::new(filename).map_err(RenderEncodingError::CStringError)?;

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

unsafe fn create_encoder_copy_from_file(
    file: &str,
    output: &str,
    encoder_options: &EncoderOptions,
) -> Result<Encoder, RenderEncodingError> {
    let mut input_format_ctx: *mut AVFormatContext = std::ptr::null_mut();
    let mut output_format_ctx: *mut AVFormatContext = std::ptr::null_mut();

    let input_video_stream =
        open_file_stream(file, &mut input_format_ctx, AVMediaType::AVMEDIA_TYPE_VIDEO)?;

    let output_file = CString::new(output).map_err(RenderEncodingError::CStringError)?;
    avformat_alloc_output_context2(
        &mut output_format_ctx,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        output_file.as_ptr(),
    );

    let output_video_stream = avformat_new_stream(output_format_ctx, std::ptr::null_mut());
    let audio_stream = Stream::make_audio(
        encoder_options.sample_rate as i32,
        output_format_ctx,
        encoder_options,
    )?;

    let encoder = Encoder {
        video_stream: Stream {
            st: output_video_stream,
            enc: std::ptr::null_mut(),
            variant: StreamVariant::Video,
        },
        audio_stream: Some(audio_stream),
        oc: output_format_ctx,
        b_frames_count: 0,
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

unsafe fn fill_video_stream_from_files(
    encoder: &mut Encoder,
    files: &[String],
) -> Result<(), RenderEncodingError> {
    let mut start_time = 0;
    let packet = av_packet_alloc();

    for file in files.iter() {
        let mut input_format_ctx = std::ptr::null_mut();

        let input_video_stream =
            open_file_stream(file, &mut input_format_ctx, AVMediaType::AVMEDIA_TYPE_VIDEO)?;

        loop {
            let res = av_read_frame(input_format_ctx, packet);
            if res < 0 {
                break;
            }

            (*packet).flags |= AV_PKT_FLAG_KEY;

            // This calculates the delta in pts based on the duration when this file must be appeared
            let delta = av_rescale_q(
                start_time,
                AV_TIME_BASE_Q,
                (*encoder.video_stream.st).time_base,
            );

            (*packet).pts += delta;
            (*packet).dts += delta;

            av_packet_rescale_ts(
                packet,
                (*input_video_stream).time_base,
                (*encoder.video_stream.st).time_base,
            );
            av_interleaved_write_frame(encoder.oc, packet);
        }

        start_time += (*input_format_ctx).duration;
        avformat_close_input(&mut input_format_ctx);
    }

    Ok(())
}

pub unsafe fn fill_audio_stream(
    encoder: &mut Encoder,
    audio_map: Option<&ResolvedAudioMap<AudioTimelineSamples>>,
    ctx: &FFramesContext,
) -> Result<(), RenderEncodingError> {
    if let (Some(audio_map), Some(audio_stream)) = (audio_map, encoder.audio_stream) {
        let stream_duration_in_samples =
            AudioTimelineSamples::from_frames(ctx.duration_in_frames, &ctx.time_base);

        let mut audio_frame =
            EncoderFrame::make(&encoder.audio_stream.ok_or_else(|| {
                RenderEncodingError::Internal("Missing audio_stream".to_owned())
            })?)?;

        let mut audio_frame_pts = 0usize;
        let frame_size = (*audio_stream.enc).frame_size as usize;

        while audio_frame_pts <= stream_duration_in_samples.as_usize() {
            let audio_data = ctx.get_mixed_audio_data_in_fltp(
                audio_map,
                AudioTimelineSamples::from_usize(audio_frame_pts),
                frame_size,
            );

            audio_frame.fill_from_audio_data(audio_frame_pts as i64, audio_data);
            encoder.send_frame(&audio_stream, &audio_frame)?;

            audio_frame_pts += frame_size;
        }

        avcodec_send_frame(audio_stream.enc, std::ptr::null_mut());
        audio_stream.free();
    }

    Ok(())
}

pub unsafe fn concat_video_files_with_audio(
    files: &[String],
    output: &str,
    audio_map: Option<&ResolvedAudioMap<AudioTimelineSamples>>,
    encoder_options: &EncoderOptions,
    ctx: &FFramesContext,
) -> Result<(), RenderEncodingError> {
    let mut encoder = create_encoder_copy_from_file(files[0].as_str(), output, encoder_options)?;

    fill_video_stream_from_files(&mut encoder, files)?;
    fill_audio_stream(&mut encoder, audio_map, ctx)?;

    av_write_trailer(encoder.oc);
    avio_close((*encoder.oc).pb);

    Ok(())
}
