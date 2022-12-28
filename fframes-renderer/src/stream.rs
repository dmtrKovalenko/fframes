use crate::ffmpeg_action;
use crate::ffmpeg_loggable_action;
use crate::renderer_error::AVError;
use crate::renderer_error::AVResult;
use ffmpeg_next::sys::*;
use std::ffi::CStr;
use std::ffi::CString;

#[derive(Clone, Copy)]
pub enum StreamVariant {
    Video,
    Audio(*mut SwrContext),
}

#[derive(Clone, Copy)]
pub struct Stream {
    pub(crate) st: *mut AVStream,
    pub(crate) enc: *mut AVCodecContext,
    pub(crate) variant: StreamVariant,
}

impl Stream {
    pub unsafe fn free(mut self) {
        avcodec_free_context(&mut self.enc);
    }

    pub unsafe fn get_frames_in_stream(&self) -> i64 {
        (*self.st).nb_frames
    }

    pub(crate) unsafe fn make(
        preferred_codec_name: &str,
        codec_id: AVCodecID,
        oc: *mut AVFormatContext,
    ) -> Result<
        (
            *const AVCodec,
            AVCodecID,
            *mut AVStream,
            *mut AVCodecContext,
        ),
        Result<Stream, AVError>,
    > {
        let codec_name = CString::new(preferred_codec_name).unwrap();
        let mut codec = avcodec_find_encoder_by_name(codec_name.as_ptr());
        if codec.is_null() {
            codec = avcodec_find_encoder(codec_id);

            let found_codec_name = CStr::from_ptr((*codec).name);
            eprintln!(
                "Warning: Can not find codec {preferred_codec_name}, continue with {codec_name}",
                codec_name = found_codec_name.to_str().unwrap()
            );
        }
        let codec_id = (*codec).id;

        let st = avformat_new_stream(oc, std::ptr::null_mut());
        (*st).id = ((*oc).nb_streams - 1) as i32;
        let c = avcodec_alloc_context3(codec);
        if c.is_null() {
            return Err(Err(AVError::CantAllocateCtx));
        }
        Ok((codec, codec_id, st, c))
    }

    pub(crate) unsafe fn make_video(
        width: i32,
        height: i32,
        fps: i32,
        oc: *mut AVFormatContext,
        preferred_codec_name: &str,
        codec_id: AVCodecID,
    ) -> AVResult<Self> {
        let (codec, codec_id, st, c) = match Self::make(preferred_codec_name, codec_id, oc) {
            Ok(value) => value,
            Err(value) => return value,
        };

        (*c).codec_id = codec_id;
        (*c).width = width;
        (*c).height = height;
        (*st).time_base = AVRational { num: 1, den: fps };
        (*c).time_base = (*st).time_base;

        (*c).gop_size = 12;
        (*c).pix_fmt = AVPixelFormat::AV_PIX_FMT_YUV420P;
        (*c).qmin = 10;
        (*c).qmax = 51;
        (*c).qcompress = 0.6;
        (*c).max_qdiff = 4;
        (*c).bit_rate_tolerance = 0;

        if (*(*oc).oformat).flags & AVFMT_GLOBALHEADER != 0 {
            (*c).flags |= AV_CODEC_FLAG_GLOBAL_HEADER as i32;
        }

        let opts: *mut *mut AVDictionary = &mut std::ptr::null_mut();

        let crf = CString::new("crf").unwrap();
        let crfval = CString::new("23").unwrap();
        av_dict_set(opts, crf.as_ptr(), crfval.as_ptr(), 0);

        ffmpeg_loggable_action!(avcodec_open2(c, codec, opts));
        ffmpeg_loggable_action!(avcodec_parameters_from_context((*st).codecpar, c));

        Ok(Stream {
            st,
            enc: c,
            variant: StreamVariant::Video,
        })
    }

    pub(crate) unsafe fn make_audio(
        sample_rate: i32,
        oc: *mut AVFormatContext,
        preferred_codec_name: &str,
        codec_id: AVCodecID,
    ) -> AVResult<Self> {
        let (codec, _codec_id, st, c) = match Self::make(preferred_codec_name, codec_id, oc) {
            Ok(value) => value,
            Err(value) => return value,
        };

        (*c).sample_fmt = AVSampleFormat::AV_SAMPLE_FMT_FLTP;
        (*c).sample_rate = sample_rate;
        (*c).bit_rate = 320000;
        (*st).time_base = AVRational {
            num: 1,
            den: sample_rate,
        };

        // TODO verify that codec supports 44100 sample_rate
        crate::encoder::make_stereo_layout_channel(c);

        // TODO pass user options
        let opts: *mut *mut AVDictionary = &mut std::ptr::null_mut();

        ffmpeg_loggable_action!(avcodec_open2(c, codec, opts));
        ffmpeg_loggable_action!(avcodec_parameters_from_context((*st).codecpar, c));

        let swr_ctx = swr_alloc();
        if swr_ctx.is_null() {
            return Err(AVError::Internal("Can not allocate swr".to_owned()));
        }

        Self::set_swr_option(swr_ctx, "in_sample_rate", (*c).sample_rate);
        Self::set_swr_option(swr_ctx, "out_sample_rate", (*c).sample_rate);

        Self::set_swr_chlayout(swr_ctx, "in_chlayout", &(*c).ch_layout);
        Self::set_swr_chlayout(swr_ctx, "out_chlayout", &(*c).ch_layout);

        Self::set_swr_fmt(swr_ctx, "in_sample_fmt", AVSampleFormat::AV_SAMPLE_FMT_FLTP);
        Self::set_swr_fmt(swr_ctx, "out_sample_fmt", (*c).sample_fmt);

        ffmpeg_action!(
            swr_init(swr_ctx),
            AVError::Internal("Can not init swr".to_owned())
        );

        Ok(Stream {
            st,
            enc: c,
            variant: StreamVariant::Audio(swr_ctx),
        })
    }

    pub(crate) unsafe fn set_swr_option(swr_ctx: *mut SwrContext, name: &str, val: i32) {
        let name = CString::new(name).unwrap();
        av_opt_set_int(
            swr_ctx as *mut std::ffi::c_void,
            name.as_ptr(),
            val.into(),
            0,
        );
    }

    pub(crate) unsafe fn set_swr_chlayout(
        swr_ctx: *mut SwrContext,
        name: &str,
        val: &AVChannelLayout,
    ) {
        let name = CString::new(name).unwrap();
        av_opt_set_chlayout(
            swr_ctx as *mut std::ffi::c_void,
            name.as_ptr(),
            val as *const AVChannelLayout,
            0,
        );
    }

    pub(crate) unsafe fn set_swr_fmt(swr_ctx: *mut SwrContext, name: &str, val: AVSampleFormat) {
        let name = CString::new(name).unwrap();
        av_opt_set_sample_fmt(swr_ctx as *mut std::ffi::c_void, name.as_ptr(), val, 0);
    }
}
