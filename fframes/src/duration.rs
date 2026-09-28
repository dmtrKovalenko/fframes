use crate::AudioTimelineFrames;
use crate::AudioTimelineUnit;
use crate::audio_map::AudioMap;
use std::ops::Add;
use std::ops::Sub;
use std::rc::Rc;

#[derive(Debug)]
pub enum Duration<'a> {
    /// Resolves duration from the audio file, the string is the file name in the media folder.
    FromAudio(&'a str),
    /// Resolves duration from the video file metadata.
    FromVideo(&'a str),
    /// Duration in seconds
    Seconds(f32),
    /// Duration in pure frames, not recommended to use because the value must keep in sync with FPS.
    Frames(usize),
    /// The duration that will be inferred automatically either from scenes or audio map.
    /// If neither provided – rendering is not possible.
    Auto,
    /// Sum of two durations.
    /// Do not use directly, use the `+` operator instead.
    __Add(Rc<(Duration<'a>, Duration<'a>)>),
    /// Subtraction of two durations.
    /// Do not use directly, use the `-` operator instead.
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
    /// Audio files this duration is resolved from.
    pub fn used_audio_files(&self) -> Option<Vec<&'a str>> {
        self.used_files(|duration| match duration {
            Duration::FromAudio(audio) => Some(audio),
            _ => None,
        })
    }

    /// Video files whose metadata this duration is resolved from.
    pub fn used_video_files(&self) -> Option<Vec<&'a str>> {
        self.used_files(|duration| match duration {
            Duration::FromVideo(video) => Some(video),
            _ => None,
        })
    }

    fn used_files(
        &self,
        pick: impl Fn(&Duration<'a>) -> Option<&'a str> + Copy,
    ) -> Option<Vec<&'a str>> {
        match self {
            Duration::__Subtract(alt) | Duration::__Add(alt) => {
                let (left, right) = alt.as_ref();
                match (left.used_files(pick), right.used_files(pick)) {
                    (Some(mut left), Some(mut right)) => {
                        left.append(&mut right);
                        Some(left)
                    }
                    (Some(start), None) => Some(start),
                    (None, Some(end)) => Some(end),
                    (None, None) => None,
                }
            }
            duration => pick(duration).map(|file| vec![file]),
        }
    }

    /// `resolve_audio_duration` returns the duration of a media file in seconds.
    pub(super) fn to_frames_async<TFun: Fn(&str) -> super::error::Result<f64>>(
        &'a self,
        fps: usize,
        related_audio_map: &AudioMap,
        resolve_audio_duration: &'a TFun,
    ) -> crate::error::Result<usize> {
        match self {
            Duration::FromAudio(audio) | Duration::FromVideo(audio) => Ok(
                crate::audio_map::seconds_to_frames_floor(resolve_audio_duration(audio)?, fps),
            ),
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
                    .ok_or(crate::error::FFramesError::MissingDurationOrScenes)?;

                let max_frame = resolved_audio_map
                    .into_iter()
                    .map(|track| track.range.end.as_usize())
                    .max()
                    .ok_or(crate::error::FFramesError::MissingDurationOrScenes)?;

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

                Ok(left.saturating_sub(right))
            }
        }
    }
}
