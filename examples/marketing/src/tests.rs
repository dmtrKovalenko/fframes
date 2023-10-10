use fframes::{MediaProvider, TimeBase, Video};

#[test]
fn compile_time_svg() {
    let marketing_media = crate::MarketingMedia::new().unwrap();
    let video = crate::MarketingVideo {
        audio_track: "marketing.mp3",
        media: &marketing_media,
    };

    let frame = video.render_frame(
        fframes::Frame {
            index: 264,
            global_index: 264,
            fps: crate::MarketingVideo::FPS,
            breaks_lru_cache: None,
        },
        &fframes::FFramesContext {
            time_base: TimeBase {
                fps: crate::MarketingVideo::FPS,
                sample_rate: 44100,
            },
            mode: fframes::FFramesMode::Renderer,
            media_source: Some(&marketing_media as &dyn MediaProvider),
            duration_in_frames: 1200,
            font_source: None,
            scenes: None,
        },
    );

    fframes_test_utils::assert_compile_time_svgr_eq_runtime("marketing", frame);
}
