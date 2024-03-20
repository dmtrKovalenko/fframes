// Imports are defined here
use crate::error::Result;
use crate::FFramesMediaError;
use ffmpeg_next::sys::*;
use std::ffi::CString;
use std::io::{self, Cursor, Read};
use std::ptr;

#[inline(always)]
#[allow(non_snake_case)]
pub const fn FFMPEG_AVERROR(e: std::os::raw::c_int) -> std::os::raw::c_int {
    -e
}

unsafe fn decode_packet(
    sample_rate: u32,
    dec_ctx: *mut AVCodecContext,
    swr_ctx: *mut SwrContext,
    pkt: *mut AVPacket,
    frame: *mut AVFrame,
    samples: &mut Vec<f32>,
) -> Result<()> {
    let mut ret;

    /* send the packet with the compressed data to the decoder */
    ret = avcodec_send_packet(dec_ctx, pkt);
    if ret < 0 {
        return Err(FFramesMediaError::LibAVAudioDecodingError((
            ret,
            "Error submitting packet to decoder".to_string(),
        )));
    }

    while ret >= 0 {
        ret = avcodec_receive_frame(dec_ctx, frame);

        match ret {
            AVERROR_EOF => {
                return Ok(());
            }
            ret if ret == FFMPEG_AVERROR(EAGAIN) => {
                return Ok(());
            }
            ret if ret < 0 => {
                return Err(FFramesMediaError::LibAVAudioDecodingError((
                    ret,
                    "Error during decoding".to_string(),
                )));
            }
            _ => (),
        };

        let nb_samples = av_rescale_rnd(
            swr_get_delay(swr_ctx, (*dec_ctx).sample_rate.into()) + (*frame).nb_samples as i64,
            sample_rate as i64,
            (*dec_ctx).sample_rate.into(),
            AVRounding::AV_ROUND_UP,
        );

        // Here the samples are convert to fit fframes format which is FLTP (float planar) and
        // sample rate is equal to what user is requesting in the options.
        //
        // Make sure that to avoid allocating a temporary buffer and copying data twice we
        // are writing data directly to the samples slice owned by the rust renderer caller.
        // To do this we first resereve enough space in the slice (which is a vec)
        // and then taking a raw pointer with an offset to the current last element and `swr_convert`
        // is writing the output audio data directly to the slice.
        let current_length = samples.len();
        samples.reserve(nb_samples as usize);
        let ret = swr_convert(
            swr_ctx,
            // the input buffes is a C array of pointers (pointer of pointers) which is directly
            // compatible with sized array of pointer in rust [ptr; 1]
            // the size is fixed because audio is 1 channel only.
            [samples.as_mut_ptr().add(current_length)].as_ptr() as *mut *mut _,
            nb_samples as i32,
            (*frame).data.as_mut_ptr() as *mut _ as *mut *const u8,
            (*frame).nb_samples,
        );
        // Now manually set the length of the vec as it was changed outside of the rust knowledge
        samples.set_len(current_length + nb_samples as usize);

        if ret < 0 {
            return Err(FFramesMediaError::LibAVAudioDecodingError((
                ret,
                "Error while resampling".to_string(),
            )));
        }
    }

    Ok(())
}

unsafe extern "C" fn read_packet(
    opaque: *mut std::ffi::c_void,
    buf: *mut u8,
    buf_size: std::ffi::c_int,
) -> std::ffi::c_int {
    let cursor = &mut *(opaque as *mut Cursor<&[u8]>);
    let destination_buf = std::slice::from_raw_parts_mut(buf, buf_size as usize);

    match cursor.read(destination_buf).unwrap_or(0) as i32 {
        0 => AVERROR_EOF,
        ret => ret,
    }
}

const AV_TMP_BUF_SIZE: usize = 4096;

