#![cfg(not(target_arch = "wasm32"))]

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use fframes_media::{FFmpegDecoder, FrameConvertOptions, ResizeVideoFrame};

const FPS: usize = 30;
const FRAME_COUNT: i64 = 300;

struct VideoFixture(PathBuf);

impl VideoFixture {
    fn new(b_frames: usize) -> Self {
        Self::with_rate(b_frames, FPS, FRAME_COUNT)
    }

    // Requires the ffmpeg CLI with libx264, like the end-to-end render tests.
    fn with_rate(b_frames: usize, rate: usize, frames: i64) -> Self {
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
                &format!("testsrc2=size=32x32:rate={rate}"),
                "-frames:v",
                &frames.to_string(),
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
fn an_older_frame_is_not_returned_after_a_failed_conversion() {
    let video = VideoFixture::new(0);
    // The scaler can't be created for an empty output
    let empty = FrameConvertOptions {
        resize: ResizeVideoFrame {
            width: 0,
            height: 0,
        },
    };
    // The CPU renderer decodes into a single reused buffer, the Skia pipeline into a ring.
    for buffer_size in [1, 3] {
        let mut decoder = unsafe { FFmpegDecoder::new(&video.path(), FPS, buffer_size) }.unwrap();
        // Fill every buffer, so the next frame is written into the buffer of an older one
        for index in 0..buffer_size as i64 {
            assert_frame(&mut decoder, index * 50);
        }
        let index = buffer_size as i64 * 50;
        unsafe {
            let mut reference = FFmpegDecoder::new(&video.path(), FPS, 1).unwrap();
            assert!(reference.decode_up_to(index).unwrap());
            let expected = reference
                .get_raw_frame()
                .convert_last_decoded_frame_into_svg_image(None)
                .unwrap();

            assert!(decoder.decode_up_to(index).unwrap());
            let frame = decoder.get_raw_frame();
            assert!(
                frame
                    .convert_last_decoded_frame_into_svg_image(Some(&empty))
                    .is_err()
            );
            let image = frame
                .convert_last_decoded_frame_into_svg_image(None)
                .unwrap();
            assert!(
                image.data == expected.data,
                "frame {index} differs from a decoder that converted it without a failure \
                 (buffer size {buffer_size})"
            );
        }
    }
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

#[test]
fn last_frame_is_shown_until_the_stream_ends_at_a_higher_fps() {
    // 48 frames at 24 fps last 2 s, i.e. 60 frames at 30 fps. Offset 59 (1.967 s) falls after the
    // last frame's timestamp (1.958 s) but before the end, so it must still show that frame.
    let video = VideoFixture::with_rate(3, 24, 48);
    let mut decoder = unsafe { FFmpegDecoder::new(&video.path(), FPS, 1) }.unwrap();
    let last_frame_seconds = 47. / 24.;
    for index in 0..60 {
        unsafe {
            assert!(
                decoder.decode_up_to(index).unwrap(),
                "missing frame {index}"
            );
            let frame = decoder.get_raw_frame();
            let expected_seconds = (index as f32 / FPS as f32 * 24.).ceil().min(47.) / 24.;
            assert!(
                (frame.timestamp_seconds() - expected_seconds).abs() < 0.0001,
                "wrong timestamp for frame {index}: {}",
                frame.timestamp_seconds()
            );
        }
    }
    let frame = decoder.get_raw_frame();
    assert!((unsafe { frame.timestamp_seconds() } - last_frame_seconds).abs() < 0.0001);
    let image = unsafe { frame.convert_last_decoded_frame_into_svg_image(None) }.unwrap();
    assert_eq!(image.data.len(), 32 * 32 * 4);

    // Past the end there is nothing to show, and looping wraps to the first frame.
    for _ in 0..2 {
        assert!(!unsafe { decoder.decode_up_to(60) }.unwrap());
    }
    let offset = unsafe { decoder.adjust_offset_for_looping(60) }.unwrap();
    assert_eq!(offset, 0);
    assert_frame(&mut decoder, 0);

    // A seek straight to the tail must not return a frame left over from before the seek.
    unsafe { decoder.seek_to_offset(59) }.unwrap();
    assert!(unsafe { decoder.decode_up_to(59) }.unwrap());
    let frame = decoder.get_raw_frame();
    assert!((unsafe { frame.timestamp_seconds() } - last_frame_seconds).abs() < 0.0001);
}
