#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use fframes_media::FFmpegDecoder;

const FPS: usize = 30;
const FRAME_COUNT: i64 = 300;

struct VideoFixture(PathBuf);

impl VideoFixture {
    // Requires the ffmpeg CLI with libx264, like the end-to-end render tests.
    fn new(b_frames: usize) -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "fframes-decoder-eof-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = Self(dir);
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
                "300",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-g",
                "30",
                "-bf",
                &b_frames.to_string(),
                "-x264-params",
                "b-adapt=0:scenecut=0",
                "-an",
            ])
            .arg(fixture.path())
            .output()
            .expect("ffmpeg with libx264 is required for video decoder tests");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fixture
    }

    fn path(&self) -> PathBuf {
        self.0.join("video.mp4")
    }
}

impl Drop for VideoFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn assert_frame(decoder: &mut FFmpegDecoder, index: i64) {
    unsafe {
        assert!(
            decoder.decode_up_to(index).unwrap(),
            "missing frame {index}"
        );
        let frame = decoder.get_raw_frame();
        let expected_seconds = index as f32 / FPS as f32;
        assert!(
            (frame.timestamp_seconds() - expected_seconds).abs() < 0.0001,
            "wrong timestamp for frame {index}: {}",
            frame.timestamp_seconds()
        );
        let image = frame
            .convert_last_decoded_frame_into_svg_image(None)
            .unwrap();
        assert_eq!(image.data.len(), 32 * 32 * 4);
    }
}

#[test]
fn b_frames_are_drained_and_decoding_can_restart_after_eof() {
    let video = VideoFixture::new(3);
    let mut decoder = unsafe { FFmpegDecoder::new(&video.path(), FPS, 1) }.unwrap();
    for index in 0..FRAME_COUNT {
        assert_frame(&mut decoder, index);
    }
    // Repeating the last buffered frame must still work while draining.
    assert_frame(&mut decoder, FRAME_COUNT - 1);
    for _ in 0..2 {
        assert!(!unsafe { decoder.decode_up_to(FRAME_COUNT) }.unwrap());
    }

    // Explicit seeks and out-of-order requests must reset the decoder's EOF state.
    unsafe { decoder.seek_to_offset(0) }.unwrap();
    assert_frame(&mut decoder, 0);
    assert_frame(&mut decoder, FRAME_COUNT - 1);
    assert_frame(&mut decoder, FRAME_COUNT - 2);
    assert_frame(&mut decoder, FRAME_COUNT - 1);

    // Looping also seeks after decoding the delayed tail.
    let offset = unsafe { decoder.adjust_offset_for_looping(FRAME_COUNT) }.unwrap();
    assert_eq!(offset, 0);
    assert_frame(&mut decoder, offset);
    assert_frame(&mut decoder, FRAME_COUNT - 1);
}

#[test]
fn videos_without_b_frames_stop_at_eof() {
    let video = VideoFixture::new(0);
    let mut decoder = unsafe { FFmpegDecoder::new(&video.path(), FPS, 1) }.unwrap();
    for index in 0..FRAME_COUNT {
        assert_frame(&mut decoder, index);
    }
    for _ in 0..2 {
        assert!(!unsafe { decoder.decode_up_to(FRAME_COUNT) }.unwrap());
    }
}
