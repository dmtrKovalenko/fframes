use crate::EncoderOptions;
use crate::ffmpeg_action;
use crate::ffmpeg_helper::MONO_CH_LAYOUT;
use crate::ffmpeg_loggable_action;
use crate::renderer_error::{RenderEncodingError, RenderEncodingResult};
use ffmpeg_sys_fframes::*;
use std::ffi::CStr;
use std::ffi::CString;
#[derive(Clone, Copy)]
pub enum StreamVariant {
    Video,
    #[allow(dead_code)]
    Audio(*mut SwrContext),
}

#[derive(Clone)]
pub struct Stream {
    pub(crate) st: *mut AVStream,
    pub(crate) enc: *mut AVCodecContext,
    pub(crate) variant: StreamVariant,
}

unsafe impl Send for Stream {}
unsafe impl Sync for Stream {}

pub unsafe fn validate_sample_rate_fits_codec(codec: *const AVCodec, sample_rate: i32) -> i32 {
    unsafe {
        if (*codec).supported_samplerates.is_null() {
            return sample_rate; // we are likely in some bad state here
        }

        let mut i = 0;
        // it is terminated by 0
        while *(*codec).supported_samplerates.add(i) != 0 {
            if *(*codec).supported_samplerates.add(i) == sample_rate {
                return sample_rate;
            }
            i += 1;
        }

        *(*codec).supported_samplerates
    }
}

unsafe fn is_pixel_format_supported(
    pixel_format: AVPixelFormat,
    supported_formats: *const AVPixelFormat,
) -> bool {
    let mut index = 0;
    loop {
        let format = unsafe { *supported_formats.offset(index) };
        if format == AVPixelFormat::AV_PIX_FMT_NONE {
            return false;
        }

        if format == pixel_format {
            return true;
        }

        index += 1;
    }
}

impl Stream {
    /// Can't implement this as a triat cause it needs to be called in specific order
    pub fn free(&mut self) {
        unsafe {
            // in case encoder is not needed (remux) we won't allocate the encoder
            if !self.enc.is_null() {
                avcodec_send_frame(self.enc, std::ptr::null_mut());
                avcodec_close(self.enc);
                avcodec_free_context(&mut self.enc);

                if let StreamVariant::Audio(mut swr_ctx) = self.variant {
                    swr_free(&mut swr_ctx);
                }
            }
        }
    }

    pub unsafe fn get_frames_in_stream(&self) -> i64 {
        unsafe { (*self.st).nb_frames }
    }

    pub fn set_encoder_threads_count(&self, count: usize) {
        unsafe {
            (*self.enc).thread_count = count as i32;
        }
    }

    pub(crate) unsafe fn prepare_stream_codec(
        preferred_codec_name: Option<&str>,
        default_codec_id: AVCodecID,
        oc: *mut AVFormatContext,
    ) -> RenderEncodingResult<(
        *const AVCodec,
        AVCodecID,
        *mut AVStream,
        *mut AVCodecContext,
    )> {
        unsafe {
            let mut codec = if let Some(preferred_codec_name) = preferred_codec_name {
                let codec_name = CString::new(preferred_codec_name)
                    .map_err(RenderEncodingError::CStringError)?;

                avcodec_find_encoder_by_name(codec_name.as_ptr())
            } else {
                std::ptr::null_mut()
            };

            if codec.is_null() {
                codec = avcodec_find_encoder(default_codec_id);

                if let Some(preferred_codec_name) = preferred_codec_name {
                    let found_codec_name = CStr::from_ptr((*codec).name);

                    eprintln!(
                        "Warning: Can not find codec {preferred_codec_name}, continue with {codec_name}",
                        codec_name = found_codec_name.to_str().unwrap_or("unknown codec name")
                    );
                }
            }

            if codec.is_null() {
                return Err(RenderEncodingError::CannotLocateCodec);
            }

            let codec_id = (*codec).id;

            let st = avformat_new_stream(oc, std::ptr::null_mut());
            (*st).id = ((*oc).nb_streams - 1) as i32;
            let c = avcodec_alloc_context3(codec);
            if c.is_null() {
                return Err(RenderEncodingError::CantAllocate(
                    "encoding context".to_owned(),
                ));
            }

            Ok((codec, codec_id, st, c))
        }
    }

