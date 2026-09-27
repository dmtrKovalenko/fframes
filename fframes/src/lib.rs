mod audio_data;
mod audio_map;
mod audio_window_functions;
mod color;
mod duration;
mod fframes_context;
mod font_data;
mod frame;
mod media_provider;
mod named_range;
mod scenes;
mod shader;
mod svgr;
mod text;
mod video;

#[cfg(not(target_arch = "wasm32"))]
mod renderer;
#[cfg(not(target_arch = "wasm32"))]
pub use renderer::*;

// Methods that we are not pub use ::* should be declared here:
pub mod animation;
pub mod error;
pub mod log;

#[cfg(test)]
mod tests;
mod transform;
mod video_data;

pub use audio_data::*;
pub use audio_map::*;
pub use audio_window_functions::*;
pub use color::*;
pub use duration::*;
pub use fframes_context::*;
pub use font_data::*;
pub use frame::*;
pub use media_provider::*;
pub use named_range::*;
pub use scenes::*;
pub use shader::*;
pub use svgr::*;
pub use svgr_macro::*;
pub use text::*;
pub use transform::*;
pub use video::*;
pub use video_data::*;

// reexported deps
pub use crate::usvgr::roxmltree;
pub use fframes_media as media;
pub use fframes_media_dir_macro::*;
pub use lazy_static;
pub use lru;
pub use media::bytemuck;
#[cfg(not(target_arch = "wasm32"))]
pub use media::ffmpeg_sys_fframes;
pub use serde;
pub use ttf_parser;
pub use usvgr;

#[cfg(feature = "exif")]
pub use media::exif;
