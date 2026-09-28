use fframes::StaticMediaProvider;
use fframes::cli::{self, clap};
use motion_graphics_example::{MotionGraphicsMedia, QuoteCardVideo, render_options};
use std::process::ExitCode;

/// A quote card video with auto-sized text.
#[derive(Debug, clap::Args)]
struct Args {
    /// The text, e.g. "PERFORMANCE" or "SPEED\n!=\nFAST" for several lines.
    #[arg(long, default_value = "PERFORMANCE", global = true)]
    text: String,
    #[arg(long, default_value = "libx264", global = true)]
    video_codec: String,
    #[arg(short, long, global = true)]
    concurrency: Option<usize>,
}

fn main() -> ExitCode {
    let args = cli::parse::<Args>();
    let media = MotionGraphicsMedia::prepare().unwrap();

    // A literal \n in the shell argument is a line break.
    let text = args.app.text.replace("\\n", "\n");
    let video_codec = args.app.video_codec.clone();
    let concurrency = args.app.concurrency.unwrap_or(1);
    let output = format!(
        "quote_{}.mp4",
        text.to_lowercase().replace(['\n', ' '], "_")
    );

    cli::new(
        &QuoteCardVideo::new(&media, &text),
        render_options(&media, Some(&video_codec), false),
    )
    .args(args)
    .default_output(output)
    .backend(fframes::cpu::CpuRenderingBackend {
        concurrency,
        cache_capacity: 10,
        ..Default::default()
    })
    .run()
}
