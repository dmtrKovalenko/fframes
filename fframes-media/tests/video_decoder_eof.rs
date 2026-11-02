#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use fframes_media::{FFmpegDecoder, FrameConvertOptions, PlaybackSpeed, ResizeVideoFrame};

const FPS: usize = 30;
const FRAME_COUNT: i64 = 300;

struct VideoFixture(PathBuf);

impl VideoFixture {
    fn new(b_frames: usize) -> Self {
        Self::with_rate(b_frames, FPS, FRAME_COUNT)
    }

    fn with_rate(b_frames: usize, rate: usize, frames: i64) -> Self {
        Self::with_size(b_frames, rate, frames, (32, 32))
    }

    // Requires the ffmpeg CLI with libx264, like the end-to-end render tests.
    fn with_size(b_frames: usize, rate: usize, frames: i64, (width, height): (u32, u32)) -> Self {
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
                &format!("testsrc2=size={width}x{height}:rate={rate}"),
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
fn retained_frame_keeps_its_pixels_and_timestamp_across_decode_and_seek() {
    let video = VideoFixture::new(3);
    let mut decoder = unsafe { FFmpegDecoder::new(&video.path(), FPS, 1) }.unwrap();
    unsafe {
        assert!(decoder.decode_up_to(3).unwrap());
        let held = decoder.retained_frame().unwrap();
        let pts = held.get_pts();
        let expected = held
            .convert_last_decoded_frame_into_svg_image(None)
            .unwrap();

        std::thread::scope(|scope| {
            let decode = scope.spawn(move || {
                assert_frame(&mut decoder, 5);
                decoder.seek_to_offset(0).unwrap();
                assert_frame(&mut decoder, 0);
            });
            assert_eq!(held.get_pts(), pts);
            decode.join().unwrap();
        });

        assert_eq!(held.get_pts(), pts);
        let actual = held
            .convert_last_decoded_frame_into_svg_image(None)
            .unwrap();
        assert_eq!(actual.data, expected.data);
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
    assert!(unsafe { decoder.decode_looping(FRAME_COUNT) }.unwrap());
    assert_frame(&mut decoder, 0);
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
    // A single buffer, as in the CPU renderer, so frame 50 is written into frame 0's buffer
    let mut decoder = unsafe { FFmpegDecoder::new(&video.path(), FPS, 1) }.unwrap();
    assert_frame(&mut decoder, 0);
    unsafe {
        let mut reference = FFmpegDecoder::new(&video.path(), FPS, 1).unwrap();
        assert!(reference.decode_up_to(50).unwrap());
        let expected = reference
            .get_raw_frame()
            .convert_last_decoded_frame_into_svg_image(None)
            .unwrap();

        assert!(decoder.decode_up_to(50).unwrap());
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
            "frame 50 differs from a decoder that converted it without a failure"
        );
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
fn converted_images_survive_single_buffer_reuse_and_decoder_drop() {
    let video = VideoFixture::new(3);
    let mut decoder = unsafe { FFmpegDecoder::new(&video.path(), FPS, 1) }.unwrap();
    let mut images = Vec::new();
    for index in [0, 17, 2, 90, 299] {
        assert!(unsafe { decoder.decode_up_to(index) }.unwrap());
        let image = unsafe {
            decoder
                .get_raw_frame()
                .convert_last_decoded_frame_into_svg_image(None)
        }
        .unwrap();
        let expected = image.data.to_vec();
        images.push((image, expected));
    }
    drop(decoder);
    for (image, expected) in images {
        assert_eq!(image.data.as_ref(), expected);
    }
}

#[test]
fn released_frames_lend_their_pixel_buffers_to_later_conversions() {
    let video = VideoFixture::new(0);
    let mut decoder = unsafe { FFmpegDecoder::new(&video.path(), FPS, 1) }.unwrap();
    let mut convert = |index| {
        assert!(unsafe { decoder.decode_up_to(index) }.unwrap());
        unsafe {
            decoder
                .get_raw_frame()
                .convert_last_decoded_frame_into_svg_image(None)
        }
        .unwrap()
    };
    let first = convert(0);
    let first_pixels = first.data.as_ptr();
    // The first image is still held, so the second one needs its own buffer.
    let second = convert(30);
    assert_ne!(second.data.as_ptr(), first_pixels);
    drop(first);
    // Once released, the buffer of the first image is reused.
    let third = convert(60);
    assert_eq!(third.data.as_ptr(), first_pixels);
    assert_ne!(third.data.as_ref(), second.data.as_ref());
    assert_eq!(third.data.len(), 32 * 32 * 4);
}

#[test]
fn preview_decoders_serve_recent_frames_out_of_order_without_seeking() {
    use fframes_media::DecodeMode;

    let video = VideoFixture::new(2);
    let mut decoder =
        unsafe { FFmpegDecoder::new_with_mode(&video.path(), FPS, 1, DecodeMode::Preview) }
            .unwrap();
    // Parallel workers reach the decoder out of order within a few frames.
    for index in [100, 102, 101, 104, 103, 100, 105, 107, 106] {
        assert_frame(&mut decoder, index);
    }
    assert_eq!(decoder.seeks(), 1, "only the jump from the start seeks");
    // Beyond the recent frames the decoder seeks back as before.
    assert_frame(&mut decoder, 90);
    assert_eq!(decoder.seeks(), 2);
    assert_frame(&mut decoder, 91);
    assert_eq!(decoder.seeks(), 2);
}

#[test]
fn a_frame_requested_again_at_another_size_is_converted_to_that_size() {
    let video = VideoFixture::new(0);
    let thumbnail = FrameConvertOptions {
        resize: ResizeVideoFrame {
            width: 16,
            height: 16,
        },
    };
    let thumbnail_len = 16 * 16 * 4;
    // The CPU renderer decodes into a single reused buffer, the Skia pipeline into a ring.
    for buffer_size in [1, 3] {
        unsafe {
            let mut decoder = FFmpegDecoder::new(&video.path(), FPS, buffer_size).unwrap();
            let mut reference = FFmpegDecoder::new(&video.path(), FPS, 1).unwrap();
            assert!(decoder.decode_up_to(5).unwrap());
            assert!(reference.decode_up_to(5).unwrap());
            let expected = reference
                .get_raw_frame()
                .convert_last_decoded_frame_into_svg_image(Some(&thumbnail))
                .unwrap();

            // One frame drawn at its own size and as a thumbnail, e.g. by two scenes
            let frame = decoder.get_raw_frame();
            let image = frame
                .convert_last_decoded_frame_into_svg_image(None)
                .unwrap();
            assert_eq!((image.width, image.height), (32, 32));
            let thumb = frame
                .convert_last_decoded_frame_into_svg_image(Some(&thumbnail))
                .unwrap();
            assert_eq!((thumb.width, thumb.height), (16, 16));
            assert_eq!(thumb.data.len(), thumbnail_len);
            assert_eq!(thumb.data, expected.data);
            let again = frame
                .convert_last_decoded_frame_into_svg_image(Some(&thumbnail))
                .unwrap();
            assert!(Arc::ptr_eq(&thumb, &again));
            let pts = frame.get_pts();
            assert!(Arc::ptr_eq(
                &thumb,
                &decoder.get_decoded_image_in_buf(pts).unwrap()
            ));
            assert!(decoder.decode_up_to(6).unwrap());
            let _full_size = frame
                .convert_last_decoded_frame_into_svg_image(None)
                .unwrap();
            if buffer_size == 1 {
                assert!(decoder.get_decoded_image_in_buf(pts).is_none());
            } else {
                // The cached thumbnail retains its own dimensions after a full-size conversion.
                assert!(Arc::ptr_eq(
                    &thumb,
                    &decoder.get_decoded_image_in_buf(pts).unwrap()
                ));
            }
        }
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
    assert!(unsafe { decoder.decode_looping(60) }.unwrap());
    assert_frame(&mut decoder, 0);

    // A seek straight to the tail must not return a frame left over from before the seek.
    unsafe { decoder.seek_to_offset(59) }.unwrap();
    assert!(unsafe { decoder.decode_up_to(59) }.unwrap());
    let frame = decoder.get_raw_frame();
    assert!((unsafe { frame.timestamp_seconds() } - last_frame_seconds).abs() < 0.0001);
}

fn convert_frame(decoder: &mut FFmpegDecoder, index: i64, size: Option<(u32, u32)>) -> Vec<u8> {
    let options = size.map(|(width, height)| FrameConvertOptions {
        resize: ResizeVideoFrame { width, height },
    });
    let (width, height) = size.unwrap_or((32, 32));
    unsafe {
        assert!(
            decoder.decode_up_to(index).unwrap(),
            "missing frame {index}"
        );
        let image = decoder
            .get_raw_frame()
            .convert_last_decoded_frame_into_svg_image(options.as_ref())
            .unwrap();
        assert_eq!((image.width, image.height), (width, height));
        assert_eq!(
            image.data.len(),
            (width * height * 4) as usize,
            "wrong data length for frame {index} at {width}x{height}"
        );
        image.data.to_vec()
    }
}

#[test]
fn frames_can_be_requested_at_a_larger_size_than_before() {
    let video = VideoFixture::new(0);
    // The 32x32 stream at its own size, grown and shrunk, four frames each.
    let sizes = [None, Some((256, 256)), Some((16, 16))];
    // The CPU renderer decodes into a single reused buffer, the Skia pipeline into a ring.
    for buffer_size in [1, 3] {
        let mut decoder = unsafe { FFmpegDecoder::new(&video.path(), FPS, buffer_size) }.unwrap();
        // Decoders that are only ever asked for one size
        let mut references = HashMap::new();
        for index in 0..12 {
            let size = sizes[index as usize / 4];
            let image = convert_frame(&mut decoder, index, size);
            let reference = references
                .entry(size)
                .or_insert_with(|| unsafe { FFmpegDecoder::new(&video.path(), FPS, 1) }.unwrap());
            assert!(
                image == convert_frame(reference, index, size),
                "frame {index} at {size:?} differs from a decoder that only converts to that size \
                 (buffer size {buffer_size})"
            );
        }
    }
}

#[test]
fn frames_can_be_converted_at_widths_that_are_not_a_multiple_of_16() {
    // At their own size, swscale's x86 YUV to RGB converter writes these rows in whole blocks
    // of 16 pixels, from 56 bytes (2 px wide) to 8 bytes (46 px) past the last one
    for width in [2, 40, 42, 44, 46] {
        let video = VideoFixture::with_size(0, FPS, 8, (width, 32));
        let mut decoder = unsafe { FFmpegDecoder::new(&video.path(), FPS, 1) }.unwrap();
        for index in 0..8 {
            convert_frame(&mut decoder, index, Some((width, 32)));
        }
    }
}

#[test]
fn preview_decoding_preserves_frames_and_color_across_seeks() {
    use fframes_media::DecodeMode;
    let fixture = VideoFixture::with_size(3, FPS, 56, (320, 192));
    let file = fixture.path();
    let mut preview =
        unsafe { FFmpegDecoder::new_with_mode(&file, FPS, 1, DecodeMode::Preview) }.unwrap();
    let mut software =
        unsafe { FFmpegDecoder::new_with_mode(&file, FPS, 1, DecodeMode::Software) }.unwrap();
    for i in [0, 1, 16, 9, 55, 2] {
        unsafe {
            assert!(preview.decode_up_to(i).unwrap() && software.decode_up_to(i).unwrap());
            let held = preview.retained_frame().unwrap();
            let reference = software.retained_frame().unwrap();
            assert_eq!(held.get_pts(), reference.get_pts());
            // Changing the cursor must not invalidate a mapped VideoToolbox surface.
            assert!(preview.decode_up_to((i + 1) % 56).unwrap());
            let a = held
                .convert_last_decoded_frame_into_svg_image(None)
                .unwrap();
            let b = reference
                .convert_last_decoded_frame_into_svg_image(None)
                .unwrap();
            assert_eq!((a.width, a.height), (320, 192));
            assert_eq!(a.data.len(), b.data.len());
            // Hardware can reconstruct a channel a little differently than libavcodec.
            let error: usize = a
                .data
                .iter()
                .zip(b.data.iter())
                .map(|(x, y)| x.abs_diff(*y) as usize)
                .sum();
            assert!(
                error < a.data.len() * 2,
                "frame {i}: mean channel difference exceeds two"
            );
        }
    }
}

#[test]
fn preview_falls_back_for_lossless_alpha_video() {
    use fframes_media::DecodeMode;
    let fixture = VideoFixture::with_rate(0, FPS, 8);
    let file = fixture.path().with_extension("mkv");
    let output = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=32x32:rate=30,format=bgra,colorchannelmixer=aa=0.5",
            "-frames:v",
            "8",
            "-c:v",
            "ffv1",
        ])
        .arg(&file)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut preview =
        unsafe { FFmpegDecoder::new_with_mode(&file, FPS, 1, DecodeMode::Preview) }.unwrap();
    let mut software =
        unsafe { FFmpegDecoder::new_with_mode(&file, FPS, 1, DecodeMode::Software) }.unwrap();
    for i in [0, 6, 3] {
        let pixels = convert_frame(&mut preview, i, None);
        assert_eq!(pixels, convert_frame(&mut software, i, None));
        assert!((127..=128).contains(&pixels[3]));
    }
}

/// The source frame that offset `n` shows at `num / den` speed: the first frame at or
/// after `n * num / den` (the last frame until the stream ends), in a loop if `looping`.
fn expected_frame(offset: i64, num: i64, den: i64, looping: bool) -> i64 {
    let mut position = offset * num;
    if looping {
        position %= FRAME_COUNT * den;
    }
    ((position + den - 1) / den).min(FRAME_COUNT - 1)
}

fn assert_shows(decoder: &FFmpegDecoder, speed: f32, offset: i64, frame: i64) {
    let seconds = unsafe { decoder.get_raw_frame().timestamp_seconds() };
    assert!(
        (seconds - frame as f32 / FPS as f32).abs() < 0.0001,
        "{speed}x: offset {offset} shows {seconds}s, expected frame {frame}"
    );
}

fn assert_speed(speed: f32, num: i64, den: i64) {
    let video = VideoFixture::new(3);
    let speed_of = |speed| {
        unsafe { FFmpegDecoder::new(&video.path(), FPS, 1) }
            .unwrap()
            .with_playback_speed(PlaybackSpeed::new(speed))
    };

    // Every offset before the end of the stream has a frame, also when the stream's
    // duration is not a whole number of offsets.
    let mut decoder = speed_of(speed);
    let offsets = (FRAME_COUNT * den + num - 1) / num;
    for offset in 0..offsets {
        assert!(
            unsafe { decoder.decode_up_to(offset) }.unwrap(),
            "{speed}x: offset {offset}"
        );
        assert_shows(
            &decoder,
            speed,
            offset,
            expected_frame(offset, num, den, false),
        );
    }
    assert!(!unsafe { decoder.decode_up_to(offsets) }.unwrap());

    // Loops follow the exact duration and do not drift.
    let mut decoder = speed_of(speed);
    for offset in 0..3 * offsets {
        assert!(
            unsafe { decoder.decode_looping(offset) }.unwrap(),
            "{speed}x looping: offset {offset}"
        );
        assert_shows(
            &decoder,
            speed,
            offset,
            expected_frame(offset, num, den, true),
        );
    }
}

#[test]
fn playback_speed_scales_the_source_time() {
    assert_speed(2.0, 2, 1);
    assert_speed(1.5, 3, 2);
    assert_speed(1.4, 7, 5);
    assert_speed(0.5, 1, 2);
}
