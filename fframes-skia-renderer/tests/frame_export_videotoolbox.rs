#![cfg(all(target_os = "macos", feature = "metal"))]

use fframes::ffmpeg_sys_fframes::AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX;
use fframes::ffmpeg_sys_fframes::{
    av_color_primaries_name, av_color_range_name, av_color_space_name, av_color_transfer_name,
    avformat_close_input, avformat_find_stream_info, avformat_open_input,
};
use fframes::media::FFmpegDecoder;
use fframes::{
    AudioMap, Color, Duration, EncoderOptions, FFramesContext, FFramesRenderBackend, Frame,
    RenderOptions, Svgr, Video, VideoEncoderInfo, YuvMatrix,
};
use fframes_skia_renderer::metal::SkiaMetalCtx;
use fframes_skia_renderer::{
    SkiaFFramesRenderer, SkiaFrameExport, SkiaPipelineConcurrencyPolicy, SkiaPipelineConfig,
};
use std::ffi::{CStr, CString, c_char};
use std::path::{Path, PathBuf};

const ENCODERS: [&str; 2] = ["h264_videotoolbox", "hevc_videotoolbox"];
const FPS: usize = 60;
// Three 40-frame segments end at fractional seconds and expose truncated MP4 edit lists.
const FRAMES: usize = 120;
const SWITCH_AT: usize = FRAMES / 2;

#[derive(Debug)]
struct TwoColors;

impl Video for TwoColors {
    const FPS: usize = FPS;
    const WIDTH: usize = 1280;
    const HEIGHT: usize = 720;
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

/// Mid saturation, so nothing clips. Converted with BT.601 and decoded as the BT.709 the
/// stream is tagged with, it is off by 15 in green.
const FLAT: [u8; 3] = [60, 179, 113];
const FLAT_FRAMES: usize = 12;

#[derive(Debug)]
struct Flat;

impl Video for Flat {
    const FPS: usize = FPS;
    const WIDTH: usize = 1280;
    const HEIGHT: usize = 720;
    const BACKGROUND_COLOR: Color = Color::rgb(FLAT[0], FLAT[1], FLAT[2]);

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(FLAT_FRAMES)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    // The background fills the frame.
    fn render_frame<'a>(&'a self, _frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width={Self::WIDTH} height={Self::HEIGHT}></svg>
        )
    }
}

