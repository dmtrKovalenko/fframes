use fframes::StaticMediaProvider;
use fframes::cli::{self, clap};
use motion_graphics_example::{MotionGraphicsMedia, MotionGraphicsVideo, render_options};
use std::process::ExitCode;

#[derive(Debug, clap::Args)]
struct Args {
    #[arg(long, default_value = "libx264", global = true)]
    video_codec: String,
}

fn main() -> ExitCode {
    let args = cli::parse::<Args>();
    let media = MotionGraphicsMedia::prepare().unwrap();
    let video_codec = args.app.video_codec.clone();

    cli::new(
        &MotionGraphicsVideo { media: &media },
        render_options(&media, Some(&video_codec), false),
    )
    .args(args)
    .backend(fframes::cpu::CpuRenderingBackend {
        concurrency: 1,
        cache_capacity: 10,
        ..Default::default()
    })
    .run()
}
