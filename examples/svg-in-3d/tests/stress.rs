//! Many frames with nested svgr images and 3D render at the same time on the Skia Vulkan
//! backend: no deadlock, and the same video on every run.
#![cfg(not(target_os = "macos"))]
#![allow(clippy::unreadable_literal)]

use std::hash::{Hash, Hasher};
use std::path::Path;

use fframes::media::{FFmpegDecoder, ImageData};
use fframes::{
    AudioMap, Color, Duration, FFramesContext, Frame, RenderOptions, StaticMediaProvider, Svgr,
    Video, svgr,
};
use fframes_3d::*;
use fframes_skia_renderer::vulkan::SkiaVulkanCtx;
use fframes_skia_renderer::{
    SkiaFFramesRenderer, SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig,
};
use svg_in_3d_example::SvgIn3dMedia;

const SIZE: usize = 320;
const FPS: usize = 30;
const FRAMES: usize = 200;

#[derive(Debug)]
struct Busy;

/// `image` on a quad turned by `angle` radians, rendered at `size` x `size`.
fn quad(image: &ImageData<'_>, size: u32, angle: f32) -> ImageData<'static> {
    let mut scene = Scene::new().environment(Environment::studio());
    scene.add(
        Node::mesh(
            Geometry::plane(2.0, 2.0),
            Material::standard(0xffffff).map(Texture::from_image(image)),
        )
        .rotate_y(angle),
    );
    let camera = Camera::perspective(40.0).at(0.0, 0.0, 3.0);
    Renderer::new(size, size).gpu().render(&scene, &camera)
}

impl Video for Busy {
    const FPS: usize = FPS;
    const WIDTH: usize = SIZE;
    const HEIGHT: usize = SIZE;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(FRAMES)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let n = frame.index as f32;
        // a static image (rendered once) and a new one in every frame
        let Some(logo) = ctx.render_svgr(
            svgr!(
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128" width="128" height="128">
                    <rect x="0" y="0" width="128" height="128" fill="#2f81f7" />
                    <text x="64" y="88" text-anchor="middle" font-family="DM Sans" font-size="64" fill="#ffffff">"ff"</text>
                </svg>
            ),
            128,
            128,
        ) else {
            return Svgr::empty();
        };
        let Some(inner) = ctx.render_svgr(
            svgr!(
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">
                    <rect x="0" y="0" width="256" height="256" fill="#ffffff" />
                    <rect x={(n * 3.0) % 192.0} y="40" width="64" height="64" fill="#ff0000" />
                    <image href={logo.href()} x="16" y="120" width="96" height="96" />
                    <text x="128" y="200" font-family="JetBrains Mono" font-size="40" fill="#000000">{frame.index.to_string()}</text>
                </svg>
            ),
            256,
            256,
        ) else {
            return Svgr::empty();
        };
        let a = quad(&inner, 256, (n * 0.05).sin() * 0.7);
        let Some(b) = ctx.render_svgr(
            svgr!(
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 320" width="320" height="320">
                    <rect x="0" y="0" width="320" height="320" fill="#202830" />
                    <image href={a.href()} x="32" y="32" width="256" height="256" />
                    <circle cx={20.0 + (n * 2.0) % 280.0} cy="300" r="12" fill="#00ff00" />
                </svg>
            ),
            320,
            320,
        ) else {
            return Svgr::empty();
        };
        let c = quad(&b, 320, (n * 0.02).cos() * 0.5);
        svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 320" width="320" height="320">
                <image href={c.href()} x="0" y="0" width="320" height="320" />
            </svg>
        )
    }
}

/// Renders the video and returns the hash of every decoded frame.
fn render(gpu: &SkiaVulkanCtx, output: &Path) -> u64 {
    let media = SvgIn3dMedia::prepare().expect("media");
    let chunks = output.with_extension("chunks");
    let options = RenderOptions {
        media: Some(&media),
        logger: fframes::fframes_logger::FFramesLoggerVariant::Silent,
        tmp_files_directory: Some(&chunks),
        ..Default::default()
    };
    let backend = SkiaFFramesRenderer::new_vulkan(
        gpu,
        SkiaPipelineConfig {
            concurrency_policy: SkiaPipelineConcurrencyPolicy::MaxPerformance,
            ..Default::default()
        },
    )
    .unwrap();
    fframes::render(output, &Busy, backend, &options).expect("the video renders");

    let mut hasher = std::hash::DefaultHasher::new();
    let mut decoder = unsafe { FFmpegDecoder::new(output, FPS, 1) }.unwrap();
    for frame in 0..FRAMES {
        unsafe {
            assert!(
                decoder.decode_up_to(frame as i64).unwrap(),
                "frame {frame} is missing"
            );
            let image = decoder
                .get_raw_frame()
                .convert_last_decoded_frame_into_svg_image(None)
                .unwrap();
            image.data.hash(&mut hasher);
        }
    }
    hasher.finish()
}

#[test]
fn many_parallel_frames_with_nested_images_are_deterministic() {
    let Ok(gpu) = SkiaVulkanCtx::new(SIZE, SIZE) else {
        eprintln!("no Vulkan device, skipped");
        return;
    };
    assert!(gpu.shared_gpu().is_some(), "wgpu shares the Vulkan device");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("svg-in-3d-stress");
    std::fs::create_dir_all(&dir).unwrap();

    let first = render(&gpu, &dir.join("first.mp4"));
    let second = render(&gpu, &dir.join("second.mp4"));
    eprintln!("frame hash: {first:016x}");
    assert_eq!(first, second, "two renders of the same video differ");
    if let Ok(keep) = std::env::var("SVG_IN_3D_TEST_OUTPUT") {
        std::fs::copy(dir.join("first.mp4"), Path::new(&keep).join("stress.mp4")).unwrap();
    }
    let _ = std::fs::remove_dir_all(&dir);
}
