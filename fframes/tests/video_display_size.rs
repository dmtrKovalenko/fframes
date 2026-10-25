#![cfg(not(target_arch = "wasm32"))]

use std::path::{Path, PathBuf};
use std::process::Command;

use fframes::FFramesSyncedVideoFrame;
use fframes::media::FFmpegDecoder;

const FPS: usize = 30;
const FRAMES_PER_SAMPLE_ASPECT: i64 = 30;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("fframes-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn ffmpeg(args: &[&str]) {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .output()
        .expect("ffmpeg with libx264 is required for video decoder tests");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn clip(dir: &Path, sample_aspects: &[&str], container: &str, container_args: &[&str]) -> PathBuf {
    let mut stream = Vec::new();
    for sample_aspect in sample_aspects {
        let part = dir.join("part.h264");
        ffmpeg(&[
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc2=size=64x32:rate={FPS}"),
            "-vf",
            &format!("setsar={sample_aspect}"),
            "-frames:v",
            &FRAMES_PER_SAMPLE_ASPECT.to_string(),
            "-c:v",
            "libx264",
            "-bf",
            "0",
            "-f",
            "h264",
            part.to_str().unwrap(),
        ]);
        stream.extend(std::fs::read(&part).unwrap());
    }
    let stream_path = dir.join("stream.h264");
    std::fs::write(&stream_path, stream).unwrap();
    let clip = dir.join(format!("clip.{container}"));
    let fps = FPS.to_string();
    let mut args = vec![
        "-r",
        &fps,
        "-f",
        "h264",
        "-i",
        stream_path.to_str().unwrap(),
        "-c",
        "copy",
    ];
    args.extend_from_slice(container_args);
    args.push(clip.to_str().unwrap());
    ffmpeg(&args);
    clip
}

#[test]
fn frames_report_the_size_players_show_non_square_pixels_at() {
    let dir = TempDir::new("display-size");
    let expected = [("0", (64, 32)), ("10/11", (64, 35)), ("4/3", (85, 32))];
    let sample_aspects = expected.map(|(sample_aspect, _)| sample_aspect);
    let clip = clip(&dir.0, &sample_aspects, "mp4", &[]);
    let mut decoder = unsafe { FFmpegDecoder::new(&clip, FPS, 1) }.unwrap();

    for (index, (sample_aspect, display)) in expected.into_iter().enumerate() {
        assert!(unsafe { decoder.decode_up_to(index as i64 * FRAMES_PER_SAMPLE_ASPECT) }.unwrap());
        let frame = decoder.get_raw_frame();
        let image = frame.into_image();
        assert_eq!(
            (frame.width(), frame.height()),
            (64, 32),
            "native size at SAR {sample_aspect}"
        );
        assert_eq!(
            (image.metadata.width, image.metadata.height),
            (64, 32),
            "image size at SAR {sample_aspect}"
        );
        assert_eq!(
            (frame.display_width(), frame.display_height()),
            display,
            "display size at SAR {sample_aspect}"
        );
    }
}

#[test]
fn the_container_sample_aspect_wins_over_the_frames() {
    let dir = TempDir::new("container-aspect");
    let clip = clip(&dir.0, &["10/11"], "mkv", &["-aspect", "2:1"]);
    let mut decoder = unsafe { FFmpegDecoder::new(&clip, FPS, 1) }.unwrap();
    assert!(unsafe { decoder.decode_up_to(0) }.unwrap());
    let frame = decoder.get_raw_frame();
    assert_eq!((frame.display_width(), frame.display_height()), (64, 32));
}

fn set_mp4_sample_aspect(clip: &Path, (horizontal, vertical): (u32, u32)) {
    let mut bytes = std::fs::read(clip).unwrap();
    let pasp = bytes.windows(4).position(|tag| tag == b"pasp").unwrap() + 4;
    bytes[pasp..pasp + 4].copy_from_slice(&horizontal.to_be_bytes());
    bytes[pasp + 4..pasp + 8].copy_from_slice(&vertical.to_be_bytes());
    std::fs::write(clip, bytes).unwrap();
}

#[test]
fn a_container_sample_aspect_that_collapses_the_image_falls_back_to_the_frames() {
    let dir = TempDir::new("invalid-container-aspect");
    let clip = clip(&dir.0, &["10/11"], "mp4", &[]);
    set_mp4_sample_aspect(&clip, (1, 30_000));
    let mut decoder = unsafe { FFmpegDecoder::new(&clip, FPS, 1) }.unwrap();
    assert!(unsafe { decoder.decode_up_to(0) }.unwrap());
    let frame = decoder.get_raw_frame();
    assert_eq!((frame.display_width(), frame.display_height()), (64, 35));
}
