//! Platform smoke test: renders a short video with the CPU backend and decodes it back
//! with the fframes decoder. Unlike the visual regression test it needs no reference
//! frames or external tools (ffmpeg CLI, odiff), so it runs on every OS in CI, Windows
//! included.

use fframes::media::FFmpegDecoder;
use fframes::{AudioMap, Color, Duration, FFramesContext, Frame, RenderOptions, Svgr, Video};

const FPS: usize = 10;
const FRAMES: usize = 20;
const SWITCH_AT: usize = FRAMES / 2;

#[derive(Debug)]
struct SmokeVideo;

impl Video for SmokeVideo {
    const FPS: usize = FPS;
    const WIDTH: usize = 320;
    const HEIGHT: usize = 240;
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

        assert_eq!(image.width as usize, SmokeVideo::WIDTH);
        assert_eq!(image.height as usize, SmokeVideo::HEIGHT);

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
fn renders_and_decodes_video() {
    let dir = std::env::temp_dir().join(format!("fframes-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let output = dir.join("smoke.mp4");

    fframes::render(
        &output,
        &SmokeVideo,
        fframes::cpu::CpuRenderingBackend {
            concurrency: 2,
            ..Default::default()
        },
        &RenderOptions {
            logger: fframes::fframes_logger::FFramesLoggerVariant::Silent,
            tmp_files_directory: Some(&dir.join("chunks")),
            ..Default::default()
        },
    )
    .unwrap();

    assert!(std::fs::metadata(&output).unwrap().len() > 0);

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
