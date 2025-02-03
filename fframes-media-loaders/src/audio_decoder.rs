use crate::error::Result;
use crate::FFramesMediaError;
use ffmpeg_sys_fframes::*;
use std::ffi::CString;
use std::path::Path;
use std::ptr;

pub struct AudioDecoder {
    fmt_context: *mut AVFormatContext,
    decoding_ctx: *mut AVCodecContext,
    swr_ctx: *mut SwrContext,
    avpkt: *mut AVPacket,
    frame: *mut AVFrame,
    stream_idx: i32,
    out_sample_rate: u32,
}

impl AudioDecoder {
    pub fn new(filename: impl AsRef<Path>, sample_rate: Option<u32>) -> Result<Self> {
        unsafe {
            av_log_set_level(AV_LOG_FATAL);

            let filename = std::ffi::CString::new(filename.as_ref().to_string_lossy().as_ref())?;

            let mut fmt_context = avformat_alloc_context();
            if fmt_context.is_null() {
                return Err(FFramesMediaError::AudioDecodingError(
                    "Could not allocate format context".to_string(),
                ));
            }

            let ret = avformat_open_input(
                &mut fmt_context,
                filename.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
            );

            if ret < 0 {
                avformat_free_context(fmt_context);
                return Err(FFramesMediaError::LibAVAudioDecodingError((
                    ret,
                    "Could not open file".to_string(),
                )));
            }

            let ret = avformat_find_stream_info(fmt_context, ptr::null_mut());
            if ret < 0 {
                avformat_close_input(&mut fmt_context);
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
                avformat_close_input(&mut fmt_context);
                return Err(FFramesMediaError::LibAVAudioDecodingError((
                    stream_idx,
                    "Could not find fitting audio stream in the media file".to_string(),
                )));
            }

            let streams = std::slice::from_raw_parts_mut(
                (*fmt_context).streams,
                (*fmt_context).nb_streams as usize,
            );
            let audio_stream = *streams[stream_idx as usize];
            let codec_id = (*audio_stream.codecpar).codec_id;

            let codec = avcodec_find_decoder(codec_id);
            if codec.is_null() {
                avformat_close_input(&mut fmt_context);
                return Err(FFramesMediaError::AudioDecodingError(
                    "Could not find encoder".to_string(),
                ));
            }

            let mut decoding_ctx = avcodec_alloc_context3(codec);
            if decoding_ctx.is_null() {
                avformat_close_input(&mut fmt_context);
                return Err(FFramesMediaError::AudioDecodingError(
                    "Error while parsing".to_string(),
                ));
            }

            avcodec_parameters_to_context(decoding_ctx, audio_stream.codecpar);
            let ret = avcodec_open2(decoding_ctx, codec, ptr::null_mut());
            if ret < 0 {
                avcodec_free_context(&mut decoding_ctx);
                avformat_close_input(&mut fmt_context);
                return Err(FFramesMediaError::LibAVAudioDecodingError((
                    ret,
                    "Could not open codec".to_string(),
                )));
            }

            let out_sample_rate = sample_rate.unwrap_or((*decoding_ctx).sample_rate as u32);

            let swr_ctx = swr_alloc();
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
                CString::new("in_chlayout")?.as_ptr(),
                &(*decoding_ctx).ch_layout,
                0,
            );

            av_opt_set_chlayout(
                swr_ctx as *mut _ as *mut std::ffi::c_void,
                CString::new("out_chlayout")?.as_ptr(),
                &MONO_CH_LAYOUT,
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
                avcodec_free_context(&mut decoding_ctx);
                avformat_close_input(&mut fmt_context);
                return Err(FFramesMediaError::LibAVAudioDecodingError((
                    ret,
                    "Failed to initialize the resampler context".to_string(),
                )));
            }

            let mut avpkt = av_packet_alloc();
            if avpkt.is_null() {
                avcodec_free_context(&mut decoding_ctx);
                avformat_close_input(&mut fmt_context);
                return Err(FFramesMediaError::AudioDecodingError(
                    "Could not allocate packet".to_string(),
                ));
            }

            let frame = av_frame_alloc();
            if frame.is_null() {
                av_packet_free(&mut avpkt);
                avcodec_free_context(&mut decoding_ctx);
                avformat_close_input(&mut fmt_context);
                return Err(FFramesMediaError::AudioDecodingError(
                    "Could not allocate frame".to_string(),
                ));
            }

            Ok(Self {
                fmt_context,
                decoding_ctx,
                swr_ctx,
                avpkt,
                frame,
                stream_idx,
                out_sample_rate,
            })
        }
    }

    unsafe fn decode_packet(&mut self, samples: &mut Vec<f32>) -> Result<()> {
        let mut ret;

        ret = avcodec_send_packet(self.decoding_ctx, self.avpkt);
        if ret < 0 {
            return Err(FFramesMediaError::LibAVAudioDecodingError((
                ret,
                "Error submitting packet to decoder".to_string(),
            )));
        }

        while ret >= 0 {
            ret = avcodec_receive_frame(self.decoding_ctx, self.frame);

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
                swr_get_delay(self.swr_ctx, (*self.decoding_ctx).sample_rate.into())
                    + (*self.frame).nb_samples as i64,
                self.out_sample_rate as i64,
                (*self.decoding_ctx).sample_rate.into(),
                AVRounding::AV_ROUND_UP,
            );

            let current_length = samples.len();
            samples.reserve(nb_samples as usize);
            let ret = swr_convert(
                self.swr_ctx,
                [samples.as_mut_ptr().add(current_length)].as_ptr() as *mut *mut _,
                nb_samples as i32,
                (*self.frame).data.as_mut_ptr() as *mut _ as *mut *const u8,
                (*self.frame).nb_samples,
            );
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

    pub fn decode_all_samples(&mut self) -> Result<(u32, Vec<f32>)> {
        unsafe {
            let mut samples = Vec::new();

            while av_read_frame(self.fmt_context, self.avpkt) >= 0 {
                if (*self.avpkt).stream_index == self.stream_idx {
                    let decode_result = self.decode_packet(&mut samples);
                    if decode_result.is_err() {
                        av_packet_unref(self.avpkt);

                        break;
                    }
                }

                av_packet_unref(self.avpkt);
            }

            // Flush the decoder
            self.decode_packet(&mut samples)?;
            av_frame_unref(self.frame);

            Ok((self.out_sample_rate, samples))
        }
    }
}

