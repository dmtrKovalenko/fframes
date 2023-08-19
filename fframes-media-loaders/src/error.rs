#[derive(Debug)]
pub enum FFramesMediaError {
    #[cfg(not(target_arch = "wasm32"))]
    Mp3Error(minimp3::Error),
    #[cfg(not(target_arch = "wasm32"))]
    ImageError(image::ImageError),
    FontError(ttf_parser::FaceParsingError),
    VttError(webvtt_parser::VttError),
}

#[cfg(not(target_arch = "wasm32"))]
impl From<minimp3::Error> for FFramesMediaError {
    fn from(err: minimp3::Error) -> Self {
        Self::Mp3Error(err)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl From<image::ImageError> for FFramesMediaError {
    fn from(err: image::ImageError) -> Self {
        Self::ImageError(err)
    }
}

impl From<ttf_parser::FaceParsingError> for FFramesMediaError {
    fn from(err: ttf_parser::FaceParsingError) -> Self {
        Self::FontError(err)
    }
}

impl From<webvtt_parser::VttError> for FFramesMediaError {
    fn from(err: webvtt_parser::VttError) -> Self {
        Self::VttError(err)
    }
}

pub type Result<T> = std::result::Result<T, FFramesMediaError>;
