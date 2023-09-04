use std::{error::Error, fmt::Debug};
pub use webvtt_parser::{self, Vtt, VttCue, VttError};
use webvtt_parser::{OwnedVtt, OwnedVttCue};

pub type SubtitlesError = FileVttParsingError;

pub trait FFramesSubtitlesCue<'a> {
    fn text(&'a self) -> &'a str;
}

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

impl<'a> FFramesSubtitles<'a> for OwnedVtt {
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

// A not for myself in the future:
// This is a huge pain in the ass for WASM because we have no way to depend on some lifetime coming
// from outside. So for now simply substitute the types so editor always using OwnedVtt version.
// Likely need to replace with enum or Cow<'static, Vtt> like alternative.
//
#[cfg(not(target_arch = "wasm32"))]
pub type Subtitles<'a> = Vtt<'a>;

#[cfg(target_arch = "wasm32")]
pub type Subtitles<'a> = OwnedVtt;

#[derive(Debug)]
pub enum FileVttParsingError {
    IO(std::io::Error),
    Vtt(VttError),
}

impl std::fmt::Display for FileVttParsingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileVttParsingError::IO(err) => write!(f, "Failed to read vtt file: {}", err),
            FileVttParsingError::Vtt(error) => write!(f, "Failed to parse vtt file: {}", error),
        }
    }
}

impl Error for FileVttParsingError {}
