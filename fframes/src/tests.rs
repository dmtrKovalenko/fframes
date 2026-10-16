use crate::{
    AudioMap, AudioTimelineSamples, AudioTimestamp, FFramesContext, Frame,
    ResolvedRenderingTimeline, Scene, Video, resolve_timeline,
};

#[derive(Debug)]
struct FakeScene {}

impl Scene for FakeScene {
    fn audio(&self) -> AudioMap<'_> {
        use AudioTimestamp::*;

        AudioMap::from([
            ("scene1.mp3", Second(0.)..Second(10.)),
            ("scene2.mp3", Second(20.)..Eof + Second(1.)),
        ])
    }

    fn duration(&self) -> crate::Duration<'_> {
        crate::Duration::Seconds(30.)
    }

    fn render_frame(&self, _: crate::Frame, _: &crate::FFramesContext) -> crate::Svgr<'_> {
        unimplemented!()
    }
}

struct FakeVideo {}

impl Video for FakeVideo {
    const FPS: usize = 24;
    const WIDTH: usize = 100;
    const HEIGHT: usize = 100;

    fn duration(&self) -> crate::Duration<'_> {
        crate::Duration::Auto
    }

    fn audio(&self) -> AudioMap<'_> {
        use AudioTimestamp::*;

        AudioMap::from([
            ("audio1.mp3", Second(0.)..Second(10.)),
            ("audio2.mp3", Second(10.)..Second(20.)),
        ])
    }

    fn define_scenes(&self) -> crate::Scenes<'_> {
        crate::Scenes::from(vec![
            &FakeScene {} as &dyn Scene,
            &FakeScene {} as &dyn Scene,
        ])
    }

    fn render_frame(&self, _: Frame, _: &FFramesContext) -> crate::Svgr<'_> {
        unimplemented!()
    }
}

#[test]
fn test_audio_map_resolve() {
    let video = FakeVideo {};
    let audio_map = video.audio();

    let tb = crate::TimeBase {
        sample_rate: 1000,
        fps: 24,
    };

    let ResolvedRenderingTimeline { audio_map, .. } = resolve_timeline::<AudioTimelineSamples, _>(
        &video.duration(),
        &crate::ScenesWithAudio::new(&video.define_scenes()),
        &tb,
        &audio_map,
        |_| Ok(1.),
    )
    .unwrap();

    let resolved = audio_map
        .unwrap()
        .0
        .into_iter()
        .map(|track| (track.file, track.range))
        .collect::<Vec<_>>();

    assert_eq!(
        resolved,
        vec![
            (
                "audio1.mp3".to_string(),
                AudioTimelineSamples(0)..AudioTimelineSamples(10000)
            ),
            (
                "audio2.mp3".to_string(),
                AudioTimelineSamples(10000)..AudioTimelineSamples(20000)
            ),
            (
                "scene1.mp3".to_string(),
                AudioTimelineSamples(0)..AudioTimelineSamples(10000)
            ),
            (
                "scene2.mp3".to_string(),
                AudioTimelineSamples(20000)..AudioTimelineSamples(22000)
            ),
            (
                "scene1.mp3".to_string(),
                AudioTimelineSamples(30000)..AudioTimelineSamples(40000)
            ),
            (
                "scene2.mp3".to_string(),
                AudioTimelineSamples(50000)..AudioTimelineSamples(52000)
            )
        ]
    );
}

/// `include_media_dir!` mixes audio down to mono unless it gets the `stereo` option.
mod embedded_audio_channels {
    use crate::StaticMediaProvider;

    mod stereo {
        fframes_media_dir_macro::include_media_dir!(pub struct Media, "fframes/test-media/stereo", stereo);
    }
    mod mono {
        fframes_media_dir_macro::include_media_dir!(pub struct Media, "fframes/test-media/stereo");
    }

    fn channels(audio: &crate::AudioData) -> (Vec<f32>, Option<Vec<f32>>) {
        let crate::AudioData::Preloaded(data) = audio else {
            panic!("embedded audio is preloaded");
        };
        (
            data.samples.to_vec(),
            data.right.as_ref().map(|r| r.to_vec()),
        )
    }

    #[test]
    fn stereo_option_keeps_both_channels() {
        let media = stereo::Media::prepare().unwrap();
        let (left, right) = channels(&media.tone_wav);
        let right = right.expect("the right channel is embedded");
        assert_eq!(left.len(), right.len());
        // The fixture has a tone on the left and silence on the right.
        assert!(left.iter().any(|s| s.abs() > 0.1));
        assert!(right.iter().all(|s| s.abs() < 1e-3));
    }

    #[test]
    fn audio_is_mono_by_default() {
        let media = mono::Media::prepare().unwrap();
        let (mixed, right) = channels(&media.tone_wav);
        assert!(right.is_none());
        assert!(mixed.iter().any(|s| s.abs() > 0.05));
    }
}
