//! One file per section of the video. Every scene declares the beats it
//! starts and ends on; `lb` is the beat position inside the scene.

/// A scene struct with its place on the beat grid. `None` means the start or
/// the end of the video.
macro_rules! beat_scene {
    ($name:ident, $start:expr, $end:expr) => {
        #[derive(Debug)]
        pub struct $name;
        impl $name {
            pub const START: Option<f32> = $start;
            pub const END: Option<f32> = $end;
            /// Beat inside the scene (0 on its first downbeat).
            #[allow(dead_code)]
            pub fn lb(frame: &fframes::Frame) -> f32 {
                crate::beat::gbeat(frame) - Self::START.unwrap_or(0.0)
            }
            pub fn frames() -> fframes::Duration<'static> {
                fframes::Duration::Frames(crate::beat::span_frames(Self::START, Self::END))
            }
        }
    };
}

pub mod agents;
pub mod benchmark;
pub mod code;
pub mod done;
pub mod gpu;
pub mod image;
pub mod origin;
pub mod outro;
pub mod prompt;
pub mod recap;
pub mod render;
pub mod scale;
pub mod shader;
pub mod text;
pub mod video;

pub use agents::AgentsScene;
pub use benchmark::BenchmarkScene;
pub use code::CodeScene;
pub use done::DoneScene;
pub use gpu::GpuScene;
pub use image::ImageScene;
pub use origin::OriginScene;
pub use outro::OutroScene;
pub use prompt::PromptScene;
pub use recap::RecapScene;
pub use render::RenderScene;
pub use scale::ScaleScene;
pub use shader::ShaderScene;
pub use text::TextScene;
pub use video::VideoScene;
