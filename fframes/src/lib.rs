mod audio_data;
mod audio_map;
mod audio_window_functions;
mod color;
mod duration;
mod fframes_context;
mod font_data;
mod frame;
mod log;
mod media_provider;
mod named_range;
mod scenes;
mod subtitles;
mod svgr;
mod text_wrap;
mod video;

// Methods that we are not pub use ::* should be declared here:
pub mod animation;
pub mod error;

#[cfg(test)]
mod tests;

pub use audio_data::*;
pub use audio_map::*;
pub use audio_window_functions::*;
pub use color::*;
pub use duration::*;
pub use fframes_context::*;
pub use font_data::*;
pub use frame::*;
pub use log::log::*;
pub use media_provider::*;
pub use named_range::*;
pub use scenes::*;
pub use subtitles::*;
pub use svgr::*;
pub use svgr_macro::*;
pub use text_wrap::*;
pub use video::*;

// reexported deps
pub use lazy_static;
pub use lru;
pub use roxmltree;
pub use serde;
pub use ttf_parser;
pub use usvgr;
