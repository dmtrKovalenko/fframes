#![allow(clippy::module_inception)]
#![allow(clippy::missing_safety_doc)]

#[cfg(feature = "cpu_renderer")]
pub mod cpu;
#[doc(hidden)]
pub mod pix_fmt;

pub mod concatenator;

mod encoder_frame;
mod ffmpeg_helper;
mod renderer_font_source;
mod stream;

mod encoder;
pub use encoder::*;

pub mod fframes_logger;
pub use fframes_logger::*;

mod media_directory;
pub use media_directory::*;

mod render_backend;
pub use render_backend::*;

mod renderer;
pub use renderer::*;

mod renderer_error;
pub use renderer_error::*;

mod scheduler;
pub use scheduler::*;

mod segment_writer;
pub use segment_writer::*;
mod frame_guard;
pub use frame_guard::*;

mod preview;
pub use preview::*;

#[cfg(feature = "cpu_renderer")]
pub mod sheet;
pub mod snapshot;

#[cfg(feature = "cli")]
pub mod cli;

pub use rayon;
#[cfg(test)]
mod tests;
