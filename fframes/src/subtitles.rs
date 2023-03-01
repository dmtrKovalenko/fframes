use crate::frame::Frame;
use std::{fs, path::Path, str::FromStr};
use webvtt_parser::{self, parse_vtt, Vtt, VttError};

pub type SubtitlesError = VttError;

#[derive(Debug, Clone)]
pub struct Subtitles {
    pub(crate) subtitles: Vtt,
}

impl Subtitles {
    pub fn get_phrases_count(&self) -> usize {
        self.subtitles.cues.len()
    }
}

impl FromStr for Subtitles {
    type Err = VttError;

    fn from_str(content: &str) -> Result<Subtitles, VttError> {
        parse_vtt(content).map(|subtitles| Subtitles { subtitles })
    }
}

impl Subtitles {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, VttError> {
        let file = fs::read_to_string(path).unwrap();
        Self::from_str(file.as_str())
    }

    pub fn get_phrase_for_frame(&self, frame: &Frame) -> Option<&str> {
        let milliseconds = (frame.get_current_second() * 1000.0) as u64;

        self.subtitles.cues.iter().rev().find_map(|cue| {
            (milliseconds >= cue.start.0 && milliseconds <= cue.end.0).then_some(cue.text.as_str())
        })
    }
}
