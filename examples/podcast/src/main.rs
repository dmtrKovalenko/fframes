use fframes::{EncoderOptions, MediaDirectory, RenderOptions, cli};
use podcast_example::PodcastVideo;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let media_folder = MediaDirectory::read_folder(Path::new("./media")).unwrap();
    let media = media_folder.process_media_source().unwrap();

    cli::new(
        &PodcastVideo {
            goose_audio: "final.mp3",
            duck_audio: "final.mp3",
            guest_audio: "final.mp3",
        },
        RenderOptions {
            media: Some(&media),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some("libx264"),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .run()
}