    pub(crate) unsafe fn make_video(
        width: i32,
        height: i32,
        fps: i32,
        oc: *mut AVFormatContext,
        encoder_options: &EncoderOptions,
    ) -> RenderEncodingResult<Self> {
        unsafe {
            let (codec, codec_id, st, c) = Self::prepare_stream_codec(
                encoder_options.preferred_video_codec,
                (*(*oc).oformat).video_codec,
                oc,
            )?;

            (*c).codec_id = codec_id;
            (*c).width = width;
            (*c).height = height;
            (*st).time_base = AVRational { num: 1, den: fps };
            (*c).time_base = (*st).time_base;

            if !is_pixel_format_supported(encoder_options.pixel_format, (*codec).pix_fmts) {
                return Err(RenderEncodingError::InvalidPixFmt(
                    encoder_options.pixel_format,
                ));
            }

            (*c).pix_fmt = encoder_options.pixel_format;
            (*c).gop_size = encoder_options.gop_size;
            (*c).qmin = encoder_options.qmin;
            (*c).qmax = encoder_options.qmax;
            (*c).qcompress = encoder_options.qcompress;
            (*c).max_qdiff = encoder_options.max_qdiff;
            (*c).bit_rate_tolerance = encoder_options.bitrate_tolerance;

            if let Some(video_bitrate) = encoder_options.video_bitrate {
                (*c).bit_rate = video_bitrate;
            }

            if (*(*oc).oformat).flags & AVFMT_GLOBALHEADER != 0 {
                (*c).flags |= AV_CODEC_FLAG_GLOBAL_HEADER as i32;
            }

            let opts: *mut *mut AVDictionary = &mut std::ptr::null_mut();

            if let Some(codec_params) = encoder_options.codec_params {
                for (param, value) in codec_params {
                    let c_param =
                        CString::new(*param).map_err(RenderEncodingError::CStringError)?;
                    let c_value =
                        CString::new(*value).map_err(RenderEncodingError::CStringError)?;

                    av_dict_set(opts, c_param.as_ptr(), c_value.as_ptr(), 0);
                }
            }

            ffmpeg_loggable_action!(avcodec_open2(c, codec, opts));
            ffmpeg_loggable_action!(avcodec_parameters_from_context((*st).codecpar, c));

            if let Some((tag, options)) = encoder_options.video_tag.zip((*st).codecpar.as_mut()) {
                options.codec_tag = tag as u32;
            }

            av_dict_free(opts);
            Ok(Stream {
                st,
                enc: c,
                variant: StreamVariant::Video,
            })
        }
    }

    pub(crate) unsafe fn make_audio(
        sample_rate: i32,
        oc: *mut AVFormatContext,
        encoder_options: &EncoderOptions,
    ) -> RenderEncodingResult<Self> {
        unsafe {
            let (codec, _codec_id, st, c) = Self::prepare_stream_codec(
                encoder_options.preferred_audio_codec,
                (*(*oc).oformat).audio_codec,
                oc,
            )?;

            let validated_sample_rate = validate_sample_rate_fits_codec(codec, sample_rate);
            if validated_sample_rate != sample_rate {
                eprintln!(
                    "Warning: sample_rate ({sample_rate}) provided in encoder_options are not available for the codec, using {validated_sample_rate} instead",
                );
            }

            (*c).sample_fmt = encoder_options.sample_format;
            (*c).sample_rate = validated_sample_rate;
            (*c).bit_rate = encoder_options.audio_bitrate.unwrap_or(192000);
            (*st).time_base = AVRational {
                num: 1,
                den: sample_rate,
            };

            (*c).ch_layout = MONO_CH_LAYOUT;
            if let Some((tag, options)) = encoder_options.audio_tag.zip((*st).codecpar.as_mut()) {
                options.codec_tag = tag as u32;
            }

            // TODO pass user options
            let opts: *mut *mut AVDictionary = &mut std::ptr::null_mut();

            ffmpeg_loggable_action!(avcodec_open2(c, codec, opts));
            ffmpeg_loggable_action!(avcodec_parameters_from_context((*st).codecpar, c));

            let swr_ctx = swr_alloc();
            if swr_ctx.is_null() {
                return Err(RenderEncodingError::Internal(
                    "Can not allocate swr".to_owned(),
                ));
            }

            Self::set_swr_option(swr_ctx, "in_sample_rate", (*c).sample_rate);
            Self::set_swr_option(swr_ctx, "out_sample_rate", (*c).sample_rate);

            Self::set_swr_chlayout(swr_ctx, "in_chlayout", &MONO_CH_LAYOUT);
            Self::set_swr_chlayout(swr_ctx, "out_chlayout", &MONO_CH_LAYOUT);

            Self::set_swr_fmt(swr_ctx, "in_sample_fmt", AVSampleFormat::AV_SAMPLE_FMT_FLTP);
            Self::set_swr_fmt(swr_ctx, "out_sample_fmt", (*c).sample_fmt);

            ffmpeg_action!(
                swr_init(swr_ctx),
                RenderEncodingError::Internal("Can not init swr".to_owned())
            );

            Ok(Stream {
                st,
                enc: c,
                variant: StreamVariant::Audio(swr_ctx),
            })
        }
    }

    pub(crate) unsafe fn set_swr_option(swr_ctx: *mut SwrContext, name: &str, val: i32) {
        if let Ok(name) = CString::new(name).map_err(RenderEncodingError::CStringError) {
            unsafe {
                av_opt_set_int(
                    swr_ctx as *mut std::ffi::c_void,
                    name.as_ptr(),
                    val.into(),
                    0,
                );
            }
        }
    }

    pub(crate) unsafe fn set_swr_chlayout(
        swr_ctx: *mut SwrContext,
        name: &str,
        val: &AVChannelLayout,
    ) {
        if let Ok(name) = CString::new(name).map_err(RenderEncodingError::CStringError) {
            unsafe {
                av_opt_set_chlayout(
                    swr_ctx as *mut std::ffi::c_void,
                    name.as_ptr(),
                    val as *const AVChannelLayout,
                    0,
                );
            }
        }
    }

    pub(crate) unsafe fn set_swr_fmt(swr_ctx: *mut SwrContext, name: &str, val: AVSampleFormat) {
        if let Ok(name) = CString::new(name).map_err(RenderEncodingError::CStringError) {
            unsafe {
                av_opt_set_sample_fmt(swr_ctx as *mut std::ffi::c_void, name.as_ptr(), val, 0);
            }
        }
    }
}
