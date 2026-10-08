use super::EncoderOptions;
use super::ffmpeg_helper::STEREO_CH_LAYOUT;
use super::frame_export::EncoderInput;
use super::pix_fmt::YuvMatrix;
use super::renderer_error::{RenderEncodingError, RenderEncodingResult};
use crate::ffmpeg_action;
use crate::ffmpeg_loggable_action;
use crate::media::ffmpeg_sys_fframes::*;
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

/// Returns the list of values the encoder supports for `config`, or `None` when the
/// encoder accepts any value (or libav can't tell).
///
/// # Safety
/// `T` must match the element type libav documents for `config`.
unsafe fn supported_codec_config<'a, T>(
    codec: *const AVCodec,
    config: AVCodecConfig,
) -> Option<&'a [T]> {
    unsafe {
        let mut configs: *const std::ffi::c_void = std::ptr::null();
        let mut count = 0;
        let ret = avcodec_get_supported_config(
            std::ptr::null(),
            codec,
            config,
            0,
            &raw mut configs,
            &raw mut count,
        );

        if ret < 0 || configs.is_null() || count <= 0 {
            return None;
        }

        Some(std::slice::from_raw_parts(
            configs.cast::<T>(),
            count as usize,
        ))
    }
}

pub unsafe fn validate_sample_rate_fits_codec(codec: *const AVCodec, sample_rate: i32) -> i32 {
    let supported =
        unsafe { supported_codec_config::<i32>(codec, AVCodecConfig::AV_CODEC_CONFIG_SAMPLE_RATE) };

    match supported {
        Some(rates) if !rates.contains(&sample_rate) => rates[0],
        _ => sample_rate,
    }
}

/// The requested sample format if the encoder takes it, otherwise one `fill_from_stereo` can
/// write (libopus, for example, only accepts `flt` and `s16`).
unsafe fn fit_sample_format(codec: *const AVCodec, requested: AVSampleFormat) -> AVSampleFormat {
    use AVSampleFormat::*;
    let supported = unsafe {
        supported_codec_config::<AVSampleFormat>(
            codec,
            AVCodecConfig::AV_CODEC_CONFIG_SAMPLE_FORMAT,
        )
    };

    match supported {
        Some(formats) if !formats.contains(&requested) => [
            AV_SAMPLE_FMT_FLTP,
            AV_SAMPLE_FMT_FLT,
            AV_SAMPLE_FMT_S16,
            AV_SAMPLE_FMT_S16P,
            AV_SAMPLE_FMT_S32,
            AV_SAMPLE_FMT_S32P,
        ]
        .into_iter()
        .find(|format| formats.contains(format))
        .unwrap_or(formats[0]),
        _ => requested,
    }
}

/// The pixel formats the encoder takes, `None` when it accepts any (or libav can't tell).
pub(crate) unsafe fn supported_pixel_formats<'a>(
    codec: *const AVCodec,
) -> Option<&'a [AVPixelFormat]> {
    unsafe {
        supported_codec_config::<AVPixelFormat>(codec, AVCodecConfig::AV_CODEC_CONFIG_PIX_FORMAT)
    }
}

unsafe fn is_pixel_format_supported(codec: *const AVCodec, pixel_format: AVPixelFormat) -> bool {
    unsafe { supported_pixel_formats(codec) }.is_none_or(|formats| formats.contains(&pixel_format))
}

/// The encoder named `preferred_encoder`, otherwise the default encoder of `codec_id`.
pub(crate) unsafe fn find_encoder(
    preferred_encoder: Option<&str>,
    codec_id: AVCodecID,
    warn_about_fallback: bool,
) -> RenderEncodingResult<*const AVCodec> {
    unsafe {
        let mut codec = if let Some(encoder) = preferred_encoder {
            let codec_name = CString::new(encoder).map_err(RenderEncodingError::CStringError)?;

            avcodec_find_encoder_by_name(codec_name.as_ptr())
        } else {
            std::ptr::null()
        };

        if codec.is_null() {
            codec = avcodec_find_encoder(codec_id);

            if let Some(preferred_codec_name) = preferred_encoder
                && !codec.is_null()
                && warn_about_fallback
            {
                let found_encoder_name = CStr::from_ptr((*codec).name);

                eprintln!(
                    "Warning: Can not find encoder {preferred_codec_name}, continue with {found_encoder_name}",
                    found_encoder_name =
                        found_encoder_name.to_str().unwrap_or("unknown codec name")
                );
            }
        }

        if codec.is_null() {
            return Err(RenderEncodingError::CannotLocateCodec);
        }

        Ok(codec)
    }
}

