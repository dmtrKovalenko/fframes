use crate::{error, FFramesContext, ResolvedScenesTimeline, ScenesWithAudio, TimeBase};
use std::{
    iter::FromIterator,
    ops::{Add, Range, Sub},
    rc::Rc,
};

#[derive(Debug, Clone)]
pub enum AudioTimestamp<'a> {
    /// It is a **very specific** timestamp value. When used as end of audio range will be decoded as a whole duration of audio + start timestamp.
    /// So in case of range like `Second(10)..Eof` audio will be played from second 10 to the end of audio file.
    Eof,
    /// A flat frame within a video.
    Frame(usize),
    /// A second within a video timeline.
    Second(f32),
    /// Represents a flat duration of audio file. It is different from `Eof` in a way that it will always be decoded as a whole duration of audio file.
    /// So in case of range like `Second(10)..DurationOfAudio("audio20seconds.mp3")`, where audio20seconds's duration is 20 seconds, audio will be played only **10 seconds**.
    /// Because `DurationOfAudio` will always be resolved to 20 seconds.
    DurationOfAudio(&'a str),
    /// Represents a sum of two timestamps. Can be constructed using `+` operator.
    __Add(Rc<(AudioTimestamp<'a>, AudioTimestamp<'a>)>),
    /// Represents a subtraction of two timestamps. Can be constructed using `-` operator.
    __Subtract(Rc<(AudioTimestamp<'a>, AudioTimestamp<'a>)>),
}

impl<'a> Add for AudioTimestamp<'a> {
    type Output = AudioTimestamp<'a>;

    fn add(self, rhs: Self) -> Self::Output {
        Self::__Add(Rc::new((self, rhs)))
    }
}

impl<'a> Sub for AudioTimestamp<'a> {
    type Output = AudioTimestamp<'a>;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::__Subtract(Rc::new((self, rhs)))
    }
}

pub trait AudioTimelineUnit {
    fn from_frames(frames: usize, tb: &TimeBase) -> Self;
    fn from_usize(val: usize) -> Self;
    fn as_usize(&self) -> usize;
    fn to_frames(&self, tb: &TimeBase) -> usize;
}

#[derive(PartialEq, PartialOrd, Debug, Copy, Clone)]
pub struct AudioTimelineFrames(usize);

impl AudioTimelineUnit for AudioTimelineFrames {
    fn from_frames(frames: usize, _: &TimeBase) -> Self {
        AudioTimelineFrames(frames)
    }
    fn as_usize(&self) -> usize {
        self.0
    }
    fn from_usize(val: usize) -> Self {
        AudioTimelineFrames(val)
    }
    fn to_frames(&self, _: &TimeBase) -> usize {
        self.0
    }
}

#[derive(PartialEq, PartialOrd, Debug, Clone, Copy)]
pub struct AudioTimelineSamples(pub(crate) usize);

impl AudioTimelineUnit for AudioTimelineSamples {
    fn from_frames(frames: usize, tb: &TimeBase) -> Self {
        AudioTimelineSamples(frames * tb.sample_rate / tb.fps)
    }
    fn as_usize(&self) -> usize {
        self.0
    }
    fn from_usize(val: usize) -> Self {
        AudioTimelineSamples(val)
    }
    fn to_frames(&self, tb: &TimeBase) -> usize {
        self.0 * tb.fps / tb.sample_rate
    }
}

