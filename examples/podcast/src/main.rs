use std::path::Path;
pub use fframes_renderer::{fframes_logger, render, RenderOptions};
use podcast_example::PodcastVideo;

fn main() {
    let media_folder = fframes_renderer::MediaDirectory::read_folder(Path::new("./media")).unwrap();
    let media = media_folder.process_media_source().unwrap();

    render(
        &PodcastVideo {
            goose_audio: "final.mp3",
            duck_audio: "final.mp3",
            guest_audio: "final.mp3",
        },
        "out.mp4",
        &RenderOptions {
            media: Some(&media),
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            render_backend: fframes_renderer::cpu::CpuRenderingBackend {
                cache_capacity: 0,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
