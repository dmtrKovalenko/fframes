use conference_splash_screen::{ConferenceMedia, ConferenceVideo, SpeakerScene, SponsorScene};
use fframes::{
    CombinedMediaProvider, MediaProvider, StaticMediaProvider, Video, lazy_static::lazy_static,
};
use fframes_renderer::{EncoderOptions, RenderOptions, fframes_logger, render, render_frame};
use image::{ImageBuffer, Rgba};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize, Deserialize)]
struct Talk {
    #[serde(rename = "abstract")]
    description: String,
    proposal_title: String,
    speaker_name: String,
    social_links: Option<String>,
    avatar: Option<String>,
}

lazy_static! {
    static ref TALKS: Vec<Talk> = serde_json::from_str(include_str!("../sessions.json")).unwrap();
}

fn clean_dir(dir_path: &str) -> io::Result<()> {
    let path = Path::new(dir_path);

    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let file_path = entry.path();

            if file_path.is_file() {
                fs::remove_file(file_path)?;
            }
        }
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Directory not found",
        ))
    }
}

fn generate_description(talk: &Talk) -> String {
    format!(
        "{}'s FunOCaml 2024 talk recording!\n\nOverview by {}:\n{}\n{}\n\nConnect with us\nWebsite: https://fun-ocaml.com/\nTwitter: https://x.com/FunOCaml",
        talk.speaker_name,
        talk.speaker_name.split(' ').next().unwrap_or("speaker"),
        talk.description,
        talk.social_links
            .as_deref()
            .map(|links| format!("Find speaker: {}", links))
            .unwrap_or("".to_string())
    )
}

fn main() {
    let media = ConferenceMedia::prepare().unwrap();
    let media_folder =
        fframes_renderer::MediaDirectory::read_folder(Path::new("./dynamic_media")).unwrap();
    let dynamic_media = media_folder.process_media_source().unwrap();

    clean_dir("output").unwrap();

    let media_provider = CombinedMediaProvider::from([
        &media as &dyn MediaProvider,
        &dynamic_media as &dyn MediaProvider,
    ]);

    let options = RenderOptions {
        media: Some(&media_provider),
        load_system_fonts: true,
        logger: fframes_logger::FFramesLoggerVariant::Compact,
        encoder_options: EncoderOptions {
            preferred_video_codec: Some("libx265"),
            qmin: 0,
            qmax: 69,
            qcompress: 0.6,
            max_qdiff: 4,
            gop_size: 250,
            codec_params: Some(&[
                ("crf", "18"),         // Constant Rate Factor: 18 for high quality
                ("preset", "slow"),    // Slow preset for better compression
                ("profile:v", "high"), // High profile as specified in the example
                ("level:v", "5.1"),    // Level 5.1 as specified
                ("b:v", "6000k"),      // Set video bitrate to 6000 kb/s
                ("maxrate", "6600k"),  // Set max bitrate (10% higher than target)
                ("bufsize", "12000k"), // Set buffer size (2 * bitrate)
                ("g", "120"),          // Set keyframe interval to 120 (2 seconds at 60 fps)
                ("keyint_min", "60"),  // Minimum keyframe interval (1 second at 60 fps)
                ("sc_threshold", "0"), // Disable scene cut detection
            ]),
            ..Default::default()
        },
        ..Default::default()
    };

    let backend = fframes_renderer::cpu::CpuRenderingBackend {
        concurrency: 1,
        cache_capacity: 40,
        ..Default::default()
    };

    TALKS.iter().enumerate().for_each(|(index, talk)| {
        let filename = format!("{}_{index}", talk.speaker_name.replace(" ", "_"));
        let video = ConferenceVideo {
            media: &media,
            sponsor_scene: SponsorScene { media: &media },
            speaker_scene: SpeakerScene {
                avatar: talk.avatar.as_deref(),
                media: &media,
                speaker_name: &talk.speaker_name,
                talk_title: &talk.proposal_title,
                talk_description: &talk.description,
            },
        };

        render(
            format!("output/{}.mp4", filename),
            &video,
            backend,
            &options,
        )
        .expect("Failed to render video");

        // 330 is a frame that we want to use as a preview
        let preview_image = render_frame(330, &video, backend, &options).unwrap();
        let img_buffer = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(
            ConferenceVideo::WIDTH as u32,
            ConferenceVideo::HEIGHT as u32,
            preview_image,
        )
        .expect("Failed to parse the preview image buffer");
        img_buffer
            .save(format!("output/{filename}_preview.png"))
            .expect("Failed to save preview image");

        fs::write(
            PathBuf::from(format!("output/{filename}_description.txt")),
            generate_description(talk),
        )
        .expect("Failed to save description");
    });
}
