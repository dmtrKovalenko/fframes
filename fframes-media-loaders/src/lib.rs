mod audio;
mod error;
mod font;
mod images;
mod raw_file;
pub mod subtitles;
mod video_types;

pub use audio::*;
pub use bytemuck;
pub use error::*;
pub use font::*;
pub use images::*;
pub use raw_file::*;
pub use subtitles::*;
pub use video_types::*;

#[cfg(target_arch = "wasm32")]
thread_local! {
    pub static IS_PREVIEW_RENDERING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
}

#[cfg(not(target_arch = "wasm32"))]
mod audio_decoder;

#[cfg(not(target_arch = "wasm32"))]
mod video_decoder;
#[cfg(not(target_arch = "wasm32"))]
pub use video_decoder::*;

#[cfg(feature = "exif")]
pub use exif;
