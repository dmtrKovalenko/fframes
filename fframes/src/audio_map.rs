use crate::FFramesContext;
use std::{
    iter::FromIterator,
    ops::{Add, Range, Sub},
    rc::Rc,
};

#[derive(Debug, Clone)]
pub enum AudioTimestamp<'a> {
    /// A flat frame within a video.
    Frame(usize),
    /// A second within a video timeline.
    Second(f32),
    /// It is a **very specific** timestamp value. When used as end of audio range will be decoded as a whole duration of audio + start timestamp.
    /// So in case of range like `Second(10)..Eof` audio will be played from second 10 to the end of audio file.
    Eof,
    /// Represents a flat duration of audio file. It is different from `Eof` in a way that it will always be decoded as a whole duration of audio file.
    /// So in case of range like `Second(10)..DurationOfAudio("audio20seconds.mp3")`, where audio20seconds's duration is 20 seconds, audio will be played only **10 seconds**.
    /// Because `DurationOfAudio` will always be resolved to 20 seconds.
    DurationOfAudio(&'a str),
    /// Represents a sum of two timestamps. Can be constructed using `+` operator.
    Add(Rc<(AudioTimestamp<'a>, AudioTimestamp<'a>)>),
    /// Represents a subtraction of two timestamps. Can be constructed using `-` operator.
    Subtract(Rc<(AudioTimestamp<'a>, AudioTimestamp<'a>)>),
}

impl<'a> Add for AudioTimestamp<'a> {
    type Output = AudioTimestamp<'a>;

    fn add(self, rhs: Self) -> Self::Output {
        Self::Add(Rc::new((self, rhs)))
    }
}

impl<'a> Sub for AudioTimestamp<'a> {
    type Output = AudioTimestamp<'a>;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::Subtract(Rc::new((self, rhs)))
    }
}

pub trait AudioTimelineUnit {
    fn from_frames(frames: usize, ctx: &FFramesContext) -> Self;
    fn from_usize(val: usize) -> Self;
    fn as_usize(&self) -> usize;
}

#[derive(PartialEq, PartialOrd, Debug)]
pub struct AudioTimelineFrames(usize);

impl AudioTimelineUnit for AudioTimelineFrames {
    fn from_frames(frames: usize, _ctx: &FFramesContext) -> Self {
        AudioTimelineFrames(frames)
    }

    fn as_usize(&self) -> usize {
        self.0
    }
    fn from_usize(val: usize) -> Self {
        AudioTimelineFrames(val)
    }
}

#[derive(PartialEq, PartialOrd, Debug)]
pub struct AudioTimelineSamples(pub(crate) usize);

impl AudioTimelineUnit for AudioTimelineSamples {
    fn from_frames(frames: usize, ctx: &FFramesContext) -> Self {
        AudioTimelineSamples(frames * ctx.sample_rate / ctx.fps)
    }
    fn as_usize(&self) -> usize {
        self.0
    }
    fn from_usize(val: usize) -> Self {
        AudioTimelineSamples(val)
    }
}