/// Renders `video` with `encoder` into a new directory, handing it the frames the way
/// `mode` says. `None` when `FFmpeg` was built without the encoder.
fn render<V: Video + Sync + Send>(
    metal: &SkiaMetalCtx,
    video: &V,
    encoder: &str,
    (mode, contexts): (SkiaFrameExport, usize),
    color_matrix: YuvMatrix,
) -> Option<PathBuf> {
    let dir = std::env::temp_dir().join(format!(
        "fframes-skia-export-{}-{encoder}-{mode:?}-{contexts}-{color_matrix:?}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let output = dir.join("out.mp4");
    let options = RenderOptions {
        logger: fframes::fframes_logger::FFramesLoggerVariant::Silent,
        tmp_files_directory: Some(&dir.join("chunks")),
        video_encoder_options: EncoderOptions {
            preferred_encoder: Some(encoder),
            bitrate: Some(4_000_000),
            codec_params: Some(&[("allow_sw", "0")]),
            color_matrix,
            ..Default::default()
        },
        ..Default::default()
    };

    let info = VideoEncoderInfo::for_output(
        &output,
        (V::WIDTH as i32, V::HEIGHT as i32, V::FPS as i32),
        &options.video_encoder_options,
    )
    .unwrap();
    if info.name() != encoder {
        eprintln!("skipping {encoder}: FFmpeg was built without the `videotoolbox` feature");
        return None;
    }

    let backend = SkiaFFramesRenderer::new_metal(
        metal,
        SkiaPipelineConfig {
            encoder_threads: 6,
            concurrency_policy: SkiaPipelineConcurrencyPolicy::Concurrency(contexts),
            ..Default::default()
        },
    )
    .unwrap()
    .frame_export(mode);
    let input = backend.negotiate_encoder_input(&info).unwrap();
    if mode == SkiaFrameExport::Auto {
        assert_eq!(
            input.pixel_format, AV_PIX_FMT_VIDEOTOOLBOX,
            "{encoder} did not get hardware frames"
        );
    } else {
        assert!(!input.is_hardware());
    }

    fframes::render(&output, video, backend, &options)
        .unwrap_or_else(|err| panic!("{encoder}: {err:?}"));
    Some(output)
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
    // Tight enough to notice the encoder converting RGB with another matrix than the one
    // the stream is tagged with (BT.709 instead of BT.601 moves pure red by 22).
    let close = actual[..3]
        .iter()
        .zip(expected)
        .all(|(a, e)| a.abs_diff(e) <= 12);
    assert!(
        close,
        "frame {frame}: expected ~{expected:?}, decoded {actual:?}"
    );
}

/// Matrix, primaries, transfer and range of the video stream of `path`.
fn color_tags(path: &Path) -> [String; 4] {
    let name = |name: *const c_char| unsafe { CStr::from_ptr(name) }.to_string_lossy().into();
    unsafe {
        let filename = CString::new(path.to_str().unwrap()).unwrap();
        let mut input = std::ptr::null_mut();
        let status = avformat_open_input(
            &raw mut input,
            filename.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        assert_eq!(status, 0);
        assert!(avformat_find_stream_info(input, std::ptr::null_mut()) >= 0);
        let par = (**(*input).streams).codecpar;
        let tags = [
            name(av_color_space_name((*par).color_space)),
            name(av_color_primaries_name((*par).color_primaries)),
            name(av_color_transfer_name((*par).color_trc)),
            name(av_color_range_name((*par).color_range)),
        ];
        avformat_close_input(&raw mut input);
        tags
    }
}

#[test]
fn videotoolbox_encoders_read_the_frames_skia_rendered() {
    let metal = SkiaMetalCtx::new(TwoColors::WIDTH, TwoColors::HEIGHT).expect("a Metal device");

    let configurations = [
        (SkiaFrameExport::Auto, 1),
        (SkiaFrameExport::Auto, 3),
        (SkiaFrameExport::GpuConversion, 1),
        (SkiaFrameExport::CpuConversion, 1),
    ];
    for (encoder, configuration) in ENCODERS
        .into_iter()
        .flat_map(|encoder| configurations.map(|config| (encoder, config)))
    {
        let Some(output) = render(&metal, &TwoColors, encoder, configuration, YuvMatrix::Bt601)
        else {
            continue;
        };

        let mut decoder = unsafe { FFmpegDecoder::new(&output, FPS, 1) }.unwrap();
        for frame in 0..FRAMES {
            let expected = if frame < SWITCH_AT {
                [255, 0, 0]
            } else {
                [0, 0, 255]
            };
            assert_color_close(center_pixel(&mut decoder, frame), expected, frame);
        }
        assert!(!unsafe { decoder.decode_up_to(FRAMES as i64) }.unwrap());
        drop(decoder);

        let _ = std::fs::remove_dir_all(output.parent().unwrap());
    }
}

#[test]
fn videotoolbox_encoders_convert_with_bt709_when_asked() {
    let metal = SkiaMetalCtx::new(Flat::WIDTH, Flat::HEIGHT).expect("a Metal device");

    let modes = [
        SkiaFrameExport::Auto,
        SkiaFrameExport::GpuConversion,
        SkiaFrameExport::CpuConversion,
    ];
    for (encoder, mode) in ENCODERS
        .into_iter()
        .flat_map(|encoder| modes.map(|mode| (encoder, mode)))
    {
        let Some(output) = render(&metal, &Flat, encoder, (mode, 1), YuvMatrix::Bt709) else {
            continue;
        };

        assert_eq!(
            color_tags(&output),
            ["bt709", "bt709", "bt709", "tv"].map(String::from),
            "{encoder} {mode:?}"
        );

        // The encoders land within 3 of the rendered color, the BT.601 weights 15 off in green.
        let mut decoder = unsafe { FFmpegDecoder::new(&output, FPS, 1) }.unwrap();
        for frame in 0..FLAT_FRAMES {
            let pixel = center_pixel(&mut decoder, frame);
            assert!(
                pixel[..3].iter().zip(FLAT).all(|(a, e)| a.abs_diff(e) <= 5),
                "{encoder} {mode:?} frame {frame}: rendered {FLAT:?}, decoded {pixel:?}"
            );
        }
        drop(decoder);

        let _ = std::fs::remove_dir_all(output.parent().unwrap());
    }
}
