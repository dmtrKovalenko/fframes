#![cfg(feature = "vulkan")]

use fframes::ffmpeg_sys_fframes::AVPixelFormat::{self, *};
use fframes::media::FFmpegDecoder;
use fframes::{
    AudioMap, Color, Duration, EncoderFrameRenderer, EncoderInput, EncoderOptions, FFramesContext,
    Frame, RenderOptions, Svgr, Video, VideoFrame, usvgr,
};
use fframes_skia_renderer::vulkan::SkiaVulkanCtx;
use fframes_skia_renderer::{
    FrameExportPath, SkiaBackend, SkiaEncoderFrameRenderer, SkiaFFramesRenderer, SkiaFrameExport,
    SkiaPipelineConfig,
};

const WIDTH: u32 = 322;
const HEIGHT: u32 = 242;

/// `None` on machines without a Vulkan device (the tests are skipped there).
fn vulkan() -> Option<SkiaVulkanCtx> {
    match SkiaVulkanCtx::new(WIDTH as usize, HEIGHT as usize) {
        Ok(ctx) => Some(ctx),
        Err(err) => {
            eprintln!("skipping: no Vulkan device ({err:?})");
            None
        }
    }
}

/// Flat blocks that start on even pixels (so every chroma sample covers one color) and a
/// gradient that exercises the rounding.
fn test_tree() -> usvgr::Tree {
    let svgr: Svgr = fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width={WIDTH} height={HEIGHT}>
            <defs>
                <linearGradient id="fade" x1="0" y1="0" x2="1" y2="0">
                    <stop offset="0" stop-color="#102030" />
                    <stop offset="1" stop-color="#f0c020" />
                </linearGradient>
            </defs>
            <rect x="0" y="0" width="160" height="120" fill="#ff0000" />
            <rect x="160" y="0" width="162" height="120" fill="#00ff00" />
            <rect x="0" y="120" width="160" height="60" fill="#0000ff" />
            <rect x="160" y="120" width="162" height="60" fill="#ffffff" />
            <rect x="0" y="180" width="322" height="62" fill="url(#fade)" />
        </svg>
    );

    svgr.into_svg_tree(
        &usvgr::Options::default(),
        &mut usvgr::Cache::default(),
        &usvgr::fontdb::Database::new(),
    )
    .expect("valid svgr")
}

fn render(
    backend: &impl SkiaBackend,
    mode: SkiaFrameExport,
    format: AVPixelFormat,
    background: Color,
) -> (VideoFrame, FrameExportPath) {
    let mut renderer = SkiaEncoderFrameRenderer::new(
        backend,
        mode,
        &EncoderInput::software(format),
        WIDTH,
        HEIGHT,
    )
    .expect("encoder frame renderer");
    let path = renderer.path();
    let frame = renderer
        .render_tree(&test_tree(), background)
        .expect("rendered frame");

    assert_eq!(frame.pixel_format(), format);
    assert_eq!(
        (frame.width(), frame.height()),
        (WIDTH as i32, HEIGHT as i32)
    );
    (frame, path)
}

/// `(bytes per row, rows)` of every plane of `format`.
fn plane_sizes(format: AVPixelFormat) -> Vec<(usize, usize)> {
    let (w, h) = (WIDTH as usize, HEIGHT as usize);
    let (half_w, half_h) = (w.div_ceil(2), h.div_ceil(2));
    match format {
        AV_PIX_FMT_YUV420P => vec![(w, h), (half_w, half_h), (half_w, half_h)],
        AV_PIX_FMT_YUVA420P => vec![(w, h), (half_w, half_h), (half_w, half_h), (w, h)],
        AV_PIX_FMT_NV12 | AV_PIX_FMT_NV21 => vec![(w, h), (half_w * 2, half_h)],
        AV_PIX_FMT_YUV422P => vec![(w, h), (half_w, h), (half_w, h)],
        AV_PIX_FMT_YUV444P => vec![(w, h), (w, h), (w, h)],
        other => panic!("no plane sizes for {other:?}"),
    }
}

