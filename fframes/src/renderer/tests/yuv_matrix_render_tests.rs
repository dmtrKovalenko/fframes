//! Renders flat frames with each `YuvMatrix` and decodes them by the stream's tags, as
//! players do.

use crate::ffmpeg_sys_fframes::*;
use crate::media::FFmpegDecoder;
use crate::{
    AudioMap, Color, Duration, EncoderOptions, FFramesContext, Frame, RenderOptions, Svgr, Video,
    YuvMatrix, cpu::CpuRenderingBackend, fframes_logger::FFramesLoggerVariant,
};
use std::ffi::{CStr, CString, c_char};
use std::path::Path;

/// Mid saturation, so nothing clips. Decoded with the other matrix it is off by more than
/// 10 in green.
const FLAT: [u8; 3] = [60, 179, 113];

#[derive(Debug)]
struct Flat;

impl Video for Flat {
    const FPS: usize = 10;
    const WIDTH: usize = 64;
    const HEIGHT: usize = 64;
    const BACKGROUND_COLOR: Color = Color::rgb(FLAT[0], FLAT[1], FLAT[2]);

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(4)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    // The background fills the frame.
    fn render_frame<'a>(&'a self, _frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        crate::svgr!(<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"></svg>)
    }
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
        // h264 carries them in its headers
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
fn renders_and_tags_the_requested_yuv_matrix() {
    for (matrix, tags) in [
        (YuvMatrix::Bt601, ["smpte170m", "unknown", "unknown", "tv"]),
        (YuvMatrix::Bt709, ["bt709", "bt709", "bt709", "tv"]),
    ] {
        let directory = std::env::temp_dir().join(format!(
            "fframes-matrix-{matrix:?}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir(&directory).unwrap();
        let output = directory.join("flat.mp4");

        crate::render(
            &output,
            &Flat,
            CpuRenderingBackend {
                concurrency: 1,
                ..Default::default()
            },
            &RenderOptions {
                logger: FFramesLoggerVariant::Silent,
                tmp_files_directory: Some(&directory.join("chunks")),
                video_encoder_options: EncoderOptions {
                    preferred_encoder: Some("libx264"),
                    color_matrix: matrix,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .unwrap_or_else(|err| panic!("{matrix:?}: {err}"));

        assert_eq!(color_tags(&output), tags.map(String::from), "{matrix:?}");

        let pixel = unsafe {
            let mut decoder = FFmpegDecoder::new(&output, Flat::FPS, 1).unwrap();
            assert!(decoder.decode_up_to(1).unwrap());
            let image = decoder
                .get_raw_frame()
                .convert_last_decoded_frame_into_svg_image(None)
                .unwrap();
            let center = (32 * 64 + 32) * 4;
            [
                image.data[center],
                image.data[center + 1],
                image.data[center + 2],
            ]
        };
        assert!(
            pixel.iter().zip(FLAT).all(|(a, e)| a.abs_diff(e) <= 2),
            "{matrix:?}: rendered {FLAT:?}, decoded {pixel:?}"
        );

        std::fs::remove_dir_all(directory).unwrap();
    }
}
