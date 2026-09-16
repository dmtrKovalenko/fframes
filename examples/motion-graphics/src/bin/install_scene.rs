use clap::Parser;
use fframes::StaticMediaProvider;
use motion_graphics_example::{InstallSceneVideo, MotionGraphicsMedia, render_options};

#[derive(Debug, Parser)]
#[clap(name = "install_scene", about = "Render the install fff scene")]
struct Args {
    #[clap(short, long, default_value = "install_fff.mp4")]
    output: String,
    #[clap(long, default_value = "libx264")]
    video_codec: Option<String>,
    #[clap(short, long)]
    verbose: bool,
}

fn main() {
    let args = Args::parse();
    let media = MotionGraphicsMedia::prepare().unwrap();

    let backend = fframes::cpu::CpuRenderingBackend {
        concurrency: 1,
        cache_capacity: 10,
        ..Default::default()
    };

    fframes::render(
        args.output.as_str(),
        &InstallSceneVideo::new(&media),
        backend,
        &render_options(&media, args.video_codec.as_deref(), args.verbose),
    )
    .unwrap();
}