impl Drop for AudioDecoder {
    fn drop(&mut self) {
        unsafe {
            if !self.decoding_ctx.is_null() {
                avcodec_free_context(&mut self.decoding_ctx);
            }
            if !self.fmt_context.is_null() {
                avformat_close_input(&mut self.fmt_context);
            }
            if !self.avpkt.is_null() {
                av_packet_free(&mut self.avpkt);
            }
            if !self.frame.is_null() {
                av_frame_free(&mut self.frame);
            }
            if !self.swr_ctx.is_null() {
                swr_free(&mut self.swr_ctx);
            }
        }
    }
}

#[inline(always)]
#[allow(non_snake_case)]
pub const fn FFMPEG_AVERROR(e: std::os::raw::c_int) -> std::os::raw::c_int {
    -e
}

const MONO_CH_LAYOUT: AVChannelLayout = AVChannelLayout {
    order: AVChannelOrder::AV_CHANNEL_ORDER_NATIVE,
    nb_channels: 1,
    u: AVChannelLayout__bindgen_ty_1 {
        mask: AV_CH_LAYOUT_MONO,
    },
    opaque: std::ptr::null_mut(),
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_audio_decoding_mp3() {
        let mut decoder = AudioDecoder::new(PathBuf::from("test_audio/audio.mp3"), None).unwrap();
        let result = decoder.decode_all_samples();
        assert_eq!(result.unwrap().1.len(), 926100);
    }

    #[test]
    fn test_audio_decoding_flac() {
        let mut decoder = AudioDecoder::new(PathBuf::from("test_audio/audio.flac"), None).unwrap();
        let result = decoder.decode_all_samples();
        assert_eq!(result.unwrap().1.len(), 926100);
    }

    #[test]
    fn test_audio_decoding_wav() {
        let mut decoder = AudioDecoder::new(PathBuf::from("test_audio/audio.wav"), None).unwrap();
        let result = decoder.decode_all_samples();
        assert_eq!(result.unwrap().1.len(), 926100);
    }

    #[test]
    fn test_audio_decoding_aac() {
        let mut decoder = AudioDecoder::new(PathBuf::from("test_audio/audio.aac"), None).unwrap();
        let result = decoder.decode_all_samples();
        assert_eq!(result.unwrap().1.len(), 927744);
    }
}
