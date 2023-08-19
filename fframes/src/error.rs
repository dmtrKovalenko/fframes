use std::{error::Error, fmt};

#[derive(Debug)]
pub enum FFramesError {
    /// Your custom error will go here if you return the Err(YourError) from your Video's render_frame.
    UserError(String),

    // All the other core fframes errors will go here:
    CanNotProcessAudioDuration(String),
    /// Means that you didn't implement neither `duration` nor `define_scenes`, nor `audio` methods.
    /// One of them is required to calculate the output duration of the video.
    ///
    /// If you see this error likely you need to implement `duration` method and define the duration..
    MissingDurationOrScenes,
    /// Something is overflowing bounds. Contain info about what is overflowed and how much.
    /// e.g. The duration of scene is overflowed by 1000 frames.
    Overflow(String, usize),
    /// Svg parser error
    ParserError(usvgr::Error),
    MediaError(crate::media::FFramesMediaError),
}

impl Error for FFramesError {}

impl fmt::Display for FFramesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                FFramesError::CanNotProcessAudioDuration(file) => format!("Can not get the duration based on the AudioTimestamp::Eof. The file {file} is not a valid audio file."),
                FFramesError::MissingDurationOrScenes => "The Video trait implementation does not have  neither `duration` nor `define_scenes`, nor `audio` method implemented. One of them is required to to calculate the output duration of the video.".to_owned(),
                FFramesError::Overflow(what, overflow_by) => format!("The {what} is overflowed by {overflow_by}"),
                FFramesError::UserError(err) => format!("Custom error:\n{}", err),
                FFramesError::ParserError(err) => format!("SVG parsing error: {err:?}"),
                FFramesError::MediaError(err) => format!("Media parsing error: {err:?}"),
            }
        )
    }
}

impl From<crate::media::FFramesMediaError> for FFramesError {
    fn from(err: crate::media::FFramesMediaError) -> Self {
        FFramesError::MediaError(err)
    }
}

pub type Result<T> = std::result::Result<T, FFramesError>;
