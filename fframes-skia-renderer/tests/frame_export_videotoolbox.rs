//! `VideoToolbox` encoders reading the pixel buffers Skia rendered into with Metal.
//!
//! ```sh
//! cargo test -p fframes_skia_renderer --features metal,fframes/videotoolbox --test frame_export_videotoolbox
//! ```
#![cfg(all(target_os = "macos", feature = "metal"))]

use fframes::ffmpeg_sys_fframes::AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX;
use fframes::media::FFmpegDecoder;
use fframes::{
    AudioMap, Color, Duration, EncoderOptions, FFramesContext, Frame, RenderOptions, Svgr, Video,
    VideoEncoderInfo,
};
use fframes_skia_renderer::metal::SkiaMetalCtx;
use fframes_skia_renderer::{SkiaFFramesRenderer, SkiaFrameExport, SkiaPipelineConfig, negotiate};

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

#[test]
fn videotoolbox_encoders_read_the_frames_skia_rendered() {
    let metal = SkiaMetalCtx::new(TwoColors::WIDTH, TwoColors::HEIGHT).expect("a Metal device");

    for encoder in ["h264_videotoolbox", "hevc_videotoolbox"] {
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
                bitrate: Some(4_000_000),
                codec_params: Some(&[("allow_sw", "0")]),
                ..Default::default()
            },
            ..Default::default()
        };

        let info = VideoEncoderInfo::for_output(
            &output,
            (
                TwoColors::WIDTH as i32,
                TwoColors::HEIGHT as i32,
                FPS as i32,
            ),
            &options.video_encoder_options,
        )
        .unwrap();
        if info.name() != encoder {
            eprintln!("skipping {encoder}: FFmpeg was built without the `videotoolbox` feature");
            continue;
        }

        let input = negotiate(&metal, SkiaFrameExport::Auto, &info).unwrap();
        assert_eq!(
            input.pixel_format, AV_PIX_FMT_VIDEOTOOLBOX,
            "{encoder} did not get hardware frames"
        );

        fframes::render(
            &output,
            &TwoColors,
            SkiaFFramesRenderer::new_metal(
                &metal,
                SkiaPipelineConfig {
                    encoder_threads: 6,
                    ..Default::default()
                },
            )
            .unwrap(),
            &options,
        )
        .unwrap_or_else(|err| panic!("{encoder}: {err:?}"));

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

        let _ = std::fs::remove_dir_all(&dir);
    }
}
