use crate::audio_map::AudioMap;
use crate::{
    scenes::*, AudioTimelineUnit, Duration, FFramesContext, Frame, ResolvedAudioMap, SceneInfo,
    TimeBase,
};
use std::sync::Arc;

pub trait Video: Sync + Sized {
    const FPS: usize;
    const WIDTH: usize;
    const HEIGHT: usize;

    fn audio(&self) -> AudioMap;
    fn duration(&self) -> Duration;

    fn define_scenes(&self) -> Scenes {
        Scenes(None)
    }

    fn render_frame(&self, frame: Frame, ctx: &FFramesContext) -> crate::Svgr;
}

#[derive(Debug)]
pub struct ResolvedScenesTimeline(
    pub(crate) Vec<(std::ops::Range<usize>, SceneInfo, Arc<dyn Scene>)>,
);

impl ResolvedScenesTimeline {
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = &(std::ops::Range<usize>, SceneInfo, Arc<dyn Scene>)> {
        self.0.iter()
    }
}

pub struct ResolvedRenderingTimeline<TAudioUnit: AudioTimelineUnit + std::fmt::Debug> {
    pub audio_map: Option<ResolvedAudioMap<TAudioUnit>>,
    pub scenes: Option<ResolvedScenesTimeline>,
    pub duration_in_frames: usize,
}

pub fn resolve_timeline<
    TAudioUnit: AudioTimelineUnit + std::fmt::Debug + Copy,
    TFun: Fn(&str) -> super::error::Result<usize>,
>(
    duration: &Duration,
    scenes: &ScenesWithAudio,
    time_base: &TimeBase,
    top_level_audio_map: &AudioMap,
    resolve_audio_duration: TFun,
) -> crate::error::Result<ResolvedRenderingTimeline<TAudioUnit>> {
    let scenes_count = scenes.len();

    let (duration, resolved_scenes) = match (scenes.0.as_deref(), duration) {
        (Some(scenes_iter), Duration::Auto) => {
            let mut final_duration = 0;
            let mut resolved_scenes = Vec::new();

            for (index, SceneWithAudio { scene, audio_map }) in scenes_iter.iter().enumerate() {
                let duration = scene.duration().to_frames_async(
                    time_base.fps,
                    audio_map,
                    &resolve_audio_duration,
                )?;

                let (overlap_prev, overlap_next) = scene.overlap().to_frames(time_base.fps);
                resolved_scenes.push((
                    final_duration - overlap_prev..final_duration + duration + overlap_next,
                    SceneInfo {
                        index,
                        total_scenes_in_video: scenes_count,
                        duration_in_frames: duration + overlap_next,
                        is_last: index == scenes_count - 1,
                    },
                    Arc::clone(scene),
                ));

                final_duration += duration;
            }

            Ok((
                final_duration,
                Some(ResolvedScenesTimeline(resolved_scenes)),
            ))
        }
        (None, duration) => Ok((
            duration.to_frames_async(
                time_base.fps,
                top_level_audio_map,
                &resolve_audio_duration,
            )?,
            None,
        )),
        _ => Err(crate::error::FFramesCoreError::MissingDurationOrScenes),
    }?;

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
