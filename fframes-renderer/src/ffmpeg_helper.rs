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
            let error_description = $crate::encoder::av_error_to_string(res);
            return Err($crate::renderer_error::RenderEncodingError::FFmpegError(
                res,
                CString::from_raw(error_description)
                    .to_str()
                    .unwrap_or("Unknown libav error.")
                    .to_owned(),
            ));
        }
    };
}