impl AudioTimestamp<'_> {
    pub fn to_seconds(&self, filename: &str, ctx: &FFramesContext) -> f32 {
        match self {
            AudioTimestamp::Frame(frame) => *frame as f32 * ctx.fps as f32,
            AudioTimestamp::Second(seconds) => *seconds,
            AudioTimestamp::Eof => ctx.get_audio_data(filename).duration_in_seconds(),
            AudioTimestamp::DurationOfAudio(filename) => {
                ctx.get_audio_data(filename).duration_in_seconds()
            }
            AudioTimestamp::Add(add) => {
                let (a, b) = &**add;
                a.to_seconds(filename, ctx) + b.to_seconds(filename, ctx)
            }
            AudioTimestamp::Subtract(add) => {
                let (a, b) = &**add;
                a.to_seconds(filename, ctx) - b.to_seconds(filename, ctx)
            }
        }
    }

    pub(crate) fn to_frames(&self, filename: &str, ctx: &FFramesContext) -> usize {
        match self {
            AudioTimestamp::Frame(frame) => *frame,
            AudioTimestamp::Second(seconds) => (*seconds * ctx.fps as f32) as usize,
            AudioTimestamp::Eof => {
                (ctx.get_audio_data(filename).duration_in_seconds() * ctx.fps as f32) as usize
            }
            AudioTimestamp::DurationOfAudio(filename) => {
                (ctx.get_audio_data(filename).duration_in_seconds() * ctx.fps as f32) as usize
            }
            AudioTimestamp::Add(add) => {
                let (a, b) = &**add;
                a.to_frames(filename, ctx) + b.to_frames(filename, ctx)
            }
            AudioTimestamp::Subtract(add) => {
                let (a, b) = &**add;
                a.to_frames(filename, ctx) - b.to_frames(filename, ctx)
            }
        }
    }

    pub(crate) fn to_unit<TUnit: AudioTimelineUnit>(
        &self,
        filename: &str,
        ctx: &FFramesContext,
    ) -> TUnit {
        TUnit::from_frames(self.to_frames(filename, ctx), ctx)
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

/// The resolved audio_map contain each audio file position and duration in specified units.
#[derive(Debug)]
pub struct ResolvedAudioMap<TUnit: AudioTimelineUnit>(pub Vec<(String, Range<TUnit>)>);

impl ResolvedAudioMap<AudioTimelineSamples> {
    pub fn calc_stream_duration_in_samples(&self) -> usize {
        self.0
            .iter()
            .map(|(_, range)| (range.start.0 + range.end.0))
            .max()
            .unwrap_or(0)
    }
}

impl<'a> AudioMap<'a> {
    pub fn resolve<TUnit: AudioTimelineUnit + std::fmt::Debug>(
        &'a self,
        ctx: &FFramesContext,
    ) -> Option<ResolvedAudioMap<TUnit>> {
        let total_duration_units = TUnit::from_frames(ctx.duration_in_frames, ctx);
        let global_resolved_map = self
            .0
            .as_ref()
            .map(|map| resolve_map(map, 0, ctx, &total_duration_units));

        let scenes_resolved_map = ctx.scenes.map(|scenes| {
            scenes
                .0
                .iter()
                .filter_map(|(range, scene_info, scene)| {
                    scene.audio_map(scene_info).0.map(|map| (range, map))
                })
                .flat_map(|(scene_range, map)| {
                    let scene_start_unit = TUnit::from_frames(scene_range.start, ctx).as_usize();

                    resolve_map(&map, scene_start_unit, ctx, &total_duration_units)
                })
                .collect::<Vec<_>>()
        });

        match (scenes_resolved_map, global_resolved_map) {
            (Some(scenes_resolved_map), Some(mut global_resolved_map)) => {
                global_resolved_map.extend(scenes_resolved_map);
                Some(ResolvedAudioMap(global_resolved_map))
            }
            (Some(scene_maps), None) => Some(ResolvedAudioMap(scene_maps)),
            (None, Some(global_maps)) => Some(ResolvedAudioMap(global_maps)),
            (None, None) => None,
        }
    }

    pub fn none() -> Self {
        AudioMap(None)
    }
}

fn resolve_map<'a, TUnit: AudioTimelineUnit + std::fmt::Debug>(
    map: &[(&'a str, AudioDuration<'a>)],
    offset: usize,
    ctx: &FFramesContext,
    total_duration_units: &TUnit,
) -> Vec<(String, Range<TUnit>)> {
    map.iter()
        .map(|(f, range)| {
            let start_sample = offset + range.start.to_unit::<TUnit>(f, ctx).as_usize();
            let mut end_sample = offset + range.end.to_unit::<TUnit>(f, ctx).as_usize();

            if matches!(range.end, AudioTimestamp::Eof) {
                end_sample += start_sample
            };

            let end_sample = end_sample.min(total_duration_units.as_usize());

            (
                f.to_string(),
                TUnit::from_usize(start_sample)..TUnit::from_usize(end_sample),
            )
        })
        .collect::<Vec<_>>()
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
