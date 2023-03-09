use crate::FFramesContext;
use std::{iter::FromIterator, ops::Range};

#[derive(Debug, Clone)]
pub enum AudioTimestamp {
    Frame(usize),
    Second(usize),
    /// Plays audio till the end of file
    Eof,
}

pub trait AudioTimelineUnit {
    fn from_frames(frames: usize, ctx: &FFramesContext) -> Self;
    fn as_usize(&self) -> usize;
    fn from_usize(val: usize) -> Self;
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

impl AudioTimestamp {
    pub fn to_seconds(&self, filename: &str, ctx: &FFramesContext) -> f32 {
        match self {
            AudioTimestamp::Frame(frame) => *frame as f32 * ctx.fps as f32,
            AudioTimestamp::Second(seconds) => *seconds as f32,
            AudioTimestamp::Eof => ctx.get_audio_data(filename).duration_in_seconds(),
        }
    }

    pub(crate) fn to_frames(&self, filename: &str, ctx: &FFramesContext) -> usize {
        match self {
            AudioTimestamp::Frame(frame) => *frame,
            AudioTimestamp::Second(seconds) => *seconds * ctx.fps,
            AudioTimestamp::Eof => {
                (ctx.get_audio_data(filename).duration_in_seconds() * ctx.fps as f32) as usize
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

type AudioDuration = (AudioTimestamp, AudioTimestamp);

#[derive(Debug, Clone)]
/// Audio map represents when and how long each audio file should be played within video or scene.
///
/// @example
/// ```rust
/// AudioMap::from([
///     (
///        "audio1.mp3",
///       (AudioTimestamp::Second(10), AudioTimestamp::Second(20)),
///     )
/// ])
/// ```
///
/// In this case audio1.mp3 will be playing starting from 10th second until the 20. The next timestamp represents duration.
pub struct AudioMap<'a>(pub Option<Vec<(&'a str, AudioDuration)>>);

/// The resolved audio_map contain each audio file position and duration in {1/{ctx.sample_rate}} units
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
    pub fn resolve<TUnit: AudioTimelineUnit>(
        &'a self,
        ctx: &FFramesContext,
    ) -> Option<ResolvedAudioMap<TUnit>> {
        let total_duration_units = TUnit::from_frames(ctx.duration_in_frames, ctx);

        let scenes_resolved_map = ctx.scenes.map(|scenes| {
            scenes
                .0
                .iter()
                .filter_map(|(range, scene_info, scene)| {
                    scene.audio_map(scene_info).0.map(|map| (range, map))
                })
                .flat_map(|(scene_range, map)| {
                    let scene_start_unit = TUnit::from_frames(scene_range.start, ctx).as_usize();

                    map.iter()
                        .map(|(f, (start_ts, end_ts))| {
                            let start_sample =
                                scene_start_unit + start_ts.to_unit::<TUnit>(f, ctx).as_usize();
                            let end_sample = scene_start_unit
                                + end_ts.to_unit::<TUnit>(f, ctx).as_usize()
                                + start_sample;

                            let end_sample = end_sample.min(total_duration_units.as_usize());
                            (
                                f.to_string(),
                                TUnit::from_usize(start_sample)..TUnit::from_usize(end_sample),
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        });

        let global_resolved_map = self.0.as_ref().map(|hash_map| {
            hash_map
                .iter()
                .map(|(f, (start_ts, end_ts))| {
                    let start_sample = start_ts.to_unit::<TUnit>(f, ctx).as_usize();
                    let end_sample = end_ts.to_unit::<TUnit>(f, ctx).as_usize() + start_sample;
                    let end_sample = end_sample.min(total_duration_units.as_usize());

                    (
                        f.to_string(),
                        TUnit::from_usize(start_sample)..TUnit::from_usize(end_sample),
                    )
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

impl<'a, const N: usize> From<[(&'a str, AudioDuration); N]> for AudioMap<'a> {
    fn from(arr: [(&'a str, AudioDuration); N]) -> Self {
        AudioMap(Some(arr.to_vec()))
    }
}

impl<'a> FromIterator<(&'a str, AudioDuration)> for AudioMap<'a> {
    fn from_iter<T: IntoIterator<Item = (&'a str, AudioDuration)>>(iter: T) -> Self {
        AudioMap(Some(iter.into_iter().collect()))
    }
}
