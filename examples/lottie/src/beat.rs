//! The clock of the video. The music runs at 120 BPM, so one beat is 30 frames. Every cut, pop
//! and keystroke is on this beat grid. `soundtrack.rs` reads the same constants, so the sound
//! follows the picture.

use fframes::Frame;

pub const FPS: usize = 60;
/// One beat in seconds (120 BPM).
pub const BEAT: f32 = 0.5;
pub const FRAMES_PER_BEAT: usize = 30;

/// First beat of each scene.
pub const HOOK: f32 = 0.0;
pub const GRID: f32 = 8.0;
pub const CODE: f32 = 16.0;
pub const ZOOM: f32 = 24.0;
pub const OUTRO: f32 = 28.0;
pub const END: f32 = 36.0;

/// The hook is complete before 1.5 s. "NOW PLAYS" comes on beat 1, with its words 50 ms apart.
/// The letters of "LOTTIE" come on beat 2, 30 ms apart.
pub const WORD_NOW: f32 = 1.0;
pub const WORD_PLAYS: f32 = 1.1;
pub const LETTERS: f32 = 2.0;
pub const LETTER_STEP: f32 = 0.06;

/// The tiles of the grid pop on sixteenth notes (125 ms apart).
pub const TILE_POP: f32 = GRID + 0.5;
pub const TILE_STEP: f32 = 0.25;
pub const TILES: usize = 12;

/// The code types from `TYPE_START`, `CHARS_PER_BEAT` characters per beat.
pub const TYPE_START: f32 = CODE + 0.5;
pub const CHARS_PER_BEAT: f32 = 30.0;
/// "one call." comes after the code and stays 2 s.
pub const CAPTION: f32 = CODE + 4.0;

/// The outro types the `features = ["lottie"]` line from this beat.
pub const FEATURE_TYPE_START: f32 = OUTRO + 1.0;
pub const FEATURE_CHARS_PER_BEAT: f32 = 16.0;

pub fn beat_time(beat: f32) -> f32 {
    beat * BEAT
}

pub fn beat_frames(beats: f32) -> usize {
    (beats * FRAMES_PER_BEAT as f32).round() as usize
}

/// Position on the beat grid of the whole video.
pub fn global_beat(frame: &Frame) -> f32 {
    frame.global_index as f32 / FRAMES_PER_BEAT as f32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Plain,
    Punct,
    Method,
    Str,
    Type,
    Key,
}

/// The code of the code scene, line by line. Leading spaces are the indentation.
pub const CODE_LINES: &[&[(&str, Token)]] = &[
    &[
        ("frame", Token::Plain),
        (".", Token::Punct),
        ("play_lottie", Token::Method),
        ("(", Token::Punct),
        ("ctx", Token::Plain),
        (", ", Token::Punct),
        ("\"tiger.json\"", Token::Str),
        (",", Token::Punct),
    ],
    &[
        ("    ", Token::Plain),
        ("LottieOptions", Token::Type),
        (" {", Token::Punct),
    ],
    &[
        ("        ", Token::Plain),
        ("looping", Token::Plain),
        (": ", Token::Punct),
        ("true", Token::Key),
        (",", Token::Punct),
    ],
    &[
        ("        ", Token::Plain),
        ("..", Token::Punct),
        ("Default", Token::Type),
        ("::", Token::Punct),
        ("default", Token::Method),
        ("()", Token::Punct),
    ],
    &[("    ", Token::Plain), ("})", Token::Punct)],
];

/// Characters typed in the code scene, indentation excluded.
pub fn code_chars() -> usize {
    CODE_LINES
        .iter()
        .flat_map(|line| line.iter().enumerate())
        .map(|(column, (text, _))| {
            if column == 0 {
                text.trim_start().len()
            } else {
                text.len()
            }
        })
        .sum()
}

pub const FEATURE_LINE: &str = "features = [\"lottie\"]";

/// The beats of the keystrokes, for the typing sound.
pub fn key_beats() -> Vec<f32> {
    let code = (0..code_chars()).map(|i| TYPE_START + i as f32 / CHARS_PER_BEAT);
    let feature =
        (0..FEATURE_LINE.len()).map(|i| FEATURE_TYPE_START + i as f32 / FEATURE_CHARS_PER_BEAT);
    code.chain(feature).collect()
}
