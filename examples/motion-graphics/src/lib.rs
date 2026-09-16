pub mod install_scene;
pub mod motion_graphics;
pub mod quote_card;
pub use install_scene::*;
pub use motion_graphics::*;
pub use quote_card::*;

use fframes::animation::Easing;

/// The spring every scene in this example uses for its "punch in" motion.
pub const SPRING_SNAPPY: Easing = Easing::Spring {
    mass: 1.0,
    stiffness: 300.0,
    damping: 26.0,
};

#[cfg(not(target_arch = "wasm32"))]
mod render {
    use std::path::PathBuf;
    use std::sync::LazyLock;

    use fframes::{EncoderOptions, RenderOptions, fframes_logger::FFramesLoggerVariant};

    use crate::MotionGraphicsMedia;

    static TMP_FILES_DIRECTORY: LazyLock<PathBuf> = LazyLock::new(|| PathBuf::from("test_render"));

    const VIDEO_CODEC_PARAMS: &[(&str, &str)] = &[
        ("crf", "18"),
        ("preset", "slow"),
        ("profile", "high"),
        ("level", "4.2"),
        ("bframes", "0"),
        ("colorprim", "bt709"),
        ("transfer", "bt709"),
        ("colormatrix", "bt709"),
    ];

    /// Render options shared by every binary of this example.
    pub fn render_options<'a>(
        media: &'a MotionGraphicsMedia,
        video_codec: Option<&'a str>,
        verbose: bool,
    ) -> RenderOptions<'a, 'a> {
        RenderOptions {
            media: Some(media),
            load_system_fonts: true,
            logger: if verbose {
                FFramesLoggerVariant::Debug
            } else {
                FFramesLoggerVariant::Compact
            },
            tmp_files_directory: Some(&TMP_FILES_DIRECTORY),
            video_encoder_options: EncoderOptions {
                preferred_encoder: video_codec,
                codec_params: Some(VIDEO_CODEC_PARAMS),
                ..Default::default()
            },
            ..Default::default()
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use render::render_options;
