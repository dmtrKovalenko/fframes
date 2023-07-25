use crate::Frame;
use std::{fs, path::Path};
use webvtt_parser::{self, parse_vtt, Vtt, VttCue, VttError};

pub type SubtitlesError = VttError;

#[derive(Debug, Clone)]
/// Represents any of supported specific to the format cue attributes
pub enum CueVariant<'a> {
    Vtt(&'a VttCue),
}

pub struct Cue<'a> {
    pub index: usize,
    pub variant: CueVariant<'a>,
}

impl<'a> Cue<'a> {
    pub fn text(&self) -> &'a str {
        match self.variant {
            CueVariant::Vtt(cue) => cue.text.as_str(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Subtitles {
    pub start_frame: Option<usize>,
    pub end_frame: Option<usize>,
    subtitles: Vtt,
}

impl Subtitles {
    pub fn cues_count(&self) -> usize {
        self.subtitles.cues.len()
    }

    pub fn parse(content: &str, fps: usize) -> Result<Subtitles, VttError> {
        parse_vtt(content).map(|subtitles| Subtitles {
            start_frame: subtitles
                .cues
                .get(0)
                .map(|cue| ((cue.start.as_milliseconds() as f64 / 1000.) * fps as f64) as usize),
            end_frame: subtitles
                .cues
                .last()
                .map(|cue| ((cue.start.as_milliseconds() as f64 / 1000.) * fps as f64) as usize),

            subtitles,
        })
    }
}

fn validate_cue_fitting_frame(frame_milliseconds: u64, cue: &VttCue) -> bool {
    frame_milliseconds >= cue.start.as_milliseconds()
        && frame_milliseconds <= cue.end.as_milliseconds()
}

impl Subtitles {
    /// Parses subtitles from vtt file path.
    pub fn from_file<P: AsRef<Path>>(path: P, fps: usize) -> Result<Self, VttError> {
        let file = fs::read_to_string(path).unwrap();
        Self::parse(file.as_str(), fps)
    }

    /// Returns the latest cue which timestamp range fits current frame.
    /// Cue includes the text and additional metadata available in the vtt file.
    pub fn get_cue_for_frame(&self, frame: &Frame) -> Option<Cue> {
        let milliseconds = (frame.get_current_second() * 1000.0) as u64;

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

    /// The same as get_cue_for_frame but also returns the index of the matched cue.
    pub fn get_cue_with_index_for_frame(&self, frame: &Frame) -> Option<(usize, &VttCue)> {
        let milliseconds = (frame.get_current_second() * 1000.0) as u64;

        self.subtitles
            .cues
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, cue)| {
                validate_cue_fitting_frame(milliseconds, cue).then_some((index, cue))
            })
    }

    /// Returns the text of the cue which timestamp range fits current frame.
    pub fn get_phrase_for_frame(&self, frame: &Frame) -> Option<&str> {
        self.get_cue_for_frame(frame).map(|cue| cue.text())
    }

    /// Returns all the cues in order which timestamp is before or equal current frame.
    ///
    /// ### Params
    /// * `overlap` – value in milliseconds is used to control when the next cue will join the stack,
    /// e.g if overlap is 1000ms, the next cue will join the stack when it's timestamp is 1000ms or less
    /// then the time of the current frame.
    pub fn get_cue_stack(&self, frame: &Frame, overlap: u64) -> Vec<&VttCue> {
        let milliseconds = (frame.get_current_second() * 1000.0) as u64;
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
            .collect()
    }

    /// Returns the text of all the cues within subtitles file
    pub fn get_whole_text(&self) -> Vec<&str> {
        self.subtitles
            .cues
            .iter()
            .map(|cue| cue.text.as_str())
            .collect::<Vec<&str>>()
    }
}
