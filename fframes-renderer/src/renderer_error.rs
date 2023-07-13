use colored::Colorize;
use ffmpeg_next::ffi::AVPixelFormat;
use std::{fmt, sync::PoisonError};

pub enum AVError {
    MissingVideoStreamInFile(String),
    CantOpenFile(String),
    CantAllocateCtx,
    CantWriteFrame(String),
    UnknownExtension(String),
    FFmpegError(i32, String),
    InvalidPixFmt(AVPixelFormat),
    Internal(String),
    CannotLocateCodec,
    InvalidArgument(String),
}

impl fmt::Display for AVError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::MissingVideoStreamInFile(file) =>
                    format!("Missing video stream in file {file}"),
                Self::CantOpenFile(file) => format!("Missing video stream in file {}", file.cyan()),
                Self::CantAllocateCtx => "Can not allocate encoding context".to_owned(),
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
            }
        )
    }
}

pub type AVResult<T> = Result<T, AVError>;

pub enum FFramesError {
    FFmpegError(AVError),
    RenderChunkError(usize, AVError),
    MediaError(std::io::Error),
    SubtitlesParsingError(fframes::SubtitlesError),
    MissingRequiredMedia(String),
    CoreError(fframes::error::FFramesCoreError),
    ImageError((String, image::ImageError)),
    ParserError(fframes::usvgr::Error),
    ConcurrencyError,
    CustomError(String),
    InvalidOutput,
}

impl fmt::Debug for FFramesError {
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
                Self::FFmpegError(err) => format!("{err}"),
                Self::MediaError(err) => format!("{}\n{err}", "Can't load or process media".bold()),
                Self::MissingRequiredMedia(required_media) => format!(
                    "Missing required media {}. Verify that you provided correct media_dir.",
                    required_media.magenta().bold()
                ),
                Self::SubtitlesParsingError(err) => format!("{err:?}"),
                Self::CoreError(err) => format!("{err:?}"),
                Self::ImageError((file, err)) =>
                    format!("Can not decode image {file}. Error {err:?}"),
                Self::ConcurrencyError => "Something not correct happened while trying concurrently access one of the resources".to_owned(),
                Self::ParserError(err) => format!("SVG parsing error: {err:?}"),
                Self::CustomError(err) => err.to_owned(),
                Self::InvalidOutput => "Invalid output file. Path does not exist or does not the valid file".to_owned(),
            }
        )
    }
}

pub type FFramesResult<T> = Result<T, FFramesError>;

impl From<AVError> for FFramesError {
    fn from(ffmpeg_error: AVError) -> Self {
        Self::FFmpegError(ffmpeg_error)
    }
}

impl From<std::io::Error> for FFramesError {
    fn from(io_error: std::io::Error) -> Self {
        Self::MediaError(io_error)
    }
}

impl From<fframes::SubtitlesError> for FFramesError {
    fn from(err: fframes::SubtitlesError) -> Self {
        Self::SubtitlesParsingError(err)
    }
}

impl From<fframes::error::FFramesCoreError> for FFramesError {
    fn from(err: fframes::error::FFramesCoreError) -> Self {
        Self::CoreError(err)
    }
}

impl<T> From<PoisonError<T>> for FFramesError {
    fn from(_: PoisonError<T>) -> Self {
        Self::ConcurrencyError
    }
}

impl From<fframes::usvgr::Error> for FFramesError {
    fn from(err: fframes::usvgr::Error) -> Self {
        Self::ParserError(err)
    }
}
