use std::fmt;

#[derive(Debug)]
pub enum PlayerError {
    /// Resolving the timeline, fonts or rendering a frame failed.
    Renderer(fframes::FFramesRendererError),
    /// The window or the event loop could not be created.
    Window(String),
    /// The window surface could not be created or presented.
    Surface(String),
    /// No Skia backend could be initialized.
    Skia(String),
}

pub type PlayerResult<T> = std::result::Result<T, PlayerError>;

impl fmt::Display for PlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PlayerError::Renderer(err) => write!(f, "renderer error: {err}"),
            PlayerError::Window(err) => write!(f, "window error: {err}"),
            PlayerError::Surface(err) => write!(f, "surface error: {err}"),
            PlayerError::Skia(err) => write!(f, "skia error: {err}"),
        }
    }
}

impl std::error::Error for PlayerError {}

impl From<fframes::FFramesRendererError> for PlayerError {
    fn from(err: fframes::FFramesRendererError) -> Self {
        PlayerError::Renderer(err)
    }
}

impl From<winit::error::EventLoopError> for PlayerError {
    fn from(err: winit::error::EventLoopError) -> Self {
        PlayerError::Window(err.to_string())
    }
}

impl From<winit::error::OsError> for PlayerError {
    fn from(err: winit::error::OsError) -> Self {
        PlayerError::Window(err.to_string())
    }
}

impl From<softbuffer::SoftBufferError> for PlayerError {
    fn from(err: softbuffer::SoftBufferError) -> Self {
        PlayerError::Surface(err.to_string())
    }
}
