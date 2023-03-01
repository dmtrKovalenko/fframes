use crate::FFramesContext;
use std::ops::Range;

#[derive(Debug, Clone)]
pub enum AudioTimestamp {
    Frame(usize),
    Second(usize),
    /// Plays audio till the end of file
    Eof,
}

#[derive(Debug, Clone, Copy)]
pub enum ResolvedAudioUnit {
    Frames,
    Samples,
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

    pub(crate) fn to_samples(&self, filename: &str, ctx: &FFramesContext) -> usize {
        match self {
            AudioTimestamp::Frame(frame) => frame * ctx.sample_rate / ctx.fps,
            AudioTimestamp::Second(seconds) => *seconds * ctx.sample_rate,
            AudioTimestamp::Eof => {
                (ctx.get_audio_data(filename).duration_in_seconds() * ctx.sample_rate as f32)
                    as usize
            }
        }
    }

    pub(crate) fn to_unit(
        &self,
        filename: &str,
        unit: ResolvedAudioUnit,
        ctx: &FFramesContext,
    ) -> usize {
        match unit {
            ResolvedAudioUnit::Frames => self.to_frames(filename, ctx),
            ResolvedAudioUnit::Samples => self.to_samples(filename, ctx),
        }
    }
}

type AudioDuration = (AudioTimestamp, AudioTimestamp);

pub struct AudioMap<'a>(pub Option<Vec<(&'a str, AudioDuration)>>);

/// The resolved audio_map contain each audio file position and duration in {1/{ctx.sample_rate}} units
pub struct ResolvedAudioMap(ResolvedAudioUnit, pub Vec<(String, Range<usize>)>);

impl ResolvedAudioMap {
    pub fn calc_stream_duration_in_samples(&self) -> usize {
        self.1
            .iter()
            .map(|(_, range)| (range.start + range.end))
            .max()
            .unwrap_or(0)
    }
}

impl<'a> AudioMap<'a> {
    pub fn resolve(
        &'a self,
        unit: ResolvedAudioUnit,
        ctx: &FFramesContext,
    ) -> Option<ResolvedAudioMap> {
        let scenes_resolved_map = ctx.scenes.map(|scenes| {
            scenes
                .0
                .iter()
                .filter_map(|(range, _, scene)| scene.audio_map().0.map(|map| (range, map)))
                .flat_map(|(scene_range, map)| {
                    let scene_start_sample = scene_range.start * ctx.sample_rate / ctx.fps;
                    let scene_end_sample = scene_range.start * ctx.sample_rate / ctx.fps;

                    map.iter()
                        .map(|(f, (start_ts, end_ts))| {
                            let start_sample = scene_start_sample + start_ts.to_unit(f, unit, ctx);
                            let end_sample =
                                scene_end_sample + end_ts.to_unit(f, unit, ctx) + start_sample;

                            (f.to_string(), start_sample..end_sample)
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        });

        let global_resolved_map = self.0.as_ref().map(|hash_map| {
            hash_map
                .iter()
                .map(|(f, (start_ts, end_ts))| {
                    let start_sample = start_ts.to_unit(f, unit, ctx);
                    let end_sample = end_ts.to_unit(f, unit, ctx) + start_sample;

                    (f.to_string(), start_sample..end_sample)
                })
                .collect::<Vec<_>>()
        });

        match (scenes_resolved_map, global_resolved_map) {
            (Some(scenes_resolved_map), Some(mut global_resolved_map)) => {
                global_resolved_map.extend(scenes_resolved_map);
                Some(ResolvedAudioMap(unit, global_resolved_map))
            }
            (Some(scene_maps), None) => Some(ResolvedAudioMap(unit, scene_maps)),
            (None, Some(global_maps)) => Some(ResolvedAudioMap(unit, global_maps)),
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
