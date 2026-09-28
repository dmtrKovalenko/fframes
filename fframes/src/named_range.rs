#![allow(clippy::from_over_into)]
use crate::{AudioTimelineFrames, AudioTimelineUnit, ResolvedAudioMap, ResolvedScenesTimeline};

#[derive(Clone, serde::Serialize)]
pub struct NamedRange {
    pub name: String,
    pub start: usize,
    pub end: usize,
}

impl Into<NamedRange> for (String, std::ops::Range<usize>) {
    fn into(self) -> NamedRange {
        NamedRange {
            name: self.0,
            start: self.1.start,
            end: self.1.end,
        }
    }
}

impl Into<Vec<NamedRange>> for ResolvedAudioMap<AudioTimelineFrames> {
    fn into(self) -> Vec<NamedRange> {
        self.0
            .into_iter()
            .map(|track| NamedRange {
                name: track.file,
                start: track.range.start.as_usize(),
                end: track.range.end.as_usize(),
            })
            .collect::<Vec<_>>()
    }
}

impl Into<Vec<NamedRange>> for &ResolvedScenesTimeline<'_> {
    fn into(self) -> Vec<NamedRange> {
        self.iter()
            .map(|(range, _, scene)| NamedRange {
                name: scene.name().to_string(),
                start: range.start,
                end: range.end,
            })
            .collect::<Vec<_>>()
    }
}
