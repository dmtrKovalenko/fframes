use crate::audio_map::AudioMap;
use crate::{fframes_context, frame, scenes::*, SceneInfo};
use std::future::Future;
use std::ops::{Add, Sub};
use std::pin::Pin;
use std::rc::Rc;

#[allow(dead_code)]
pub enum Duration<'a> {
    /// Get the duration from the audio file.
    FromAudio(&'a str),
    Seconds(f32),
    Frames(usize),
    Auto,
    Add(Rc<(Duration<'a>, Duration<'a>)>),
    Subtract(Rc<(Duration<'a>, Duration<'a>)>),
}

impl<'a> Add for Duration<'a> {
    type Output = Duration<'a>;

    fn add(self, rhs: Self) -> Self::Output {
        Self::Add(Rc::new((self, rhs)))
    }
}

impl<'a> Sub for Duration<'a> {
    type Output = Duration<'a>;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::Subtract(Rc::new((self, rhs)))
    }
}

impl<'a> Duration<'a> {
    pub(super) fn to_frames_async<
        TFun: Fn(String) -> Pin<Box<dyn Future<Output = super::error::Result<usize>>>>,
    >(
        &'a self,
        fps: usize,
        resolve_audio_duration: &'a TFun,
    ) -> Pin<Box<dyn Future<Output = crate::error::Result<usize>> + 'a>> {
        Box::pin(async move {
            match self {
                Duration::FromAudio(audio) => resolve_audio_duration(audio.to_string()).await,
                Duration::Seconds(seconds) => Ok((seconds * fps as f32) as usize),
                Duration::Frames(frames) => Ok(*frames),
                Duration::Auto => Err(crate::error::FFramesCoreError::MissingDurationOrScenes),
                Duration::Add(sum) => {
                    let (left, right) = sum.as_ref();

                    let left = left.to_frames_async(fps, resolve_audio_duration).await?;
                    let right = right.to_frames_async(fps, resolve_audio_duration).await?;
                    Ok(left + right)
                }
                Duration::Subtract(sub) => {
                    let (left, right) = sub.as_ref();

                    let left = left.to_frames_async(fps, resolve_audio_duration).await?;
                    let right = right.to_frames_async(fps, resolve_audio_duration).await?;
                    Ok(left + right)
                }
            }
        })
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

#[allow(dead_code)]
pub async fn resolve_duration_and_scenes_async<
    TVideo: Video,
    TFun: Fn(String) -> Pin<Box<dyn Future<Output = super::error::Result<usize>>>>,
>(
    video: &TVideo,
    resolve_audio_duration: TFun,
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
                        total_scenes_in_video: scenes_count,
                        duration_in_frames: duration + overlap_next,
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