/// Configures the context of a video encoder for `input` and opens it.
pub(crate) unsafe fn open_video_encoder(
    c: *mut AVCodecContext,
    codec: *const AVCodec,
    (width, height, fps): (i32, i32, i32),
    global_header: bool,
    encoder_options: &EncoderOptions,
    input: &EncoderInput,
    thread_count: i32,
) -> RenderEncodingResult<()> {
    unsafe {
        (*c).codec_id = (*codec).id;
        (*c).width = width;
        (*c).height = height;
        // avcodec_open2 applies codec_params afterwards, so an explicit `threads`
        // option still overrides this default (including `threads=0` for auto).
        (*c).thread_count = thread_count;
        (*c).time_base = AVRational { num: 1, den: fps };

        if !is_pixel_format_supported(codec, input.pixel_format) {
            return Err(RenderEncodingError::InvalidPixFmt(input.pixel_format));
        }

        (*c).pix_fmt = input.pixel_format;
        // The encoder shares the frame pool (and with it the device) the backend
        // renders into.
        if let Some(frames) = &input.hw_frames_ctx {
            (*c).hw_frames_ctx = frames.new_ref();
        }
        if let Some(device) = &input.hw_device_ctx {
            (*c).hw_device_ctx = device.new_ref();
        }
        // Every RGBA to YUV path (the built-in yuv420 converter, swscale and
        // the GPU converters of the backends) produces limited range with
        // `input.color_matrix`. Say so: an untagged HD stream is decoded as
        // BT.709, shifting the colors of a BT.601 one. Hardware encoders that
        // are fed RGB surfaces convert with the matrix they are told here.
        let pix_fmt_desc = av_pix_fmt_desc_get(input.pixel_format);
        if !pix_fmt_desc.is_null() && (*pix_fmt_desc).flags & AV_PIX_FMT_FLAG_RGB as u64 == 0 {
            (*c).color_range = AVColorRange::AVCOL_RANGE_MPEG;
            match input.color_matrix {
                YuvMatrix::Bt601 => (*c).colorspace = AVColorSpace::AVCOL_SPC_SMPTE170M,
                // The frames are sRGB, whose primaries are BT.709's. Players treat
                // the BT.709 transfer of an SDR stream as sRGB's display.
                YuvMatrix::Bt709 => {
                    (*c).colorspace = AVColorSpace::AVCOL_SPC_BT709;
                    (*c).color_primaries = AVColorPrimaries::AVCOL_PRI_BT709;
                    (*c).color_trc = AVColorTransferCharacteristic::AVCOL_TRC_BT709;
                }
            }
        }
        (*c).gop_size = encoder_options.gop_size;
        (*c).qmin = encoder_options.qmin;
        (*c).qmax = encoder_options.qmax;
        (*c).qcompress = encoder_options.qcompress;
        (*c).max_qdiff = encoder_options.max_qdiff;
        (*c).bit_rate_tolerance = encoder_options.bitrate_tolerance;

        if let Some(video_bitrate) = encoder_options.bitrate {
            (*c).bit_rate = video_bitrate;
        }

        if global_header {
            (*c).flags |= AV_CODEC_FLAG_GLOBAL_HEADER as i32;
        }

        let opts: *mut *mut AVDictionary = &mut std::ptr::null_mut();

        let codec_options = match encoder_options.codec_params {
            Some(options) => Some(options),
            None if !(*codec).name.is_null() => {
                let codec_name = CStr::from_ptr((*codec).name).to_string_lossy();

                match codec_name.as_ref() {
                    "libx264" => Some(
                        [
                            ("preset", "ultrafast"),
                            ("tune", "animation"),
                            ("profile", "main"),
                            ("bframes", "2"),
                            ("crf", "23"),
                        ]
                        .as_slice(),
                    ),
                    "libx265" => Some(
                        [
                            ("preset", "ultrafast"),
                            ("tune", "animation"),
                            ("profile", "main"),
                            ("crf", "23"),
                            ("x265-params", "log-level=none"),
                        ]
                        .as_slice(),
                    ),
                    _ => None,
                }
            }
            _ => None,
        };

        if let Some(codec_params) = codec_options {
            for (param, value) in codec_params {
                let c_param = CString::new(*param).map_err(RenderEncodingError::CStringError)?;
                let c_value = CString::new(*value).map_err(RenderEncodingError::CStringError)?;

                av_dict_set(opts, c_param.as_ptr(), c_value.as_ptr(), 0);
            }
        }

        let status = avcodec_open2(c, codec, opts);
        av_dict_free(opts);
        ffmpeg_loggable_action!(status);

        Ok(())
    }
}

