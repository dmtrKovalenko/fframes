use crate::MarketingVideo;
use fframes::TimeBase;
use fframes_renderer::fframes_renderer::cpu::CpuRenderingBackend;

#[test]
fn compile_time_svg() {
    use fframes::Video;

    let video = crate::MarketingVideo {
        audio_track: "marketing.mp3",
    };

    let (media_provider, _, timeline, _, _) =
        fframes_renderer::prepare_rendering_context::<MarketingVideo, CpuRenderingBackend>(
            &fframes_renderer::RenderOptions {
                media_dir: "media",
                ..Default::default()
            },
            &video,
        )
        .unwrap();

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
            media_provider: &media_provider,
            duration_in_frames: timeline.duration_in_frames,
            font_source: None,
            scenes: None,
        },
    );

    fframes_test_utils::assert_compile_time_svgr_eq_runtime("marketing", frame);
}