/// The share of bytes that differ by more than `tolerance` and the largest difference.
fn difference(actual: &[u8], expected: &[u8], tolerance: u8) -> (f64, u8) {
    assert_eq!(actual.len(), expected.len());
    let mut off = 0_usize;
    let mut largest = 0;
    for (a, e) in actual.iter().zip(expected) {
        let diff = a.abs_diff(*e);
        largest = largest.max(diff);
        off += usize::from(diff > tolerance);
    }
    (off as f64 / actual.len() as f64, largest)
}

#[test]
fn gpu_converted_planes_match_the_cpu_conversion() {
    let Some(vulkan) = vulkan() else { return };

    for format in [
        AV_PIX_FMT_YUV420P,
        AV_PIX_FMT_NV12,
        AV_PIX_FMT_NV21,
        AV_PIX_FMT_YUVA420P,
        AV_PIX_FMT_YUV422P,
        AV_PIX_FMT_YUV444P,
    ] {
        let (gpu, gpu_path) = render(&vulkan, SkiaFrameExport::Auto, format, Color::BLACK);
        assert_eq!(gpu_path, FrameExportPath::GpuConversion, "{format:?}");

        let (cpu, cpu_path) = render(
            &vulkan,
            SkiaFrameExport::CpuConversion,
            format,
            Color::BLACK,
        );
        assert_eq!(cpu_path, FrameExportPath::CpuConversion, "{format:?}");

        for (plane, (width, height)) in plane_sizes(format).into_iter().enumerate() {
            let actual = gpu.copy_plane(plane, width, height);
            let expected = cpu.copy_plane(plane, width, height);

            // The converters round differently and subsample chroma differently (the GPU
            // averages the pixels of a sample, swscale filters them), which shows at the
            // block edges and in the gradient only.
            let (off, largest) = difference(&actual, &expected, 2);
            assert!(
                off < 0.04,
                "{format:?} plane {plane}: {:.2}% of the bytes differ, the largest by {largest}",
                off * 100.
            );
        }
    }
}

#[test]
fn yuv420_levels_are_exact_for_flat_colors() {
    let Some(vulkan) = vulkan() else { return };
    let (frame, _) = render(
        &vulkan,
        SkiaFrameExport::Auto,
        AV_PIX_FMT_YUV420P,
        Color::BLACK,
    );

    let (w, h) = (WIDTH as usize, HEIGHT as usize);
    let luma = frame.copy_plane(0, w, h);
    let cb = frame.copy_plane(1, w / 2, h / 2);
    let cr = frame.copy_plane(2, w / 2, h / 2);
    let sample = |x: usize, y: usize| {
        (
            luma[y * w + x],
            cb[(y / 2) * (w / 2) + x / 2],
            cr[(y / 2) * (w / 2) + x / 2],
        )
    };
    let assert_yuv = |actual: (u8, u8, u8), expected: (u8, u8, u8)| {
        let close = |a: u8, e: u8| a.abs_diff(e) <= 1;
        assert!(
            close(actual.0, expected.0)
                && close(actual.1, expected.1)
                && close(actual.2, expected.2),
            "expected ~{expected:?}, got {actual:?}"
        );
    };

    // BT.601 limited range
    assert_yuv(sample(80, 60), (82, 90, 240)); // red
    assert_yuv(sample(240, 60), (145, 54, 34)); // green
    assert_yuv(sample(80, 150), (41, 240, 110)); // blue
    assert_yuv(sample(240, 150), (235, 128, 128)); // white
}