impl Stream {
    /// Can't implement this as a trait cause it needs to be called in specific order
    pub fn free(&mut self) {
        unsafe {
            // in case encoder is not needed (remux) we won't allocate the encoder
            if !self.enc.is_null() {
                avcodec_send_frame(self.enc, std::ptr::null_mut());
                avcodec_free_context(&raw mut self.enc);

                if let StreamVariant::Audio(mut swr_ctx) = self.variant {
                    swr_free(&raw mut swr_ctx);
                }
            }
        }
    }

    pub unsafe fn get_frames_in_stream(&self) -> i64 {
        unsafe { (*self.st).nb_frames }
    }

    pub(crate) unsafe fn prepare_stream_codec(
        preferred_encoder: Option<&str>,
        codec_id: AVCodecID,
        oc: *mut AVFormatContext,
    ) -> RenderEncodingResult<(
        *const AVCodec,
        AVCodecID,
        *mut AVStream,
        *mut AVCodecContext,
    )> {
        unsafe {
            let codec = find_encoder(preferred_encoder, codec_id, true)?;
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
        input: &EncoderInput,
        thread_count: i32,
    ) -> RenderEncodingResult<Self> {
        unsafe {
            let (codec, _codec_id, st, c) = Self::prepare_stream_codec(
                encoder_options.preferred_encoder,
                (*(*oc).oformat).video_codec,
                oc,
            )?;

            (*st).time_base = AVRational { num: 1, den: fps };
            open_video_encoder(
                c,
                codec,
                (width, height, fps),
                (*(*oc).oformat).flags & AVFMT_GLOBALHEADER != 0,
                encoder_options,
                input,
                thread_count,
            )?;
            ffmpeg_loggable_action!(avcodec_parameters_from_context((*st).codecpar, c));

            if let Some((tag, options)) = encoder_options.tag.zip((*st).codecpar.as_mut()) {
                options.codec_tag = tag as u32;
            }

            Ok(Stream {
                st,
                enc: c,
                variant: StreamVariant::Video,
            })
        }
    }

    pub(crate) unsafe fn make_audio(
        oc: *mut AVFormatContext,
        encoder_options: &EncoderOptions,
    ) -> RenderEncodingResult<Self> {
        unsafe {
            let (codec, _codec_id, st, c) = Self::prepare_stream_codec(
                encoder_options.preferred_encoder,
                (*(*oc).oformat).audio_codec,
                oc,
            )?;

            let sample_rate = encoder_options.sample_rate as i32;
            let validated_sample_rate = validate_sample_rate_fits_codec(codec, sample_rate);
            if validated_sample_rate != sample_rate {
                eprintln!(
                    "Warning: sample_rate ({sample_rate}) provided in encoder_options are not available for the codec, using {validated_sample_rate} instead",
                );
            }

            (*c).sample_fmt = fit_sample_format(codec, encoder_options.sample_format);
            (*c).sample_rate = validated_sample_rate;
            (*c).bit_rate = encoder_options.bitrate.unwrap_or(192_000);
            (*st).time_base = AVRational {
                num: 1,
                den: validated_sample_rate,
            };

            (*c).ch_layout = STEREO_CH_LAYOUT;
            if let Some((tag, options)) = encoder_options.tag.zip((*st).codecpar.as_mut()) {
                options.codec_tag = tag as u32;
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

            let swr_ctx = swr_alloc();
            if swr_ctx.is_null() {
                return Err(RenderEncodingError::Internal(
                    "Can not allocate swr".to_owned(),
                ));
            }

            Self::set_swr_option(swr_ctx, "in_sample_rate", (*c).sample_rate);
            Self::set_swr_option(swr_ctx, "out_sample_rate", (*c).sample_rate);

            Self::set_swr_chlayout(swr_ctx, "in_chlayout", &STEREO_CH_LAYOUT);
            Self::set_swr_chlayout(swr_ctx, "out_chlayout", &STEREO_CH_LAYOUT);

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
                    swr_ctx.cast::<std::ffi::c_void>(),
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
                    swr_ctx.cast::<std::ffi::c_void>(),
                    name.as_ptr(),
                    std::ptr::from_ref::<AVChannelLayout>(val),
                    0,
                );
            }
        }
    }

    pub(crate) unsafe fn set_swr_fmt(swr_ctx: *mut SwrContext, name: &str, val: AVSampleFormat) {
        if let Ok(name) = CString::new(name).map_err(RenderEncodingError::CStringError) {
            unsafe {
                av_opt_set_sample_fmt(swr_ctx.cast::<std::ffi::c_void>(), name.as_ptr(), val, 0);
            }
        }
    }
}
