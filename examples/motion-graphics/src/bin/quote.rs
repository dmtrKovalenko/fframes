use clap::Parser;
use fframes::{EncoderOptions, RenderOptions, StaticMediaProvider, fframes_logger};
use motion_graphics_example::{MotionGraphicsMedia, QuoteCardVideo};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[clap(name = "quote", about = "Render a quote card video with auto-sized text")]
struct Args {
    /// The text to render, e.g. "PERFORMANCE" or "SPEED\n!=\nFAST" for multi-line
    text: String,
    #[clap(short, long)]
    output: Option<String>,
    #[clap(long, default_value = "libx264")]
    video_codec: Option<String>,
    #[clap(short, long)]
    concurrency: Option<usize>,
}

fn main() {
    let args = Args::parse();
    let media = MotionGraphicsMedia::prepare().unwrap();

    // Support literal \n in shell args as line breaks
    let text = args.text.replace("\\n", "\n");

    let output = args
        .output
        .unwrap_or_else(|| format!("quote_{}.mp4", text.to_lowercase().replace('\n', "_").replace(' ', "_")));

    let backend = fframes::cpu::CpuRenderingBackend {
        concurrency: args.concurrency.unwrap_or(1),
        cache_capacity: 10,
        ..Default::default()
    };

    fframes::render(
        output.as_str(),
        &QuoteCardVideo::new(&media, &text),
        backend,
        &RenderOptions {
            media: Some(&media),
            load_system_fonts: true,
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            tmp_files_directory: Some(&PathBuf::from("test_render")),
            video_encoder_options: EncoderOptions {
                preferred_encoder: args.video_codec.as_deref(),
                codec_params: Some(&[
                    ("crf", "18"),
                    ("preset", "slow"),
                    ("profile", "high"),
                    ("level", "4.2"),
                    ("bframes", "0"),
                    ("colorprim", "bt709"),
                    ("transfer", "bt709"),
                    ("colormatrix", "bt709"),
                ]),
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