impl AudioTimestamp<'_> {
    fn infer_relying_on_dynamic_duration_audio_files<'a>(
        &self,
        name: &'a str,
    ) -> Option<Vec<&'a str>> {
        match self {
            AudioTimestamp::Eof => Some(vec![name]),
            AudioTimestamp::DurationOfAudio(_) => Some(vec![name]),
            AudioTimestamp::__Subtract(add) | AudioTimestamp::__Add(add) => {
                let (a, b) = &**add;
                let a = a.infer_relying_on_dynamic_duration_audio_files(name);
                let b = b.infer_relying_on_dynamic_duration_audio_files(name);

                match (a, b) {
                    (Some(mut a), Some(mut b)) => {
                        a.append(&mut b);
                        Some(a)
                    }
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                }
            }
            AudioTimestamp::Frame(_) => None,
            AudioTimestamp::Second(_) => None,
        }
    }

    pub(crate) fn to_frames(
        &self,
        filename: &str,
        tb: &TimeBase,
        eof_offset: Option<usize>,
        resolve_audio_duration_in_frames: &impl Fn(&str) -> error::Result<usize>,
    ) -> error::Result<usize> {
        Ok(match self {
            AudioTimestamp::Frame(frame) => *frame,
            AudioTimestamp::Second(seconds) => (*seconds * tb.fps as f32) as usize,
            AudioTimestamp::Eof => {
                resolve_audio_duration_in_frames(filename)? + eof_offset.unwrap_or(0)
            }
            AudioTimestamp::DurationOfAudio(filename) => {
                resolve_audio_duration_in_frames(filename)?
            }
            AudioTimestamp::__Add(add) => {
                let (a, b) = &**add;
                a.to_frames(filename, tb, eof_offset, resolve_audio_duration_in_frames)?
                    + b.to_frames(filename, tb, eof_offset, resolve_audio_duration_in_frames)?
            }
            AudioTimestamp::__Subtract(add) => {
                let (a, b) = &**add;
                a.to_frames(filename, tb, eof_offset, resolve_audio_duration_in_frames)?
                    - b.to_frames(filename, tb, eof_offset, resolve_audio_duration_in_frames)?
            }
        })
    }

    pub(crate) fn to_unit<TUnit: AudioTimelineUnit>(
        &self,
        filename: &str,
        tb: &TimeBase,
        eof_offset: Option<usize>,
        resolve_audio_duration_in_frames: impl Fn(&str) -> error::Result<usize>,
    ) -> error::Result<TUnit> {
        let frames = self.to_frames(filename, tb, eof_offset, &resolve_audio_duration_in_frames)?;

        Ok(TUnit::from_frames(frames, tb))
    }
}

type AudioDuration<'a> = Range<AudioTimestamp<'a>>;

#[derive(Debug, Clone)]
/// Audio map represents when and how long each audio file should be played within video or scene.
///
/// @example
/// ```rust
/// use fframes::AudioTimestamp::*;
///
/// AudioMap::from([
///     (
///        "audio1.mp3",
///        Second(10)..Second(20)
///     )
///     (
///        "audio2.mp3",
///        Second(5)..Eof
///     )
/// ])
/// ```
///
/// In this case audio 1 will be playing from second 10 to second 20 and audio 2 will be playing from second 5 to the end of file.
pub struct AudioMap<'a>(pub Option<Vec<(&'a str, AudioDuration<'a>)>>);

type AudioTimeline<TUnit> = Vec<(String, Range<TUnit>)>;

/// The resolved audio_map contain each audio file position and duration in specified units.
#[derive(Debug)]
pub struct ResolvedAudioMap<TUnit: AudioTimelineUnit>(pub(crate) AudioTimeline<TUnit>);

impl<TUnit: AudioTimelineUnit + Copy> ResolvedAudioMap<TUnit> {
    pub(crate) fn round_max_duration(&mut self, max_duration: TUnit) {
        let max_duration_usize = max_duration.as_usize();

        for (_, range) in self.0.iter_mut() {
            if range.end.as_usize() > max_duration_usize {
                range.end = max_duration;
            }
        }
    }

    pub fn calc_stream_duration(&self) -> usize {
        self.0
            .iter()
            .map(|(_, range)| (range.start.as_usize() + range.end.as_usize()))
            .max()
            .unwrap_or(0)
    }
}

impl<'a> AudioMap<'a> {
    /// Creates a new empty audio map. It means that no audio files will be played within the video/scene.
    /// If scene or video duration that defines this audio map uses `fframes::Duration::FromAudioMap` – rendering won't be possible.
    pub fn none() -> Self {
        AudioMap(None)
    }

    pub fn unstable_flatten_with_scenes(self, scene_audios: &'a ScenesWithAudio) -> Self {
        if self.0.is_none() && scene_audios.0.is_none() {
            return Self(None);
        }

        let mut map = self.0.unwrap_or_default();

        if let Some(scenes) = scene_audios.0.as_ref() {
            for audio_map in scenes.iter() {
                if let Some(scene_audio_map) = audio_map.audio_map.0.as_ref() {
                    map.extend(scene_audio_map.iter().cloned())
                }
            }
        }

        Self(Some(map))
    }

