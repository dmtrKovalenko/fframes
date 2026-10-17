//! Renders `ProRes` and decodes it with the `ffmpeg` CLI (#230). `ProRes` keeps the colors
//! of a picture in every frame header and `ffmpeg` converts each frame by those, not by the
//! stream's tags: untagged frames are taken as BT.601, which turned BT.709 red into 231, 0, 1.
//! Requires the ffmpeg CLI, like the decoder tests of fframes-media.

use crate::ffmpeg_sys_fframes::AVPixelFormat;
use crate::{
    AudioMap, Color, Duration, EncoderOptions, FFramesContext, Frame, RenderOptions, Svgr, Video,
    YuvMatrix, cpu::CpuRenderingBackend, fframes_logger::FFramesLoggerVariant,
};
use std::path::Path;
use std::process::Command;

const RED: [u8; 3] = [255, 0, 0];
/// Mid saturation, so nothing clips. Decoded with the other matrix it is off by 15 in green.
const FLAT: [u8; 3] = [60, 179, 113];
/// Several segments, concatenated into one file.
const FRAMES: usize = 96;

#[derive(Debug)]
struct RedAndFlat;

impl Video for RedAndFlat {
    const FPS: usize = 24;
    const WIDTH: usize = 64;
    const HEIGHT: usize = 64;
    const BACKGROUND_COLOR: Color = Color::rgb(FLAT[0], FLAT[1], FLAT[2]);

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(FRAMES)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    // Red on the left half, the background on the right.
    fn render_frame<'a>(&'a self, _frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        crate::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width="64" height="64">
                <rect x="0" y="0" width="32" height="64" fill="#ff0000" />
            </svg>
        )
    }
}

/// Every frame of `path` as RGB, converted by `ffmpeg` as in the issue.
fn decode_with_ffmpeg(path: &Path) -> Vec<u8> {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .args(["-vf", "scale=out_range=pc,format=rgb24"])
        .args(["-f", "rawvideo", "-"])
        .output()
        .expect("the ffmpeg CLI is required for the ProRes color test");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

#[test]
fn ffmpeg_decodes_prores_to_the_rendered_colors() {
    for matrix in [YuvMatrix::Bt601, YuvMatrix::Bt709] {
        let directory = std::env::temp_dir().join(format!(
            "fframes-prores-{matrix:?}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir(&directory).unwrap();
        let output = directory.join("out.mov");

        crate::render(
            &output,
            &RedAndFlat,
            CpuRenderingBackend::default(),
            &RenderOptions {
                logger: FFramesLoggerVariant::Silent,
                tmp_files_directory: Some(&directory.join("chunks")),
                video_encoder_options: EncoderOptions {
                    preferred_encoder: Some("prores_ks"),
                    pixel_format: AVPixelFormat::AV_PIX_FMT_YUV422P10LE,
                    codec_params: Some(&[("profile", "3")]),
                    color_matrix: matrix,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap_or_else(|err| panic!("{matrix:?}: {err}"));

        let frame_size = RedAndFlat::WIDTH * RedAndFlat::HEIGHT * 3;
        let rgb = decode_with_ffmpeg(&output);
        assert_eq!(rgb.len(), FRAMES * frame_size, "{matrix:?}");

        for (index, frame) in rgb.chunks_exact(frame_size).enumerate() {
            for (x, drawn) in [(16, RED), (48, FLAT)] {
                let at = (32 * RedAndFlat::WIDTH + x) * 3;
                let decoded = &frame[at..at + 3];
                assert!(
                    decoded.iter().zip(drawn).all(|(a, e)| a.abs_diff(e) <= 5),
                    "{matrix:?}: frame {index} drew {drawn:?} at x {x}, ffmpeg decoded {decoded:?}"
                );
            }
        }

        std::fs::remove_dir_all(directory).unwrap();
    }
}
