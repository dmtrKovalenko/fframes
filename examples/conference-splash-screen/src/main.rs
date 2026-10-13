//! One talk's splash screen through `fframes::cli`. `render_all` renders every talk.
use conference_splash_screen::ConferenceMedia;
use fframes::cli::{self, clap};
use fframes::{CombinedMediaProvider, MediaProvider, StaticMediaProvider};
use fframes_skia_renderer::cli::SkiaRenderers;
use std::process::ExitCode;

mod talks;

#[derive(Debug, clap::Args)]
struct Args {
    /// The talk: an index into sessions.json or part of the speaker's name.
    #[arg(long, default_value = "0", global = true)]
    talk: String,
}

/// A talk by index (`3`) or by a case-insensitive part of the speaker's name.
fn find_talk(query: &str) -> Result<&'static talks::Talk, String> {
    if let Ok(index) = query.parse::<usize>() {
        return talks::TALKS.get(index).ok_or_else(|| {
            format!(
                "there are {} talks, index {index} is out of range",
                talks::TALKS.len()
            )
        });
    }
    let query = query.to_lowercase();
    talks::TALKS
        .iter()
        .find(|talk| talk.speaker_name.to_lowercase().contains(&query))
        .ok_or_else(|| format!("no talk by a speaker matching \"{query}\""))
}

fn main() -> ExitCode {
    let args = cli::parse::<Args>();
    let talk = match find_talk(&args.app.talk) {
        Ok(talk) => talk,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };

    let media = ConferenceMedia::prepare().unwrap();
    let media_folder = talks::dynamic_media();
    let dynamic_media = media_folder.process_media_source().unwrap();
    let media_provider = CombinedMediaProvider::from([
        &media as &dyn MediaProvider,
        &dynamic_media as &dyn MediaProvider,
    ]);

    cli::new(
        &talks::video(&media, talk),
        talks::render_options(&media_provider),
    )
    .args(args)
    .backend(talks::backend())
    .renderers(SkiaRenderers::default())
    .run()
}
