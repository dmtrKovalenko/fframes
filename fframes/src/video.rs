use crate::audio_map::AudioMap;
use crate::{
    fframes_context, frame, scenes::*, AudioTimelineFrames, AudioTimelineUnit, ResolvedAudioMap,
    SceneInfo, TimeBase,
};
use std::ops::{Add, Sub};
use std::rc::Rc;
use std::sync::Arc;

#[allow(dead_code)]
pub enum Duration<'a> {
    /// Get the duration from the audio file.
    FromAudio(&'a str),
    Seconds(f32),
    Frames(usize),
    /// The duration that will be inferred automatically either from scenes or audio map.
    /// If neither provided – rendering is not possible.
    Auto,
    __Add(Rc<(Duration<'a>, Duration<'a>)>),
    __Subtract(Rc<(Duration<'a>, Duration<'a>)>),
}

impl<'a> Add for Duration<'a> {
    type Output = Duration<'a>;

    fn add(self, rhs: Self) -> Self::Output {
        Self::__Add(Rc::new((self, rhs)))
    }
}

impl<'a> Sub for Duration<'a> {
    type Output = Duration<'a>;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::__Subtract(Rc::new((self, rhs)))
    }
}

impl<'a> Duration<'a> {
    pub fn used_audio_files(&self) -> Option<Vec<&str>> {
        match self {
            Duration::FromAudio(audio) => Some(vec![audio]),
            Duration::Seconds(_) => None,
            Duration::Frames(_) => None,
            Duration::Auto => None,
            Duration::__Subtract(alt) | Duration::__Add(alt) => {
                let (left, right) = alt.as_ref();
                let left = left.used_audio_files();
                let right = right.used_audio_files();

                match (left, right) {
                    (Some(mut left), Some(mut right)) => {
                        left.append(&mut right);
                        Some(left)
                    }
                    (Some(start), None) => Some(start),
                    (None, Some(end)) => Some(end),
                    (None, None) => None,
                }
            }
        }
    }

    pub(super) fn to_frames_async<TFun: Fn(&str) -> super::error::Result<usize>>(
        &'a self,
        fps: usize,
        related_audio_map: &AudioMap,
        resolve_audio_duration: &'a TFun,
    ) -> crate::error::Result<usize> {
        match self {
            Duration::FromAudio(audio) => resolve_audio_duration(audio),
            Duration::Seconds(seconds) => Ok((seconds * fps as f32) as usize),
            Duration::Frames(frames) => Ok(*frames),
            Duration::Auto => {
                let resolved_audio_map = related_audio_map
                    .resolve::<AudioTimelineFrames>(
                        AudioTimelineFrames::from_usize(0),
                        &crate::TimeBase {
                            fps,
                            sample_rate: 44100,
                        },
                        resolve_audio_duration,
                    )?
                    .ok_or(crate::error::FFramesCoreError::MissingDurationOrScenes)?;

                let max_frame = resolved_audio_map
                    .into_iter()
                    .map(|(_, range)| range.end.as_usize())
                    .max()
                    .ok_or(crate::error::FFramesCoreError::MissingDurationOrScenes)?;

                Ok(max_frame)
            }
            Duration::__Add(sum) => {
                let (left, right) = sum.as_ref();

                let left = left.to_frames_async(fps, related_audio_map, resolve_audio_duration)?;
                let right =
                    right.to_frames_async(fps, related_audio_map, resolve_audio_duration)?;
                Ok(left + right)
            }
            Duration::__Subtract(sub) => {
                let (left, right) = sub.as_ref();

                let left = left.to_frames_async(fps, related_audio_map, resolve_audio_duration)?;
                let right =
                    right.to_frames_async(fps, related_audio_map, resolve_audio_duration)?;

                Ok(left + right)
            }
        }
    }
}

pub trait Video: Sync + Sized {
    const FPS: usize;
    const WIDTH: usize;
    const HEIGHT: usize;

    fn audio(&self) -> AudioMap;
    fn duration(&self) -> Duration;

    fn define_scenes(&self) -> Scenes {
        Scenes(None)
    }

    fn render_frame(
        &self,
        frame: frame::Frame,
        ctx: &fframes_context::FFramesContext,
    ) -> crate::Svgr;
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
