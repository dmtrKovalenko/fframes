use clap::Parser;
use fframes::{EncoderOptions, RenderOptions, StaticMediaProvider, fframes_logger};
use motion_graphics_example::{InstallSceneVideo, MotionGraphicsMedia};
use std::path::PathBuf;

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
        &RenderOptions {
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
