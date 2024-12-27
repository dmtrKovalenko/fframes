use fframes::Video;
use fframes_renderer::EncoderOptions;
pub use fframes_renderer::{fframes_logger, render, RenderOptions};
use fframes_skia_renderer::SkiaFFramesRenderer;
use podcast_example::PodcastVideo;
use std::path::Path;

fn main() {
    let media_folder = fframes_renderer::MediaDirectory::read_folder(Path::new("./media")).unwrap();
    let media = media_folder.process_media_source().unwrap();

    render(
        "out.mp4",
        &PodcastVideo {
            goose_audio: "final.mp3",
            duck_audio: "final.mp3",
            guest_audio: "final.mp3",
        },
        // make sure that this will only work for macos and ios
        // for other platforms create a supported GPU context and surface
        SkiaFFramesRenderer::new_metal(PodcastVideo::WIDTH, PodcastVideo::HEIGHT)
            .expect("Failed to create metal renderer"),
        // or try a cpu editor 
        // fframes_renderer::cpu::CpuRenderingBackend::default(),
        &RenderOptions {
            media: Some(&media),
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            encoder_options: EncoderOptions {
                preferred_video_codec: Some("libx264"),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
