use clap::Parser;
use fframes::StaticMediaProvider;
use motion_graphics_example::{MotionGraphicsMedia, QuoteCardVideo, render_options};

#[derive(Debug, Parser)]
#[clap(
    name = "quote",
    about = "Render a quote card video with auto-sized text"
)]
struct Args {
    /// The text to render, e.g. "PERFORMANCE" or "SPEED\n!=\nFAST" for multi-line
    text: String,
    #[clap(short, long)]
    output: Option<String>,
    #[clap(long, default_value = "libx264")]
    video_codec: Option<String>,
    #[clap(short, long)]
    concurrency: Option<usize>,
    #[clap(short, long)]
    verbose: bool,
}

fn main() {
    let args = Args::parse();
    let media = MotionGraphicsMedia::prepare().unwrap();

    // Support literal \n in shell args as line breaks
    let text = args.text.replace("\\n", "\n");

    let output = args.output.unwrap_or_else(|| {
        format!(
            "quote_{}.mp4",
            text.to_lowercase().replace(['\n', ' '], "_")
        )
    });

    let backend = fframes::cpu::CpuRenderingBackend {
        concurrency: args.concurrency.unwrap_or(1),
        cache_capacity: 10,
        ..Default::default()
    };

    fframes::render(
        output.as_str(),
        &QuoteCardVideo::new(&media, &text),
        backend,
        &render_options(&media, args.video_codec.as_deref(), args.verbose),
    )
    .unwrap();
}
