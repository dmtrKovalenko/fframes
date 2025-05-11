use crate::ffmpeg_sys_fframes::{
    AV_CH_LAYOUT_MONO, AVChannelLayout, AVChannelLayout__bindgen_ty_1, AVChannelOrder,
};

pub const MONO_CH_LAYOUT: AVChannelLayout = AVChannelLayout {
    order: AVChannelOrder::AV_CHANNEL_ORDER_NATIVE,
    nb_channels: 1,
    u: AVChannelLayout__bindgen_ty_1 {
        mask: AV_CH_LAYOUT_MONO,
    },
    opaque: std::ptr::null_mut(),
};

#[macro_export]
macro_rules! ffmpeg_action {
    ($x:expr, $err:expr) => {
        let res = $x;

        if (res < 0) {
            return Err($err);
        }
    };
}

#[macro_export]
macro_rules! ffmpeg_loggable_action {
    ($x:expr) => {
        let res = $x;

        if (res < 0) {
            let error_description = $crate::renderer::encoder::av_error_to_string(res);
            return Err(
                $crate::renderer::renderer_error::RenderEncodingError::FFmpegError(
                    res,
                    error_description,
                ),
            );
        }
    };
}
