//! Renders the splash screen, a preview image and a description of every talk into `output/`.
use conference_splash_screen::ConferenceMedia;
use fframes::{
    CombinedMediaProvider, CpuFrameRenderer, MediaProvider, Previewer, StaticMediaProvider,
};
use std::{fs, path::PathBuf};

#[path = "../talks.rs"]
mod talks;

/// The frame used as the preview image.
const PREVIEW_FRAME: usize = 145;

fn generate_description(talk: &talks::Talk) -> String {
    format!(
        "{}'s FunOCaml 2024 talk recording!\n\nOverview by {}:\n{}\n{}\n\nConnect with us\nWebsite: https://fun-ocaml.com/\nTwitter: https://x.com/FunOCaml",
        talk.speaker_name,
        talk.speaker_name.split(' ').next().unwrap_or("speaker"),
        talk.description,
        talk.social_links
            .as_deref()
            .map(|links| format!("Find speaker: {links}"))
            .unwrap_or_default()
    )
}

fn main() {
    let media = ConferenceMedia::prepare().unwrap();
    let media_folder = talks::dynamic_media();
    let dynamic_media = media_folder.process_media_source().unwrap();
    let media_provider = CombinedMediaProvider::from([
        &media as &dyn MediaProvider,
        &dynamic_media as &dyn MediaProvider,
    ]);
    let options = talks::render_options(&media_provider);

    let _ = fs::remove_dir_all("output");
    fs::create_dir_all("output").unwrap();

    println!("Rendering {} talks...", talks::TALKS.len());
    for (index, talk) in talks::TALKS.iter().enumerate() {
        let filename = format!("{}_{index}", talk.speaker_name.replace(' ', "_"));
        let video = talks::video(&media, talk);

        fframes::render(
            format!("output/{filename}.mp4"),
            &video,
            talks::backend(),
            &options,
        )
        .expect("Failed to render video");

        Previewer::new(&video, &options)
            .and_then(|mut previewer| {
                previewer.render(PREVIEW_FRAME, &mut CpuFrameRenderer::default())
            })
            .and_then(|frame| frame.save_png(format!("output/{filename}_preview.png")))
            .expect("Failed to save the preview image");

        fs::write(
            PathBuf::from(format!("output/{filename}_description.txt")),
            generate_description(talk),
        )
        .expect("Failed to save description");
    }
}
