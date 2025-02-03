use fframes::animation::{Easing, KeyFrame, KeyFramesAnimation};

use crate::constants::*;

#[derive(Debug)]
pub struct Chapter<'a> {
    pub start: &'a str,
    pub title: &'a str,
}

impl Chapter<'_> {
    pub fn get_y_position(index: usize) -> u64 {
        CHAPTERS_START_Y + (index as u64 * (CHAPTER_HEIGHT + CHAPTER_PADDING))
    }
}

const CHAPTER_EASING: Easing = Easing::Spring {
    mass: 0.45,
    stiffness: 130.0,
    damping: 20.0,
};

pub(crate) fn create_transition_keyframes_for_chapters(
    chapters: &[Chapter],
) -> KeyFramesAnimation<f32> {
    let mut current_position = 0u64;
    KeyFramesAnimation::new(
        chapters
            .iter()
            .enumerate()
            .map(|(index, chapter)| {
                let start = parse_duration_to_seconds(chapter.start) as f32;
                let new_position = Chapter::get_y_position(index) - CHAPTERS_START_Y;

                let keyframe = KeyFrame {
                    start,
                    from: current_position as f32,
                    to: new_position as f32,
                    easing: &CHAPTER_EASING,
                };

                current_position = new_position;
                keyframe
            })
            .collect(),
    )
}

/// PANICS in case of parsing error but it's okay because the idea
/// of the project is to render it from cli and not to be used in production,
/// do the normal error handling if you deploy this somewhere
fn parse_duration_to_seconds(time_str: &str) -> u64 {
    let parts: Vec<&str> = time_str.split(':').collect();
    match parts.len() {
        // mm:ss
        2 => {
            let minutes = parts[0].parse::<u64>().expect("Failed to parse minutes");
            let seconds = parts[1].parse::<u64>().expect("FAiled to parse seconds");

            minutes * 60 + seconds
        }
        // hh:mm:ss
        3 => {
            let hours = parts[0].parse::<u64>().expect("Failed to parse hours");
            let minutes = parts[1].parse::<u64>().expect("Failed to parse minutes");
            let seconds = parts[2].parse::<u64>().expect("Failed to parse seconds");

            hours * 3600 + minutes * 60 + seconds
        }
        _ => panic!("Invalid time format string, expected either HH:MM:SS or MM:SS"),
    }
}
