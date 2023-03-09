pub mod animation;
pub mod audio_data;
pub mod audio_map;
pub mod audio_window_functions;
pub mod color;
pub mod error;
pub mod fframes_context;
mod font_data;
pub mod frame;
mod log;
pub mod media_provider;
mod scenes;
pub mod subtitles;
mod svgr;
mod text_wrap;
pub mod video;

#[cfg(test)]
mod tests;

pub use animation::*;
pub use audio_data::*;
pub use audio_map::*;
pub use audio_window_functions::*;
pub use color::*;
pub use fframes_context::*;
pub use font_data::*;
pub use frame::*;
pub use log::log::*;
pub use roxmltree;
pub use scenes::*;
pub use subtitles::*;
pub use svgr::*;
pub use svgr_macro::*;
pub use text_wrap::*;
pub use video::*;

// reexported deps
pub use lazy_static;
pub use lru;
pub use serde;
pub use ttf_parser;
pub use usvgr;