    pub fn used_audio_files(&'a self) -> Option<Vec<&'a str>> {
        self.0.as_ref().map(|map| {
            map.iter()
                .filter_map(|(filename, range)| {
                    let start = range
                        .start
                        .infer_relying_on_dynamic_duration_audio_files(filename);

                    let end = range
                        .end
                        .infer_relying_on_dynamic_duration_audio_files(filename);

                    match (start, end) {
                        (Some(mut start), Some(mut end)) => {
                            start.append(&mut end);
                            Some(start)
                        }
                        (Some(start), None) => Some(start),
                        (None, Some(end)) => Some(end),
                        (None, None) => None,
                    }
                })
                .flatten()
                .collect()
        })
    }

    pub(crate) fn resolve<TUnit: AudioTimelineUnit + std::fmt::Debug>(
        &self,
        offset: TUnit,
        tb: &TimeBase,
        resolve_audio_duration_in_frames: impl Fn(&str) -> error::Result<usize>,
    ) -> error::Result<Option<AudioTimeline<TUnit>>> {
        let offset = offset.as_usize();
        self.0
            .as_ref()
            .map(|file_durations| {
                file_durations
                    .iter()
                    .map(|(filename, range)| {
                        let start_unit = range.start.to_unit::<TUnit>(
                            filename,
                            tb,
                            None,
                            &resolve_audio_duration_in_frames,
                        )?;

                        let end_unit = range.end.to_unit::<TUnit>(
                            filename,
                            tb,
                            Some(start_unit.to_frames(tb)),
                            &resolve_audio_duration_in_frames,
                        )?;

                        Ok((
                            filename.to_string(),
                            TUnit::from_usize(start_unit.as_usize() + offset)
                                ..TUnit::from_usize(end_unit.as_usize() + offset),
                        ))
                    })
                    .collect::<error::Result<Vec<_>>>()
            })
            .transpose()
    }

    pub fn resolve_with_scenes<TUnit: AudioTimelineUnit + std::fmt::Debug>(
        &'a self,
        scenes: Option<&ResolvedScenesTimeline>,
        tb: &TimeBase,
        resolve_audio_duration_in_frames: impl Fn(&str) -> error::Result<usize>,
    ) -> error::Result<Option<ResolvedAudioMap<TUnit>>> {
        let global_resolved_map =
            self.resolve(TUnit::from_usize(0), tb, &resolve_audio_duration_in_frames)?;

        let scenes_resolved_map = scenes
            .map(|scenes| -> crate::error::Result<_> {
                Ok(scenes
                    .0
                    .iter()
                    .map(|(range, _, scene)| {
                        scene.audio_map().resolve(
                            TUnit::from_frames(range.start, tb),
                            tb,
                            &resolve_audio_duration_in_frames,
                        )
                    })
                    .collect::<error::Result<Vec<_>>>()?
                    .into_iter()
                    .flatten()
                    .flatten()
                    .collect::<Vec<_>>())
            })
            .transpose()?;

        Ok(match (scenes_resolved_map, global_resolved_map) {
            (Some(scenes_resolved_map), Some(mut global_resolved_map)) => {
                global_resolved_map.extend(scenes_resolved_map);
                Some(ResolvedAudioMap(global_resolved_map))
            }
            (Some(scene_maps), None) => Some(ResolvedAudioMap(scene_maps)),
            (None, Some(global_maps)) => Some(ResolvedAudioMap(global_maps)),
            (None, None) => None,
        })
    }

    pub fn resolve_with_ctx<TUnit: AudioTimelineUnit + std::fmt::Debug>(
        &'a self,
        tb: &FFramesContext,
    ) -> error::Result<Option<ResolvedAudioMap<TUnit>>> {
        self.resolve_with_scenes(tb.scenes, &tb.time_base, |filename| {
            Ok(tb.get_audio_data(filename).duration_in_frames(tb))
        })
    }
}

impl<'a, const N: usize> From<[(&'a str, AudioDuration<'a>); N]> for AudioMap<'a> {
    fn from(arr: [(&'a str, AudioDuration<'a>); N]) -> Self {
        AudioMap(Some(arr.to_vec()))
    }
}

impl<'a> FromIterator<(&'a str, AudioDuration<'a>)> for AudioMap<'a> {
    fn from_iter<T: IntoIterator<Item = (&'a str, AudioDuration<'a>)>>(iter: T) -> Self {
        AudioMap(Some(iter.into_iter().collect()))
    }
}
