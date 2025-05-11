use fframes::{MediaProvider, TimeBase, Video, VideoSize};

#[test]
fn compile_time_svg() {
    let marketing_media = crate::MarketingMedia::new().unwrap();
    let video = crate::MarketingVideo {
        audio_track: "marketing.mp3",
        media: &marketing_media,
    };

    let frame = video.render_frame(
        fframes::Frame::new(264, 264, crate::MarketingVideo::FPS),
        &fframes::FFramesContext {
            time_base: TimeBase {
                fps: crate::MarketingVideo::FPS,
                sample_rate: 44100,
            },
            current_video_size: VideoSize {
                width: crate::MarketingVideo::WIDTH,
                height: crate::MarketingVideo::HEIGHT,
            },
            mode: fframes::FFramesMode::Renderer,
            media_source: Some(&marketing_media as &dyn MediaProvider),
            duration_in_frames: 1200,
            font_source: None,
            scenes: None,
            abort_signal: None,
        },
    );

    // It looks like there are a slight difference on how ffmpeg decodes audio on different OS
    // the difference in minimal and might be related to the resolutiono of sample rate
    fframes_test_utils::assert_compile_time_svgr_eq_runtime(
        &format!("marketing-{}", std::env::consts::OS),
        frame,
    );
}
