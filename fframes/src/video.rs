use std::future::Future;
use std::pin::Pin;

use crate::audio_map::AudioMap;
use crate::{fframes_context, frame, scenes::*, SceneInfo};

#[allow(dead_code)]
pub enum Duration<'a> {
    /// Get the duration from the audio file.
    FromAudio(&'a str),
    Seconds(usize),
    Frames(usize),
    Auto,
}

type AsyncAudioDurationCb =
    Box<dyn Fn(String) -> Pin<Box<dyn Future<Output = super::error::Result<usize>>>>>;

impl Duration<'_> {
    pub(super) async fn to_frames_async(
        &self,
        fps: usize,
        resolve_audio_duration: &AsyncAudioDurationCb,
    ) -> crate::error::Result<usize> {
        match self {
            Duration::FromAudio(audio) => resolve_audio_duration(audio.to_string()).await,
            Duration::Seconds(seconds) => Ok(seconds * fps),
            Duration::Frames(frames) => Ok(*frames),
            Duration::Auto => Err(crate::error::FFramesCoreError::MissingDurationOrScenes),
        }
    }

    pub(super) fn to_frames_sync<TGetAudioFn: Fn(&str) -> crate::error::Result<usize>>(
        &self,
        fps: usize,
        resolve_audio_duration: TGetAudioFn,
    ) -> crate::error::Result<usize> {
        match self {
            Duration::FromAudio(audio) => resolve_audio_duration(audio),
            Duration::Seconds(seconds) => Ok(seconds * fps),
            Duration::Frames(frames) => Ok(*frames),
            Duration::Auto => Err(crate::error::FFramesCoreError::MissingDurationOrScenes),
        }
    }
}

pub trait Video: Sync + Sized {
    const FPS: usize;
    const WIDTH: usize;
    const HEIGHT: usize;
    const DURATION: Duration<'static> = Duration::Auto;

    fn audio(&self) -> AudioMap;
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
    pub(crate) Vec<(std::ops::Range<usize>, SceneInfo, Box<dyn Scene>)>,
);

impl ResolvedScenesTimeline {
    pub fn find_relative_index(&self, frame_index: usize) -> Option<usize> {
        self.0
            .iter()
            .find(|(range, _, _)| range.contains(&frame_index))
            .map(|(range, _, _)| frame_index - range.start)
    }
}

// TODO figure out how to reuse. This function completely duplicates a sync version ot it.
#[allow(dead_code)]
pub async fn resolve_duration_and_scenes_async<TVideo: Video>(
    video: &TVideo,
    resolve_audio_duration: AsyncAudioDurationCb,
) -> crate::error::Result<(usize, Option<ResolvedScenesTimeline>)> {
    match (video.define_scenes().0, TVideo::DURATION) {
        (Some(scenes), Duration::Auto) => {
            let mut final_duration = 0;
            let mut resolved_scenes = Vec::new();
            let scenes_count = scenes.len();

            for (index, scene) in scenes.into_iter().enumerate() {
                let duration = scene
                    .duration()
                    .to_frames_async(TVideo::FPS, &resolve_audio_duration)
                    .await?;

                let (overlap_prev, overlap_next) = scene.overlap().to_frames(TVideo::FPS);
                resolved_scenes.push((
                    final_duration - overlap_prev..final_duration + duration + overlap_next,
                    SceneInfo {
                        index,
                        duration_in_frames: duration,
                        is_last: index == scenes_count - 1,
                    },
                    scene,
                ));

                final_duration += duration;
            }

            Ok((
                final_duration,
                Some(ResolvedScenesTimeline(resolved_scenes)),
            ))
        }
        (None, duration) => Ok((
            duration
                .to_frames_async(TVideo::FPS, &resolve_audio_duration)
                .await?,
            None,
        )),
        _ => Err(crate::error::FFramesCoreError::MissingDurationOrScenes),
    }
}

#[allow(dead_code)]
pub fn resolve_duration_and_scenes_sync<
    TGetAudioFn: Fn(&str) -> crate::error::Result<usize>,
    TVideo: Video,
>(
    video: &TVideo,
    resolve_audio_duration: TGetAudioFn,
) -> crate::error::Result<(usize, Option<ResolvedScenesTimeline>)> {
    match (video.define_scenes().0, TVideo::DURATION) {
        (Some(scenes), Duration::Auto) => {
            let mut final_duration = 0;
            let mut resolved_scenes = Vec::new();
            let scenes_count = scenes.len();

            for (index, scene) in scenes.into_iter().enumerate() {
                let duration = scene
                    .duration()
                    .to_frames_sync(TVideo::FPS, &resolve_audio_duration)?;

                let (overlap_prev, overlap_next) = scene.overlap().to_frames(TVideo::FPS);
                let start = if final_duration < overlap_prev {
                    0
                } else {
                    final_duration - overlap_prev
                };

                let (duration, is_overflow) = duration.overflowing_add(overlap_next);
                if is_overflow {
                    return Err(crate::error::FFramesCoreError::Overflow(
                        "scene duration".to_owned(),
                        duration,
                    ));
                }

                let (end, is_overflow) = final_duration.overflowing_add(duration);
                if is_overflow {
                    return Err(crate::error::FFramesCoreError::Overflow(
                        "scene duration".to_owned(),
                        end,
                    ));
                }

                resolved_scenes.push((
                    start..end,
                    SceneInfo {
                        index,
                        duration_in_frames: duration,
                        is_last: index == scenes_count - 1,
                    },
                    scene,
                ));

                final_duration += duration;
            }

            Ok((
                final_duration,
                Some(ResolvedScenesTimeline(resolved_scenes)),
            ))
        }
        (None, duration) => Ok((
            duration.to_frames_sync(TVideo::FPS, resolve_audio_duration)?,
            None,
        )),
        _ => Err(crate::error::FFramesCoreError::MissingDurationOrScenes),
    }
}
