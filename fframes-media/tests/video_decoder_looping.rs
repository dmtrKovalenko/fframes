#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;
use std::process::Command;

use fframes_media::FFmpegDecoder;

const FPS: usize = 30;
const FRAME_COUNT: i64 = 90;

struct VideoFixture(PathBuf);

impl VideoFixture {
    // Requires the ffmpeg CLI with libx264, like the end-to-end render tests.
    fn new(extension: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "fframes-decoder-looping-{}-{extension}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = Self(dir.join(format!("video.{extension}")));
        let output = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=32x32:rate=30",
                "-frames:v",
                &FRAME_COUNT.to_string(),
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-g",
                "30",
                "-an",
            ])
            .arg(&fixture.0)
            .output()
            .expect("ffmpeg with libx264 is required for video decoder tests");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fixture
    }
}

impl Drop for VideoFixture {
    fn drop(&mut self) {
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

fn assert_loops(extension: &str) {
    let video = VideoFixture::new(extension);
    let mut decoder = unsafe { FFmpegDecoder::new(&video.0, FPS, 1) }.unwrap();
    let seconds = unsafe { decoder.get_raw_frame().get_stream_duration_in_frames() };
    assert!(
        (seconds - FRAME_COUNT as f32 / FPS as f32).abs() < 0.05,
        "{extension}: stream duration {seconds}s"
    );

    for index in 0..FRAME_COUNT * 2 + 5 {
        unsafe {
            let offset = decoder.adjust_offset_for_looping(index).unwrap();
            assert_eq!(offset, index % FRAME_COUNT, "{extension}: request {index}");
            assert!(
                decoder.decode_up_to(offset).unwrap(),
                "{extension}: missing frame {index}"
            );
            // Matroska stores milliseconds, so frame 7 sits at 0.233s: compare to the nearest frame.
            let shown = decoder.get_raw_frame().timestamp_seconds() * FPS as f32;
            assert!(
                (shown - offset as f32).abs() < 0.5,
                "{extension}: request {index} showed frame {shown}"
            );
        }
    }
}

#[test]
fn mp4_clips_loop_back_to_the_start() {
    assert_loops("mp4");
}

// Matroska (and WebM) leave the video stream's duration unset; the container carries it.
#[test]
fn mkv_clips_loop_back_to_the_start() {
    assert_loops("mkv");
}
