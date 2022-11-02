use crate::FFramesContext;
use std::ops::Range;

#[derive(Debug, Clone)]
pub enum AudioTimestamp {
    Frame(usize),
    Second(usize),
    /// Plays audio till the end of file
    Eof,
}

impl AudioTimestamp {
    pub fn to_seconds(&self, filename: &str, ctx: &FFramesContext) -> f32 {
        match self {
            AudioTimestamp::Frame(frame) => *frame as f32 * ctx.fps as f32,
            AudioTimestamp::Second(seconds) => *seconds as f32,
            AudioTimestamp::Eof => ctx.get_audio_data(filename).duration_in_seconds(),
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
}

type AudioDuration = (AudioTimestamp, AudioTimestamp);

pub struct AudioMap(pub Option<Vec<(&'static str, AudioDuration)>>);

/// The resolved audio_map contain each audio file position and duration in {1/{ctx.sample_rate}} units
pub struct ResolvedAudioMap(pub Vec<(&'static str, Range<usize>)>);

impl ResolvedAudioMap {
    pub fn calc_stream_duration_in_samples(&self) -> usize {
        self.0
            .iter()
            .map(|(_, range)| (range.start + range.end))
            .max()
            .unwrap_or(0)
    }
}

impl AudioMap {
    pub fn resolve(&self, ctx: &FFramesContext) -> Option<ResolvedAudioMap> {
        self.0
            .as_ref()
            .map(|hash_map| {
                hash_map
                    .iter()
                    .map(|(f, (start_ts, end_ts))| {
                        let start_sample = start_ts.to_samples(f, ctx);
                        let end_sample = end_ts.to_samples(f, ctx) + start_sample;

                        (*f, start_sample..end_sample)
                    })
                    .collect::<Vec<_>>()
            })
            .map(ResolvedAudioMap)
    }

    pub fn none() -> Self {
        AudioMap(None)
    }
}

impl<const N: usize> From<[(&'static str, AudioDuration); N]> for AudioMap {
    fn from(arr: [(&'static str, AudioDuration); N]) -> Self {
        AudioMap(Some(arr.to_vec()))
    }
}
