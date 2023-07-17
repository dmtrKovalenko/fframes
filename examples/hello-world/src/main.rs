use fframes_renderer::{
    fframes_logger, render, render_backend, AVPixelFormat, EncoderOptions, RenderOptions,
};
use hello_world_example::HelloWorldVideo;
use std::path::PathBuf;

fn main() {
    render(
        HelloWorldVideo {
            slug: "Hello Renderer!",
        },
        "out.webm",
        RenderOptions {
            media_dir: "./media",
            logger: fframes_logger::FFramesLoggerVariant::Compact,
            encoder_options: EncoderOptions {
                tmp_files_directory: Some(&PathBuf::from("test_render")),
                // codec_params: Some(&[("crf", "18"), ("tune", "animation")]),
                pixel_format: AVPixelFormat::AV_PIX_FMT_YUV422P10LE,
                ..Default::default()
            },
            render_backend: render_backend::CpuRenderingBackend {
                cache_capacity: 5,
                // concurrency: 1,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
}
