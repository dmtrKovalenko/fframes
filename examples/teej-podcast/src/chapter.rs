use fframes::animation::{Easing, KeyFrame, KeyFramesAnimation};

use crate::constants::*;

#[derive(Debug)]
pub struct Chapter<'a> {
    pub start: &'a str,
    pub start_seconds: u64,
    pub title: &'a str,
}

impl<'a> Chapter<'a> {
    pub fn new(title: &'a str, start: &'a str) -> Chapter<'a> {
        Chapter {
            title,
            start,
            start_seconds: parse_duration_to_seconds(start),
        }
    }

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
                let start = chapter.start_seconds as f32;
                let new_position = Chapter::get_y_position(index) - CHAPTERS_START_Y;

                let keyframe = KeyFrame {
                    start,
                    // The actual duration of animation will be automatically calculated
                    // from the spring easing function, so just add some value for timeline.
                    end: Some(start + 1.0),
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

pub fn parse_duration_to_seconds(time_str: &str) -> u64 {
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
