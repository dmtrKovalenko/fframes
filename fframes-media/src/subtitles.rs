//! Subtitle data and cue lookup for `WebVTT` and ordinary timed-word JSON.
//!
//! [`Subtitles::parse`] detects the input format from its content. Both formats
//! share owned, validated cue data and the same media-loading and frame APIs.
//! Metadata decodes lazily on first typed access and is cached by application type.
//! Legacy frame helpers keep inclusive cue ends; [`CaptionTrack::active_cues`]
//! and the new captions component use half-open `[start, end)` intervals.

use std::fmt::Debug;
pub use webvtt_parser::{self, Vtt, VttCue, VttError};
use webvtt_parser::{OwnedVtt, OwnedVttCue};

pub use crate::subtitles_data::{
    CaptionCue, CaptionError, CaptionMetadata, CaptionTrack, CaptionWord, WordState,
};

/// Compatibility name for [`CaptionTrack`], the owned VTT/JSON subtitle model.
pub type Subtitles = CaptionTrack;
/// Compatibility name for subtitle loading errors.
pub type SubtitlesError = CaptionError;

pub trait FFramesSubtitlesCue<'a> {
    fn text(&'a self) -> &'a str;
}

/// Legacy subtitle lookup used by frame phrase/cue helpers. Cue ends are inclusive;
/// cue stacks include only cues starting strictly before `milliseconds + overlap`.
pub trait FFramesSubtitles<'a> {
    type Cue: FFramesSubtitlesCue<'a> + Debug;

    fn cues_count(&self) -> usize;
    fn get_cue_by_time(&self, milliseconds: u64) -> Option<(usize, &Self::Cue)>;
    fn get_cue_stack(&self, milliseconds: u64, overlap: u64) -> Vec<&Self::Cue>;
    fn get_whole_text(&self) -> Vec<&str>;

    fn validate_cue_fitting_frame(milliseconds: u64, cue: &Self::Cue) -> bool;
}

impl FFramesSubtitlesCue<'_> for VttCue<'_> {
    fn text(&self) -> &str {
        self.text
    }
}

impl<'a> FFramesSubtitles<'a> for Vtt<'a> {
    type Cue = VttCue<'a>;

    fn cues_count(&self) -> usize {
        self.cues.len()
    }

    fn get_cue_by_time(&self, milliseconds: u64) -> Option<(usize, &Self::Cue)> {
        self.cues.iter().enumerate().rev().find_map(|(index, cue)| {
            Self::validate_cue_fitting_frame(milliseconds, cue).then_some((index, cue))
        })
    }

    fn get_cue_stack(&self, milliseconds: u64, overlap: u64) -> Vec<&Self::Cue> {
        let (joining_milliseconds, is_overflowed) = milliseconds.overflowing_add(overlap);
        let joining_milliseconds = if is_overflowed {
            0
        } else {
            joining_milliseconds
        };

        self.cues
            .iter()
            .take_while(|cue| cue.start.as_milliseconds() < joining_milliseconds)
            .collect()
    }

    fn get_whole_text(&self) -> Vec<&str> {
        self.cues.iter().map(|cue| cue.text).collect::<Vec<&str>>()
    }

    fn validate_cue_fitting_frame(frame_milliseconds: u64, cue: &Self::Cue) -> bool {
        frame_milliseconds >= cue.start.as_milliseconds()
            && frame_milliseconds <= cue.end.as_milliseconds()
    }
}

impl FFramesSubtitlesCue<'_> for OwnedVttCue {
    fn text(&self) -> &str {
        self.text.as_str()
    }
}

impl FFramesSubtitles<'_> for OwnedVtt {
    type Cue = OwnedVttCue;

    fn cues_count(&self) -> usize {
        self.cues.len()
    }

    fn get_cue_by_time(&self, milliseconds: u64) -> Option<(usize, &Self::Cue)> {
        self.cues.iter().enumerate().rev().find_map(|(index, cue)| {
            Self::validate_cue_fitting_frame(milliseconds, cue).then_some((index, cue))
        })
    }

    fn get_cue_stack(&self, milliseconds: u64, overlap: u64) -> Vec<&Self::Cue> {
        let (joining_milliseconds, is_overflowed) = milliseconds.overflowing_add(overlap);
        let joining_milliseconds = if is_overflowed {
            0
        } else {
            joining_milliseconds
        };

        self.cues
            .iter()
            .take_while(|cue| cue.start.as_milliseconds() < joining_milliseconds)
            .collect()
    }

    fn get_whole_text(&self) -> Vec<&str> {
        self.cues
            .iter()
            .map(|cue| cue.text.as_str())
            .collect::<Vec<&str>>()
    }

    fn validate_cue_fitting_frame(frame_milliseconds: u64, cue: &Self::Cue) -> bool {
        frame_milliseconds >= cue.start.as_milliseconds()
            && frame_milliseconds <= cue.end.as_milliseconds()
    }
}

impl FFramesSubtitlesCue<'_> for CaptionCue {
    fn text(&self) -> &str {
        CaptionCue::phrase(self)
    }
}

impl FFramesSubtitles<'_> for CaptionTrack {
    type Cue = CaptionCue;

    fn cues_count(&self) -> usize {
        self.cues().len()
    }

    fn get_cue_by_time(&self, milliseconds: u64) -> Option<(usize, &Self::Cue)> {
        let seconds = milliseconds as f64 / 1000.0;
        self.cues()
            .iter()
            .enumerate()
            .rev()
            .find(|(_, cue)| cue.start <= seconds && seconds <= cue.end)
    }

    fn get_cue_stack(&self, milliseconds: u64, overlap: u64) -> Vec<&Self::Cue> {
        let joining_seconds = milliseconds.checked_add(overlap).unwrap_or(0) as f64 / 1000.0;
        self.cues()
            .iter()
            .take_while(|cue| cue.start < joining_seconds)
            .collect()
    }

    fn get_whole_text(&self) -> Vec<&str> {
        self.cues().iter().map(CaptionCue::phrase).collect()
    }

    fn validate_cue_fitting_frame(milliseconds: u64, cue: &Self::Cue) -> bool {
        let seconds = milliseconds as f64 / 1000.0;
        cue.start <= seconds && seconds <= cue.end
    }
}
