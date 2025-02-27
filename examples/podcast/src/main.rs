use fframes_renderer::EncoderOptions;
pub use fframes_renderer::{RenderOptions, fframes_logger, render};
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
        fframes_renderer::cpu::CpuRenderingBackend::default(),
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