/// # Safety
/// Uses libav directly so it can't be safe. Let's hope for the best and avoid segfaults.
/// Tested on all of the most poopulate formats and codecs, perfectly supported wav, mp3, mp2,
/// flac, ogg, aac, alac, ac3, and opous.
///
/// # Planar audio
/// Make sure that fframes does not support stereo at all for now, so all the audio is decoded as
/// mono. For planar audio we take only first channel, make sure that it might be required to
/// normalize channels manually. To do so do here is a  simple ffmpeg command:
///
/// ```bash
/// ffmpeg -i input.mp3 -ac 1 output.mp3
/// ```
///
/// # Returns
///
/// A sample rate that and a vector of f32 fltp planar audio samples. If the sample_rate were not
/// provided uses original sample rate of the input audio file.
pub unsafe fn decode_audio(
    data: &[u8],
    filename: &str,
    sample_rate: Option<u32>,
) -> Result<(u32, Vec<f32>)> {
    av_log_set_level(AV_LOG_FATAL);
    let filename = std::ffi::CString::new(filename).unwrap();

    let mut fmt_context = avformat_alloc_context();
    if fmt_context.is_null() {
        return Err(FFramesMediaError::AudioDecodingError(
            "Could not allocate format context".to_string(),
        ));
    }

    let buf = av_malloc(AV_TMP_BUF_SIZE * AV_INPUT_BUFFER_PADDING_SIZE as usize) as *mut u8;
    let mut cursor = io::Cursor::new(data);
    let mut avio_ctx = avio_alloc_context(
        buf,
        AV_TMP_BUF_SIZE as i32,
        0,
        &mut cursor as *mut _ as *mut std::ffi::c_void,
        Some(read_packet),
        None,
        None,
    );

    (*fmt_context).pb = avio_ctx;

    let ret = avformat_open_input(
        &mut fmt_context,
        filename.as_ptr(),
        ptr::null_mut(),
        ptr::null_mut(),
    );

    if ret < 0 {
        return Err(FFramesMediaError::LibAVAudioDecodingError((
            ret,
            "Could not open file".to_string(),
        )));
    }

    let ret = avformat_find_stream_info(fmt_context, ptr::null_mut());
    if ret < 0 {
        return Err(FFramesMediaError::LibAVAudioDecodingError((
            ret,
            "Could not find stream info".to_string(),
        )));
    }

    let stream_idx = av_find_best_stream(
        fmt_context,
        AVMediaType::AVMEDIA_TYPE_AUDIO,
        -1,
        -1,
        ptr::null_mut(),
        0,
    );

    if stream_idx < 0 {
        return Err(FFramesMediaError::LibAVAudioDecodingError((
            stream_idx,
            "Could not find fitting audio stream in a file".to_string(),
        )));
    }

    let streams =
        std::slice::from_raw_parts_mut((*fmt_context).streams, (*fmt_context).nb_streams as usize);
    let audio_stream = *streams[stream_idx as usize];
    let codec_id = (*audio_stream.codecpar).codec_id;

    let codec = avcodec_find_decoder(codec_id);
    if codec.is_null() {
        return Err(FFramesMediaError::AudioDecodingError(
            "Could not find encoder".to_string(),
        ));
    }

    let mut decoding_ctx = avcodec_alloc_context3(codec);
    if decoding_ctx.is_null() {
        return Err(FFramesMediaError::AudioDecodingError(
            "Error while parsing".to_string(),
        ));
    }

    avcodec_parameters_to_context(decoding_ctx, audio_stream.codecpar);
    let ret = avcodec_open2(decoding_ctx, codec, ptr::null_mut());
    if ret < 0 {
        return Err(FFramesMediaError::LibAVAudioDecodingError((
            ret,
            "Could not open codec".to_string(),
        )));
    }

    let out_sample_rate = sample_rate.unwrap_or((*decoding_ctx).sample_rate as u32);

    let mut swr_ctx = swr_alloc();
    av_opt_set_int(
        swr_ctx as *mut _ as *mut std::ffi::c_void,
        CString::new("in_sample_rate")?.as_ptr(),
        (*decoding_ctx).sample_rate as i64,
        0,
    );
    av_opt_set_int(
        swr_ctx as *mut _ as *mut std::ffi::c_void,
        CString::new("out_sample_rate")?.as_ptr(),
        out_sample_rate as i64,
        0,
    );

    av_opt_set_chlayout(
        swr_ctx as *mut _ as *mut std::ffi::c_void,
        CString::new("ichl")?.as_ptr(),
        &(*decoding_ctx).ch_layout,
        0,
    );
    av_opt_set_channel_layout(
        swr_ctx as *mut _ as *mut std::ffi::c_void,
        CString::new("out_channel_layout")?.as_ptr(),
        AV_CH_LAYOUT_MONO as i64,
        0,
    );

    av_opt_set_sample_fmt(
        swr_ctx as *mut _ as *mut std::ffi::c_void,
        CString::new("in_sample_fmt")?.as_ptr(),
        (*decoding_ctx).sample_fmt,
        0,
    );
    av_opt_set_sample_fmt(
        swr_ctx as *mut _ as *mut std::ffi::c_void,
        CString::new("out_sample_fmt")?.as_ptr(),
        AVSampleFormat::AV_SAMPLE_FMT_FLTP,
        0,
    );

    let ret = swr_init(swr_ctx);
    if ret < 0 {
        return Err(FFramesMediaError::LibAVAudioDecodingError((
            ret,
            "Failed to initialize the resampler context".to_string(),
        )));
    }

    let mut avpkt = av_packet_alloc();
    let mut frame = av_frame_alloc();

    let mut samples = Vec::new();
    while av_read_frame(fmt_context, avpkt) >= 0 {
        if (*avpkt).stream_index != stream_idx {
            continue;
        }

        let res = decode_packet(
            out_sample_rate,
            decoding_ctx,
            swr_ctx,
            avpkt,
            frame,
            &mut samples,
        );

        av_frame_unref(frame);
        av_packet_unref(avpkt);

        if res.is_err() {
            break;
        }
    }

    // Flush the decoder and write EOF by sending empty packet
    decode_packet(
        out_sample_rate,
        decoding_ctx,
        swr_ctx,
        ptr::null_mut(),
        frame,
        &mut samples,
    )?;
    av_frame_unref(frame);

    avcodec_free_context(&mut decoding_ctx);
    avformat_close_input(&mut fmt_context);
    av_packet_free(&mut avpkt);
    av_frame_free(&mut frame);
    avio_context_free(&mut avio_ctx);
    swr_free(&mut swr_ctx);

    Ok((out_sample_rate, samples))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_decoding_mp3() {
        let result =
            unsafe { decode_audio(include_bytes!("../test_audio/audio.mp3"), "audio.mp3", None) };

        assert_eq!(result.unwrap().1.len(), 926255);
    }

    #[test]
    fn test_audio_decoding_flac() {
        let result = unsafe {
            decode_audio(
                include_bytes!("../test_audio/audio.flac"),
                "audio.flac",
                None,
            )
        };

        assert_eq!(result.unwrap().1.len(), 926100);
    }

    #[test]
    fn test_audio_decoding_wav() {
        let result =
            unsafe { decode_audio(include_bytes!("../test_audio/audio.wav"), "audio.wav", None) };

        assert_eq!(result.unwrap().1.len(), 926100);
    }

    #[test]
    fn test_audio_decoding_aac() {
        let result =
            unsafe { decode_audio(include_bytes!("../test_audio/audio.aac"), "audio.aac", None) };

        assert_eq!(result.unwrap().1.len(), 927744);
    }
}
