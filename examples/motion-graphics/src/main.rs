use clap::Parser;
use fframes::{EncoderOptions, RenderOptions, StaticMediaProvider, fframes_logger};
use motion_graphics_example::{MotionGraphicsMedia, MotionGraphicsVideo, QuoteCardVideo};
use std::path::PathBuf;

#[derive(Debug, Parser)]
struct Args {
    #[clap(short, long, default_value = "out.mp4")]
    output: String,
    #[clap(long, default_value = "libx264")]
    video_codec: Option<String>,
    #[clap(short, long)]
    verbose: bool,
    /// Render a single quote card instead of the intro. Pass the text e.g. --quote "PERFORMANCE"
    #[clap(long)]
    quote: Option<String>,
}

fn main() {
    let args = Args::parse();
    let media = MotionGraphicsMedia::prepare().unwrap();

    let backend = fframes::cpu::CpuRenderingBackend {
        concurrency: 1,
        cache_capacity: 10,
        ..Default::default()
    };

    let options = RenderOptions {
        media: Some(&media),
        load_system_fonts: true,
        logger: if args.verbose {
            fframes_logger::FFramesLoggerVariant::Debug
        } else {
            fframes_logger::FFramesLoggerVariant::Compact
        },
        tmp_files_directory: Some(&PathBuf::from("test_render")),
        video_encoder_options: EncoderOptions {
            preferred_encoder: args.video_codec.as_deref(),
            codec_params: Some(&[("crf", "18"), ("preset", "slow"), ("profile", "high"), ("level", "4.2"), ("bframes", "0"), ("colorprim", "bt709"), ("transfer", "bt709"), ("colormatrix", "bt709")]),
            ..Default::default()
        },
        ..Default::default()
    };

    if let Some(text) = &args.quote {
        fframes::render(
            args.output.as_str(),
            &QuoteCardVideo::new(&media, text),
            backend,
            &options,
        )
        .unwrap();
    } else {
        fframes::render(
            args.output.as_str(),
            &MotionGraphicsVideo { media: &media },
            backend,
            &options,
        )
        .unwrap();
    }
}
