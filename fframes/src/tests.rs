use crate::{
    media_provider::MediaProvider, resolve_duration_and_scenes_async, AudioMap,
    AudioTimelineSamples, AudioTimestamp, FFramesContext, Frame, Scene, SceneInfo, Video,
};
use futures::executor::block_on;

#[derive(Debug)]
struct FakeScene {}

impl Scene for FakeScene {
    fn audio_map(&self, _: &SceneInfo) -> AudioMap {
        AudioMap::from([
            (
                "scene1.mp3",
                (AudioTimestamp::Second(0), AudioTimestamp::Second(10)),
            ),
            (
                "scene2.mp3",
                (AudioTimestamp::Second(20), AudioTimestamp::Second(40)),
            ),
        ])
    }

    fn duration(&self) -> crate::video::Duration {
        crate::video::Duration::Seconds(30.)
    }

    fn render_frame(&self, _: crate::frame::Frame, _: &crate::FFramesContext) -> crate::Svgr {
        unimplemented!()
    }
}

struct FakeVideo {}

impl Video for FakeVideo {
    const FPS: usize = 24;
    const WIDTH: usize = 100;
    const HEIGHT: usize = 100;

    fn audio(&self) -> AudioMap {
        AudioMap::from([
            (
                "audio1.mp3",
                (AudioTimestamp::Second(0), AudioTimestamp::Second(10)),
            ),
            (
                "audio2.mp3",
                (AudioTimestamp::Second(10), AudioTimestamp::Second(20)),
            ),
        ])
    }

    fn define_scenes(&self) -> crate::Scenes {
        crate::Scenes::from(vec![
            Box::new(FakeScene {}) as Box<dyn Scene>,
            Box::new(FakeScene {}) as Box<dyn Scene>,
        ])
    }

    fn render_frame(&self, _: Frame, _: &FFramesContext) -> crate::Svgr {
        unimplemented!()
    }
}

#[test]
fn test_audio_map_resolve() {
    let video = FakeVideo {};
    let (duration_in_frames, scenes) = block_on(resolve_duration_and_scenes_async(&video, |_| {
        Box::pin(async move { Ok(0) })
    }))
    .unwrap();

    let ctx = FFramesContext {
        fps: 24,
        duration_in_frames,
        sample_rate: 1000,
        mode: crate::FFramesMode::Editor,
        media_provider: &MediaProvider::default(),
        scenes: scenes.as_ref(),
        font_source: None,
    };

    let resolved_map = video.audio().resolve::<AudioTimelineSamples>(&ctx).unwrap();

    assert_eq!(
        resolved_map.0,
        vec![
            (
                "audio1.mp3".to_string(),
                AudioTimelineSamples(0)..AudioTimelineSamples(10000)
            ),
            (
                "audio2.mp3".to_string(),
                AudioTimelineSamples(10000)..AudioTimelineSamples(30000)
            ),
            (
                "scene1.mp3".to_string(),
                AudioTimelineSamples(0)..AudioTimelineSamples(10000)
            ),
            (
                "scene2.mp3".to_string(),
                AudioTimelineSamples(20000)..AudioTimelineSamples(60000)
            ),
            (
                "scene1.mp3".to_string(),
                AudioTimelineSamples(30000)..AudioTimelineSamples(60000)
            ),
            (
                "scene2.mp3".to_string(),
                AudioTimelineSamples(50000)..AudioTimelineSamples(60000)
            )
        ]
    );
}
