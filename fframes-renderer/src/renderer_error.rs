use colored::Colorize;
use ffmpeg_next::ffi::AVPixelFormat;
use std::{error::Error, fmt, sync::PoisonError};

/// Thread or Chunk level error which can happen during parallelized rendering
pub enum RenderEncodingError {
    MissingVideoStreamInFile(String),
    CantOpenFile(String),
    CantAllocate(String),
    CantWriteFrame(String),
    UnknownExtension(String),
    FFmpegError(i32, String),
    InvalidPixFmt(AVPixelFormat),
    Internal(String),
    CannotLocateCodec,
    InvalidArgument(String),
    CoreError(fframes::error::FFramesError),
    RenderError,
    CStringError(std::ffi::NulError),
}

impl fmt::Display for RenderEncodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::MissingVideoStreamInFile(file) =>
                    format!("Missing video stream in file {file}"),
                Self::CantOpenFile(file) => format!("Missing video stream in file {}", file.cyan()),
                Self::CantAllocate(what) => format!("Can not allocate {what}"),
                Self::FFmpegError(code, description) =>
                    format!("libav error {code}: {description}"),
                Self::CantWriteFrame(file) =>
                    format!("Can not write frame to file {}", file.cyan()),
                Self::UnknownExtension(file) => format!(
                    "Can not deduce file format of output file {} from extension.",
                    file.cyan().bold()
                ),
                Self::Internal(message) => message.to_owned(),
                Self::CannotLocateCodec => "Couldn't locate audio or video codec neither from render_options nor from the output file extension. Make sure that extension is a valid video file and you have installed appropriate codecs for this specific container. E.g. in order to output the .webm extension you should have vp9 and opus codecs installed".to_owned(),
                Self::InvalidArgument(argument) => format!("Argument {argument} that was provided is not valid or not supported for the current codec."),
                Self::InvalidPixFmt(pix_fmt) => format!("Pixel format `{pix_fmt:?}` is not supported for current codec"),
                Self::CoreError(err) => format!("{err:?}"),
                Self::CStringError(err) => format!("Failed to convert string to c string: {err:?}"),
                Self::RenderError => "Rendering pipeline failed.".to_owned()
            }
        )
    }
}

pub type RenderEncodingResult<T> = Result<T, RenderEncodingError>;

pub enum FFramesRendererError {
    RenderChunkError(usize, RenderEncodingError),
    ConcatChunkError(RenderEncodingError),
    MediaError(std::io::Error),
    SubtitlesParsingError(fframes::SubtitlesError),
    MissingRequiredMedia(String),
    ImageError((String, image::ImageError)),
    ConcurrencyError,
    Internal(String),
    InvalidOutput,
    CoreError(fframes::error::FFramesError),

    /// Any custom rendering backend implementation-specific error
    Custom(String),
}

impl Error for FFramesRendererError {}

impl fmt::Display for FFramesRendererError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl fmt::Debug for FFramesRendererError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "\n{header}\n{error}",
            header = "Failure".red().bold(),
            error = match self {
                Self::RenderChunkError(chunk, error) => format!(
                    "Rendering chunk {chunk} failed.\nReason: {error}",
                    chunk = chunk.to_string().cyan().bold()
                ),
                Self::ConcatChunkError(error)=>  format!(
                    "Concatenation of rendered video chunks failed failed.\nReason: {error}",
                ),
                Self::MediaError(err) => format!("{}\n{err}", "Can't load or process media".bold()),
                Self::MissingRequiredMedia(required_media) => format!(
                    "Missing required media {}. Verify that you provided correct media_dir.",
                    required_media.magenta().bold()
                ),
                Self::SubtitlesParsingError(err) => format!("Failed to process subtitle file: {err:?}"),
                Self::CoreError(err) => format!("{err:?}"),
                Self::ImageError((file, err)) =>
                    format!("Can not decode image {file}. Error {err:?}"),
                Self::ConcurrencyError => "Something not correct happened while trying concurrently access one of the resources".to_owned(),
                Self::Internal(err) | Self::Custom(err) => err.to_owned(),
                Self::InvalidOutput => "Invalid output file. Path does not exist or does not the valid file".to_owned(),
            }
        )
    }
}

pub type FFramesRendererResult<T> = Result<T, FFramesRendererError>;

impl From<std::io::Error> for FFramesRendererError {
    fn from(io_error: std::io::Error) -> Self {
        Self::MediaError(io_error)
    }
}

impl From<fframes::SubtitlesError> for FFramesRendererError {
    fn from(err: fframes::SubtitlesError) -> Self {
        Self::SubtitlesParsingError(err)
    }
}

impl<T> From<PoisonError<T>> for FFramesRendererError {
    fn from(_: PoisonError<T>) -> Self {
        Self::ConcurrencyError
    }
}

// Duplicate implementation here because it is completely valid scenario to have core error during rendering/encoding phase and the preparation phase as well.
impl From<fframes::error::FFramesError> for RenderEncodingError {
    fn from(err: fframes::error::FFramesError) -> Self {
        Self::CoreError(err)
    }
}

impl From<fframes::error::FFramesError> for FFramesRendererError {
    fn from(err: fframes::error::FFramesError) -> Self {
        Self::CoreError(err)
    }
}
