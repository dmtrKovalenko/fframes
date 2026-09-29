//! fframes renders videos from Rust code. A video is a type that implements [`Video`]. It declares
//! the size, frame rate and duration once and returns an SVG tree ([`Svgr`]) for every frame from
//! the [`svgr!`] macro. The renderer rasterises the frames on all cores, mixes the audio and
//! encodes the result with `FFmpeg`.
//!
//! # Pipeline
//!
//! 1. [`Video::duration`], [`Video::define_scenes`] and [`Video::audio`] are resolved into a frame
//!    timeline once per render. Scenes ([`Scene`], [`Scenes`], [`Overlap`]) are placed back to back,
//!    audio tracks ([`AudioMap`], [`AudioTrack`]) are placed at their sample position.
//! 2. For every frame the renderer builds a [`Frame`] (index, time, scene offset) and a
//!    [`FFramesContext`] (media lookups, scene info, video size) and calls
//!    [`Video::render_frame`]. Frames are rendered concurrently, so the method must be pure: read
//!    precomputed data from `self`, no I/O, no panics.
//! 3. The returned [`Svgr`] becomes a `usvgr::Tree`. With the `compile-time-svgtree` feature the
//!    macro emits the tree at compile time and every subtree without `{}` interpolation carries a
//!    static hash, which lets the backends cache its rasterisation across frames. Without the
//!    feature the markup is a string parsed per frame.
//! 4. A rendering backend ([`FFramesRenderBackend`]) rasterises the tree. The built-in
//!    [`cpu::CpuRenderingBackend`] renders one video segment per thread with tiny-skia. The Skia
//!    backend in the `fframes_skia_renderer` crate walks the tree on the GPU and also executes
//!    [`Shader`] layers.
//! 5. Segments are encoded through `FFmpeg` ([`EncoderOptions`]), concatenated, and muxed with the
//!    audio mix ([`AudioMixOptions`]: summing, ducking, fades, master limiter).
//!
//! # Minimal video
//!
//! ```rust
//! use fframes::{AudioMap, Duration, FFramesContext, Frame, RenderOptions, Svgr, Video, animation::Easing};
//!
//! // Every file in the folder becomes a field; fonts are registered by family name.
//! fframes::include_media_dir!(pub struct Media, "media");
//!
//! struct Hello<'a> {
//!     media: &'a Media,
//!     title: &'a str,
//! }
//!
//! impl Video for Hello<'_> {
//!     const FPS: usize = 30;
//!     const WIDTH: usize = 1920;
//!     const HEIGHT: usize = 1080;
//!
//!     fn duration(&self) -> Duration<'_> {
//!         Duration::Seconds(3.0)
//!     }
//!
//!     fn audio(&self) -> AudioMap<'_> {
//!         AudioMap::none()
//!     }
//!
//!     fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
//!         let opacity = frame.animate(&fframes::timeline!(
//!             at 0.0 => 0.5, animate 0.0_f32 => 1.0, Easing::EaseOut
//!         ));
//!         fframes::svgr!(
//!             <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1920 1080"
//!                  width={Self::WIDTH} height={Self::HEIGHT}>
//!                 <text x="120" y="560" font-family="DM Sans" font-size="150" fill="#fff"
//!                       opacity={opacity}>
//!                     "Hello " {self.title}
//!                 </text>
//!             </svg>
//!         )
//!     }
//! }
//!
//! fn main() -> std::process::ExitCode {
//!     let media = Media::prepare().expect("embedded media");
//!     let video = Hello { media: &media, title: "world" };
//!     // `render`, `frame`, `strip`, `inspect`, `snapshot`, `audio` and more (feature `cli`).
//!     fframes::cli::new(&video, RenderOptions { media: Some(&media), ..Default::default() }).run()
//! }
//! ```
//!
//! Without the `cli` feature call [`render`] with the output path, a backend and [`RenderOptions`],
//! or render single frames through [`Previewer`].
//!
//! # Feature flags
//!
//! - `cpu_renderer` (default): the tiny-skia based [`cpu::CpuRenderingBackend`] and the encoding
//!   pipeline. Disable it for a `wasm32` build.
//! - `cli`: the [`cli`] module and its `clap` dependency.
//! - `compile-time-svgtree`: [`svgr!`] builds the SVG tree at compile time and hashes static
//!   subtrees. Required by the Skia backend and by [`Shader`].
//! - `exif`: EXIF orientation of loaded images.
//! - Codecs `h264`, `h265`, `aac`, `mp3lame`, `opus`, `vpx`: compile the library into the static
//!   `FFmpeg` build. Some of them need `libav-agree-gpl`, `libav-agree-nonfree` or
//!   `libav-agree-version3`, which state that you accept the corresponding `FFmpeg` license terms.
//! - Hardware acceleration `videotoolbox`, `audiotoolbox`, `vaapi`, `nvidia`, `qsv`, `vulkan`,
//!   `mediacodec`: enable the platform encoders and decoders in `FFmpeg`.
//!
//! # `FFmpeg`
//!
//! Decoding, encoding and muxing use the `FFmpeg` libraries through `ffmpeg-sys-fframes`, re-exported
//! as [`ffmpeg_sys_fframes`]. On Linux and macOS `FFmpeg` is linked statically: a prebuilt build
//! for the target and codec features is downloaded during `cargo build`, and compiled from source
//! when there is none, which needs the toolchain listed in the repository README (nasm, clang).
//! The codec features link the system codec libraries (x264, x265, ...), so their dev packages
//! must be installed either way. On Windows a prebuilt `FFmpeg` 9 is
//! linked through `FFMPEG_DIR` or vcpkg and the codec features are not available.
//!
mod audio_analysis;
mod audio_data;
mod audio_map;
mod audio_mix;
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
mod time_spec;
mod video;

#[cfg(not(target_arch = "wasm32"))]
mod renderer;
#[cfg(not(target_arch = "wasm32"))]
pub use renderer::*;

// Methods that we are not pub use ::* should be declared here:
pub mod animation;
pub mod diagnostics;
pub mod error;
pub mod log;

#[cfg(test)]
mod tests;
mod transform;
mod video_data;

pub use audio_analysis::*;
pub use audio_data::*;
pub use audio_map::*;
pub use audio_mix::*;
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
pub use time_spec::*;
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
