pub enum FFramesMediaError {
    #[cfg(not(target_arch = "wasm32"))]
    ImageError(image::ImageError),
    LibAVAllocationError(&'static str),
    LibAVAudioDecodingError((i32, String)),
    LibAVVideoDecodingError((i32, String)),
    AudioDecodingError(String),
    FontError(ttf_parser::FaceParsingError),
    VttError(webvtt_parser::VttError),
    NulError(std::ffi::NulError),
    MediaDirectoryProvided,
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

impl From<std::ffi::NulError> for FFramesMediaError {
    fn from(err: std::ffi::NulError) -> Self {
        Self::NulError(err)
    }
}

pub type Result<T> = std::result::Result<T, FFramesMediaError>;

impl std::fmt::Debug for FFramesMediaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LibAVAudioDecodingError((code, msg)) => {
                write!(f, "LibAVAudioDecodingError: {} - {}", code, msg,)
            }
            Self::LibAVVideoDecodingError((code, msg)) => {
                write!(f, "LibAVVideoDecodingError: {} - {}", code, msg,)
            }
            Self::AudioDecodingError(msg) => write!(f, "AudioDecodingError: {}", msg),
            #[cfg(not(target_arch = "wasm32"))]
            Self::ImageError(err) => write!(f, "ImageError: {:?}", err),
            Self::FontError(err) => write!(f, "FontError: {:?}", err),
            Self::VttError(err) => write!(f, "VttError: {:?}", err),
            Self::NulError(err) => write!(f, "{:?}", err),
            Self::LibAVAllocationError(what) => write!(f, "libav: Failed to allocate {}", what),
            Self::MediaDirectoryProvided => write!(f, "Can not open directory as a media file. Use a separate media source for directories."),
        }
    }
}