#[test]
fn neighbouring_pixels_do_not_bleed_into_each_other() {
    let Some(vulkan) = vulkan() else { return };

    // White columns one pixel wide: the four luma bytes that share a texel of the packed
    // surface alternate between the darkest and the brightest value.
    let columns: Vec<Svgr> = (0..WIDTH / 2)
        .map(|column| {
            fframes::svgr!(<rect x={column * 2} y="0" width="1" height={HEIGHT} fill="#ffffff" />)
        })
        .collect();
    let tree: usvgr::Tree = fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width={WIDTH} height={HEIGHT} shape-rendering="crispEdges">
            {columns}
        </svg>
    )
    .into_svg_tree(
        &usvgr::Options::default(),
        &mut usvgr::Cache::default(),
        &usvgr::fontdb::Database::new(),
    )
    .unwrap();

    for format in [AV_PIX_FMT_YUV420P, AV_PIX_FMT_NV12] {
        let mut renderer = SkiaEncoderFrameRenderer::new(
            &vulkan,
            SkiaFrameExport::Auto,
            &EncoderInput::software(format),
            WIDTH,
            HEIGHT,
        )
        .unwrap();
        assert_eq!(renderer.path(), FrameExportPath::GpuConversion);
        let frame = renderer.render_tree(&tree, Color::BLACK).unwrap();

        let luma = frame.copy_plane(0, WIDTH as usize, HEIGHT as usize);
        for (x, y) in luma[100 * WIDTH as usize..101 * WIDTH as usize]
            .iter()
            .enumerate()
        {
            let expected = if x % 2 == 0 { 235 } else { 16 };
            assert!(y.abs_diff(expected) <= 1, "{format:?}: luma {y} at x={x}");
        }

        // every chroma sample averages a white and a black column: no color
        let chroma_bytes = plane_sizes(format)[1];
        let chroma = frame.copy_plane(1, chroma_bytes.0, chroma_bytes.1);
        assert!(chroma.iter().all(|c| c.abs_diff(128) <= 1), "{format:?}");
    }
}

#[test]
fn alpha_plane_follows_the_coverage() {
    let Some(vulkan) = vulkan() else { return };
    let transparent = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    let tree: usvgr::Tree = fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width={WIDTH} height={HEIGHT}>
            <rect x="0" y="0" width="160" height="242" fill="#ffffff" />
        </svg>
    )
    .into_svg_tree(
        &usvgr::Options::default(),
        &mut usvgr::Cache::default(),
        &usvgr::fontdb::Database::new(),
    )
    .unwrap();

    let mut renderer = SkiaEncoderFrameRenderer::new(
        &vulkan,
        SkiaFrameExport::Auto,
        &EncoderInput::software(AV_PIX_FMT_YUVA420P),
        WIDTH,
        HEIGHT,
    )
    .unwrap();
    let frame = renderer.render_tree(&tree, transparent).unwrap();

    let alpha = frame.copy_plane(3, WIDTH as usize, HEIGHT as usize);
    assert_eq!(alpha[100 * WIDTH as usize + 80], 255);
    assert_eq!(alpha[100 * WIDTH as usize + 240], 0);
}

#[test]
fn formats_without_gpu_conversion_fall_back_to_the_cpu() {
    let Some(vulkan) = vulkan() else { return };
    let (frame, path) = render(
        &vulkan,
        SkiaFrameExport::Auto,
        AV_PIX_FMT_YUV420P10LE,
        Color::BLACK,
    );

    assert_eq!(path, FrameExportPath::CpuConversion);
    assert_eq!(frame.pixel_format(), AV_PIX_FMT_YUV420P10LE);
}

const FPS: usize = 10;
const FRAMES: usize = 30;
const SWITCH_AT: usize = FRAMES / 2;

#[derive(Debug)]
struct TwoColors;

impl Video for TwoColors {
    const FPS: usize = FPS;
    const WIDTH: usize = WIDTH as usize;
    const HEIGHT: usize = HEIGHT as usize;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(FRAMES)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let fill = if frame.index < SWITCH_AT {
            "#ff0000"
        } else {
            "#0000ff"
        };

        fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width={Self::WIDTH} height={Self::HEIGHT}>
                <rect x="0" y="0" width={Self::WIDTH} height={Self::HEIGHT} fill={fill} />
            </svg>
        )
    }
}

