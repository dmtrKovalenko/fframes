use crate::error::{FFramesMediaError, Result};
use std::error::Error;
pub use webvtt_parser::{self, Vtt, VttCue, VttError};

pub type SubtitlesError = FileVttParsingError;

#[derive(Debug, Clone)]
/// Represents any of supported specific to the format cue attributes
pub enum CueVariant<'a> {
    Vtt(&'a VttCue<'a>),
}

pub struct Cue<'a> {
    pub index: usize,
    pub variant: CueVariant<'a>,
}

impl<'a> Cue<'a> {
    pub fn text(&self) -> &'a str {
        match self.variant {
            CueVariant::Vtt(cue) => cue.text,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Subtitles<'a> {
    subtitles: Vtt<'a>,
}

impl<'a> Subtitles<'a> {
    pub fn cues_count(&self) -> usize {
        self.subtitles.cues.len()
    }

    pub fn parse(content: &'a str) -> Result<Subtitles<'a>> {
        Vtt::parse(content)
            .map(|subtitles| Subtitles { subtitles })
            .map_err(FFramesMediaError::from)
    }
}

fn validate_cue_fitting_frame(frame_milliseconds: u64, cue: &VttCue) -> bool {
    frame_milliseconds >= cue.start.as_milliseconds()
        && frame_milliseconds <= cue.end.as_milliseconds()
}

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

impl<'a> Subtitles<'a> {
    /// Returns the latest cue which timestamp range fits current frame.
    /// Cue includes the text and additional metadata available in the vtt file.
    pub fn get_cue_by_time(&'a self, milliseconds: u64) -> Option<Cue<'a>> {
        self.subtitles
            .cues
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, cue)| {
                validate_cue_fitting_frame(milliseconds, cue).then_some(Cue {
                    index,
                    variant: CueVariant::Vtt(cue),
                })
            })
    }

    pub fn get_cue_stack(&'a self, milliseconds: u64, overlap: u64) -> Vec<Cue<'a>> {
        let (joining_milliseconds, is_overflowed) = milliseconds.overflowing_add(overlap);
        let joining_milliseconds = if is_overflowed {
            0
        } else {
            joining_milliseconds
        };

        self.subtitles
            .cues
            .iter()
            .take_while(|cue| cue.start.as_milliseconds() < joining_milliseconds)
            .enumerate()
            .map(|(index, cue)| Cue {
                index,
                variant: CueVariant::Vtt(cue),
            })
            .collect()
    }

    /// Returns the text of all the cues within subtitles file
    pub fn get_whole_text(&self) -> Vec<&str> {
        self.subtitles
            .cues
            .iter()
            .map(|cue| cue.text)
            .collect::<Vec<&str>>()
    }
}
