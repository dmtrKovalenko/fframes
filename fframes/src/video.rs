use crate::audio_map::AudioMap;
use crate::error::Result;
use crate::{
    scenes::*, AudioTimelineUnit, Duration, FFramesContext, Frame, ResolvedAudioMap, SceneInfo,
    Svgr, TimeBase,
};

/// The base fframes video trait. It represents how to render a video for a struct which becomes an
/// input of the video.
pub trait Video: Sync + Sized {
    const FPS: usize;
    const WIDTH: usize;
    const HEIGHT: usize;
    fn duration(&self) -> Duration;
    fn audio(&self) -> AudioMap;

    /// Defines the scenes timeline of the video.
    /// Each scene is an dyn object which implements the `Scene` trait.
    ///
    /// Every scene must be either bound to the `&self` lifetime or be a zero sized type.
    /// In short: put your scenes to the `&self` or do not add any fields to the scene struct.
    ///
    /// # Example
    /// ```rust
    /// use fframes::{Video, Scenes, Scene, Frame, Svgr, FFramesContext};
    ///
    /// struct SceneZeroSize;
    /// struct SceneWithInput {
    ///    value: String;
    /// }
    ///
    /// impl Scene For SceneZeroSize { ... }
    /// impl Scene For SceneWithInput { ... }
    ///
    /// struct MyVideo {
    ///     scene_with_input: SceneWithInput,
    /// };
    ///
    /// impl Video for MyVideo {
    ///     fn define_scenes(&self) -> Scenes {
    ///         let scenes: Vec<&dyn Scene> = vec![
    ///             // notice this is a zero sized type so we can create ref right here
    ///             &SceneZeroSize { },
    ///             // And here we passing a ref bound to the &self
    ///             &self.scene_with_input,
    ///         ]
    ///         
    ///         Scenes::from(scenes)
    ///     }
    /// }
    /// ```
    fn define_scenes(&self) -> Scenes {
        Scenes(None)
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a>;
}

#[derive(Debug)]
pub struct ResolvedScenesTimeline<'a> {
    pub total_scenes_duration: usize,
    pub(crate) timeline: Vec<(std::ops::Range<usize>, SceneInfo, &'a (dyn Scene + 'a))>,
}

impl<'a> ResolvedScenesTimeline<'a> {
    pub(crate) fn iter(
        &'a self,
    ) -> impl Iterator<Item = &(std::ops::Range<usize>, SceneInfo, &'a (dyn Scene + 'a))> {
        self.timeline.iter()
    }

    fn from_scenes(
        time_base: &TimeBase,
        scenes: &[SceneWithAudio<'a>],
        resolve_audio_duration: &impl Fn(&str) -> super::error::Result<usize>,
    ) -> Result<Self> {
        let scenes_count = scenes.len();
        let mut final_duration = 0;
        let mut resolved_scenes = Vec::new();

        for (index, SceneWithAudio { scene, audio_map }) in scenes.iter().enumerate() {
            let duration = scene.duration().to_frames_async(
                time_base.fps,
                audio_map,
                &resolve_audio_duration,
            )?;

            let (overlap_prev, overlap_next) = scene.overlap().to_frames(time_base.fps);
            let start_frame = final_duration - overlap_prev;
            let end_frame = final_duration + duration + overlap_next;
            resolved_scenes.push((
                final_duration - overlap_prev..final_duration + duration + overlap_next,
                SceneInfo {
                    index,
                    start_frame,
                    end_frame,
                    total_scenes_in_video: scenes_count,
                    duration_in_frames: duration + overlap_next,
                    is_last: index == scenes_count - 1,
                },
                *scene,
            ));

            final_duration += duration;
        }

        Ok(ResolvedScenesTimeline {
            total_scenes_duration: final_duration,
            timeline: resolved_scenes,
        })
    }
}

pub struct ResolvedRenderingTimeline<'a, TAudioUnit: AudioTimelineUnit + std::fmt::Debug> {
    pub audio_map: Option<ResolvedAudioMap<TAudioUnit>>,
    pub scenes: Option<ResolvedScenesTimeline<'a>>,
    pub duration_in_frames: usize,
}

pub fn resolve_timeline<
    'a,
    TAudioUnit: AudioTimelineUnit + std::fmt::Debug + Copy,
    TFun: Fn(&str) -> super::error::Result<usize>,
>(
    duration: &Duration,
    scenes: &ScenesWithAudio<'a>,
    time_base: &TimeBase,
    top_level_audio_map: &AudioMap,
    resolve_audio_duration: TFun,
) -> Result<ResolvedRenderingTimeline<'a, TAudioUnit>> {
    let (duration, resolved_scenes) = match (scenes.0.as_deref(), duration) {
        (Some(scenes), Duration::Auto) => {
            let timeline =
                ResolvedScenesTimeline::from_scenes(time_base, scenes, &resolve_audio_duration)?;

            (timeline.total_scenes_duration, Some(timeline))
        }
        // if both duration and scenes provided we use duration
        (Some(scenes), duration) => {
            let timeline =
                ResolvedScenesTimeline::from_scenes(time_base, scenes, &resolve_audio_duration)?;
            let total_duration = duration.to_frames_async(
                time_base.fps,
                top_level_audio_map,
                &resolve_audio_duration,
            )?;

            (total_duration, Some(timeline))
        }
        (None, duration) => {
            let total_duration = duration.to_frames_async(
                time_base.fps,
                top_level_audio_map,
                &resolve_audio_duration,
            )?;

            (total_duration, None)
        }
    };

    let mut resolved_audio_map = top_level_audio_map.resolve_with_scenes::<TAudioUnit>(
        resolved_scenes.as_ref(),
        time_base,
        &resolve_audio_duration,
    )?;

    if let Some(resolved_audio_map) = resolved_audio_map.as_mut() {
        resolved_audio_map.round_max_duration(TAudioUnit::from_frames(duration, time_base));
    };

    Ok(ResolvedRenderingTimeline {
        audio_map: resolved_audio_map,
        scenes: resolved_scenes,
        duration_in_frames: duration,
    })
}