fn center_pixel(decoder: &mut FFmpegDecoder, frame: usize) -> [u8; 4] {
    unsafe {
        assert!(
            decoder.decode_up_to(frame as i64).unwrap(),
            "frame {frame} is missing from the rendered video"
        );

        let image = decoder
            .get_raw_frame()
            .convert_last_decoded_frame_into_svg_image(None)
            .unwrap();
        let offset =
            ((image.height as usize / 2) * image.width as usize + image.width as usize / 2) * 4;
        image.data[offset..offset + 4].try_into().unwrap()
    }
}

fn assert_color_close(actual: [u8; 4], expected: [u8; 3], frame: usize) {
    // lossy yuv420p encoding shifts solid colors by a few units
    let close = actual[..3]
        .iter()
        .zip(expected)
        .all(|(a, e)| a.abs_diff(e) <= 24);
    assert!(
        close,
        "frame {frame}: expected ~{expected:?}, decoded {actual:?}"
    );
}

#[test]
fn renders_videos_through_every_export_mode() {
    let Some(vulkan) = vulkan() else { return };

    for (mode, pixel_format) in [
        (SkiaFrameExport::Auto, AV_PIX_FMT_YUV420P),
        (SkiaFrameExport::GpuConversion, AV_PIX_FMT_YUV420P),
        (SkiaFrameExport::CpuConversion, AV_PIX_FMT_YUV420P),
    ] {
        let dir = std::env::temp_dir().join(format!(
            "fframes-skia-export-{}-{mode:?}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let output = dir.join("out.mp4");

        fframes::render(
            &output,
            &TwoColors,
            SkiaFFramesRenderer::new_vulkan(&vulkan, SkiaPipelineConfig::default())
                .unwrap()
                .frame_export(mode),
            &RenderOptions {
                logger: fframes::fframes_logger::FFramesLoggerVariant::Silent,
                tmp_files_directory: Some(&dir.join("chunks")),
                video_encoder_options: EncoderOptions {
                    // decodes in software whatever FFmpeg was built with
                    preferred_encoder: Some("mpeg4"),
                    pixel_format,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap_or_else(|err| panic!("{mode:?}: {err:?}"));

        let mut decoder = unsafe { FFmpegDecoder::new(&output, FPS, 1) }.unwrap();
        assert_color_close(center_pixel(&mut decoder, 0), [255, 0, 0], 0);
        assert_color_close(
            center_pixel(&mut decoder, SWITCH_AT + 2),
            [0, 0, 255],
            SWITCH_AT + 2,
        );
        drop(decoder);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[test]
fn h264_renders_decode_with_or_without_a_hardware_decoder() {
    let Some(vulkan) = vulkan() else { return };
    let encoder_options = EncoderOptions {
        preferred_encoder: Some("libx264"),
        ..Default::default()
    };
    let dir = std::env::temp_dir().join(format!("fframes-skia-export-{}-h264", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let output = dir.join("out.mp4");

    let encoder = fframes::VideoEncoderInfo::for_output(
        &output,
        (WIDTH as i32, HEIGHT as i32, FPS as i32),
        &encoder_options,
    )
    .unwrap();
    if encoder.name() != "libx264" {
        eprintln!("skipping: FFmpeg was built without libx264 (the `h264` feature)");
        return;
    }

    fframes::render(
        &output,
        &TwoColors,
        SkiaFFramesRenderer::new_vulkan(&vulkan, SkiaPipelineConfig::default()).unwrap(),
        &RenderOptions {
            logger: fframes::fframes_logger::FFramesLoggerVariant::Silent,
            tmp_files_directory: Some(&dir.join("chunks")),
            video_encoder_options: encoder_options,
            ..Default::default()
        },
    )
    .unwrap();

    // With FFmpeg's Vulkan support compiled in the decoder asks the GPU first and has to
    // fall back to software on drivers without H.264 decode.
    let mut decoder = unsafe { FFmpegDecoder::new(&output, FPS, 1) }.unwrap();
    assert_color_close(center_pixel(&mut decoder, 0), [255, 0, 0], 0);
    assert_color_close(
        center_pixel(&mut decoder, SWITCH_AT + 2),
        [0, 0, 255],
        SWITCH_AT + 2,
    );
    drop(decoder);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_encoder_that_does_not_open_fails_the_render() {
    let Some(vulkan) = vulkan() else { return };
    let dir = std::env::temp_dir().join(format!("fframes-skia-export-{}-bad", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    // The encoder is opened by the encoding threads. When they give up, the stages
    // feeding them have to stop too instead of waiting for room in the queues.
    let result = fframes::render(
        dir.join("out.mp4"),
        &TwoColors,
        SkiaFFramesRenderer::new_vulkan(&vulkan, SkiaPipelineConfig::default()).unwrap(),
        &RenderOptions {
            logger: fframes::fframes_logger::FFramesLoggerVariant::Silent,
            tmp_files_directory: Some(&dir.join("chunks")),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some("mpeg4"),
                // a maximum bitrate needs a buffer size, which is not set
                codec_params: Some(&[("maxrate", "1000")]),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        result.is_err(),
        "the encoder was expected to reject its options"
    );
}

#[cfg(feature = "vulkan-video")]
mod vulkan_video {
    use super::*;
    use fframes::{EncoderInput, FFramesRenderBackend};

    fn shared_vulkan() -> Option<SkiaVulkanCtx> {
        match SkiaVulkanCtx::new_shared_with_encoder(WIDTH as usize, HEIGHT as usize) {
            Ok(ctx) => Some(ctx),
            Err(err) => {
                eprintln!("skipping: FFmpeg has no Vulkan device ({err:?})");
                None
            }
        }
    }

    fn nv12_frames(vulkan: &SkiaVulkanCtx) -> EncoderInput {
        EncoderInput::hardware_frames(
            vulkan.encoder_device().expect("a shared device"),
            AV_PIX_FMT_VULKAN,
            AV_PIX_FMT_NV12,
            (WIDTH as i32, HEIGHT as i32),
            |_| {},
        )
        .unwrap_or_else(|err| panic!("NV12 Vulkan frames: {err}"))
    }

    #[test]
    fn hardware_frames_are_offered_for_8_bit_420_only() {
        let Some(vulkan) = shared_vulkan() else {
            return;
        };
        let offer = |pixel_format| {
            let options = EncoderOptions {
                preferred_encoder: Some("h264_vulkan"),
                pixel_format,
                ..Default::default()
            };
            let encoder = fframes::VideoEncoderInfo::for_output(
                std::path::Path::new("out.mp4"),
                (WIDTH as i32, HEIGHT as i32, 30),
                &options,
            )
            .unwrap();
            assert_eq!(encoder.name(), "h264_vulkan");
            vulkan.negotiate_hardware_frames(&encoder)
        };

        let input = offer(AV_PIX_FMT_YUV420P).expect("NV12 frames for the default request");
        assert_eq!(input.pixel_format, AV_PIX_FMT_VULKAN);
        assert_eq!(input.software_format(), AV_PIX_FMT_NV12);

        // 10 bit or 4:4:4 would be encoded as 8 bit 4:2:0
        assert!(offer(AV_PIX_FMT_YUV420P10LE).is_none());
        assert!(offer(AV_PIX_FMT_YUV444P).is_none());

        // a device that is not shared with FFmpeg has no frames for the encoder
        let Some(own) = super::vulkan() else { return };
        let options = EncoderOptions {
            preferred_encoder: Some("h264_vulkan"),
            ..Default::default()
        };
        let encoder = fframes::VideoEncoderInfo::for_output(
            std::path::Path::new("out.mp4"),
            (WIDTH as i32, HEIGHT as i32, 30),
            &options,
        )
        .unwrap();
        assert!(own.negotiate_hardware_frames(&encoder).is_none());
    }

    #[derive(Debug)]
    struct TwoColors720;

    impl Video for TwoColors720 {
        const FPS: usize = 30;
        const WIDTH: usize = 1280;
        const HEIGHT: usize = 720;
        const BACKGROUND_COLOR: Color = Color::BLACK;

        fn duration(&self) -> Duration<'_> {
            Duration::Frames(60)
        }

        fn audio(&self) -> AudioMap<'_> {
            AudioMap::none()
        }

        fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
            let fill = if frame.index < 30 {
                "#ff0000"
            } else {
                "#0000ff"
            };

            fframes::svgr!(
                <svg xmlns="http://www.w3.org/2000/svg" width={Self::WIDTH} height={Self::HEIGHT}>
                    <rect x="0" y="0" width={Self::WIDTH} height={Self::HEIGHT} fill={fill} />
                </svg>
            )
        }
    }

    /// Renders through a Vulkan Video encoder and checks the decoded colors. Skipped where
    /// the driver has no such encoder.
    fn renders_through(encoder: &str) {
        let Ok(vulkan) =
            SkiaVulkanCtx::new_shared_with_encoder(TwoColors720::WIDTH, TwoColors720::HEIGHT)
        else {
            eprintln!("skipping: FFmpeg has no Vulkan device");
            return;
        };

        let dir = std::env::temp_dir().join(format!(
            "fframes-skia-export-{}-{encoder}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let output = dir.join("out.mp4");
        let options = RenderOptions {
            logger: fframes::fframes_logger::FFramesLoggerVariant::Silent,
            tmp_files_directory: Some(&dir.join("chunks")),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some(encoder),
                // Mesa's Intel driver (25.1, `ANV_DEBUG=video-encode`) writes HEVC that
                // decodes washed out below the quantizer 26, whatever feeds the encoder.
                codec_params: Some(&[("qp", "26")]),
                ..Default::default()
            },
            ..Default::default()
        };

        let info = fframes::VideoEncoderInfo::for_output(
            &output,
            (
                TwoColors720::WIDTH as i32,
                TwoColors720::HEIGHT as i32,
                TwoColors720::FPS as i32,
            ),
            &options.video_encoder_options,
        )
        .unwrap();
        let backend =
            SkiaFFramesRenderer::new_vulkan(&vulkan, SkiaPipelineConfig::default()).unwrap();
        let hardware = info.name() == encoder
            && backend
                .negotiate_encoder_input(&info)
                .is_ok_and(|input| input.is_hardware());
        if !hardware {
            eprintln!("skipping {encoder}: the driver has no such encoder");
            return;
        }

        fframes::render(&output, &TwoColors720, backend, &options)
            .unwrap_or_else(|err| panic!("{encoder}: {err:?}"));

        let mut decoder = unsafe { FFmpegDecoder::new(&output, TwoColors720::FPS, 1) }.unwrap();
        assert_color_close(center_pixel(&mut decoder, 0), [255, 0, 0], 0);
        assert_color_close(center_pixel(&mut decoder, 32), [0, 0, 255], 32);
        drop(decoder);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hevc_vulkan_reads_the_frames_skia_rendered() {
        renders_through("hevc_vulkan");
    }

    #[test]
    fn h264_vulkan_reads_the_frames_skia_rendered() {
        renders_through("h264_vulkan");
    }

    #[test]
    fn skia_renders_on_the_device_of_ffmpeg() {
        let Some(vulkan) = shared_vulkan() else {
            return;
        };
        let (shared, path) = render(
            &vulkan,
            SkiaFrameExport::Auto,
            AV_PIX_FMT_YUV420P,
            Color::BLACK,
        );
        assert_eq!(path, FrameExportPath::GpuConversion);

        let Some(own) = super::vulkan() else { return };
        let (reference, _) = render(
            &own,
            SkiaFrameExport::Auto,
            AV_PIX_FMT_YUV420P,
            Color::BLACK,
        );
        for (plane, (width, height)) in plane_sizes(AV_PIX_FMT_YUV420P).into_iter().enumerate() {
            assert_eq!(
                shared.copy_plane(plane, width, height),
                reference.copy_plane(plane, width, height),
                "plane {plane}"
            );
        }
    }

    #[test]
    fn hardware_frames_hold_the_converted_frame() {
        let Some(vulkan) = shared_vulkan() else {
            return;
        };
        let input = nv12_frames(&vulkan);
        assert!(input.is_hardware());
        assert_eq!(input.software_format(), AV_PIX_FMT_NV12);

        let mut renderer =
            SkiaEncoderFrameRenderer::new(&vulkan, SkiaFrameExport::Auto, &input, WIDTH, HEIGHT)
                .expect("hardware frame renderer");
        assert_eq!(renderer.path(), FrameExportPath::HardwareFrames);

        let (reference, _) = render(
            &vulkan,
            SkiaFrameExport::GpuConversion,
            AV_PIX_FMT_NV12,
            Color::BLACK,
        );
        let tree = test_tree();

        // more frames than copies can be in flight, so command buffers and pool images
        // are reused
        for index in 0..12 {
            let frame = renderer.render_tree(&tree, Color::BLACK).expect("frame");
            assert_eq!(frame.pixel_format(), AV_PIX_FMT_VULKAN);

            let downloaded = frame
                .download()
                .unwrap_or_else(|err| panic!("download of frame {index}: {err}"));
            assert_eq!(downloaded.pixel_format(), AV_PIX_FMT_NV12);

            for (plane, (width, height)) in plane_sizes(AV_PIX_FMT_NV12).into_iter().enumerate() {
                let (off, largest) = difference(
                    &downloaded.copy_plane(plane, width, height),
                    &reference.copy_plane(plane, width, height),
                    1,
                );
                assert!(
                    off == 0.,
                    "frame {index} plane {plane}: {:.2}% of the bytes differ, the largest by {largest}",
                    off * 100.
                );
            }
        }
    }

    #[test]
    fn hardware_frames_follow_the_rendered_frame() {
        let Some(vulkan) = shared_vulkan() else {
            return;
        };
        let input = nv12_frames(&vulkan);
        let mut renderer =
            SkiaEncoderFrameRenderer::new(&vulkan, SkiaFrameExport::Auto, &input, WIDTH, HEIGHT)
                .unwrap();

        let tree = |fill: &'static str| -> usvgr::Tree {
            fframes::svgr!(
                <svg xmlns="http://www.w3.org/2000/svg" width={WIDTH} height={HEIGHT}>
                    <rect x="0" y="0" width={WIDTH} height={HEIGHT} fill={fill} />
                </svg>
            )
            .into_svg_tree(
                &usvgr::Options::default(),
                &mut usvgr::Cache::default(),
                &usvgr::fontdb::Database::new(),
            )
            .unwrap()
        };

        // Frames stay valid while later ones are rendered: nothing is shared between them.
        let frames: Vec<_> = ["#ff0000", "#0000ff", "#ffffff"]
            .into_iter()
            .map(|fill| renderer.render_tree(&tree(fill), Color::BLACK).unwrap())
            .collect();

        for (frame, (y, cb, cr)) in
            frames
                .iter()
                .zip([(82, 90, 240), (41, 240, 110), (235, 128, 128)])
        {
            let downloaded = frame
                .download()
                .unwrap_or_else(|err| panic!("download: {err}"));
            let luma = downloaded.copy_plane(0, WIDTH as usize, HEIGHT as usize);
            let chroma = downloaded.copy_plane(1, WIDTH as usize, HEIGHT as usize / 2);
            assert!(luma.iter().all(|v| v.abs_diff(y) <= 1), "luma of {y}");
            assert!(
                chroma
                    .chunks(2)
                    .all(|uv| uv[0].abs_diff(cb) <= 1 && uv[1].abs_diff(cr) <= 1),
                "chroma of {cb},{cr}"
            );
        }
    }
}
