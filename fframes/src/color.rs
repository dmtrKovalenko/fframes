use crate::Animatable;
use std::fmt;

/// The basic rgb color type. It is limited to the range of 0-255 because the most of video
/// codecs only support 8-bit color depth.  
/// Can be used in fframes animations and as a raw svgr! value.
///
/// @example
/// ```no_run
///use crate::fframes::{Color, Svgr, svgr};
///
///const WHITE: Color = Color::hex("#FFFFFF");
///
/// svgr!(<rect fill={WHITE} />);
/// ```
#[derive(Copy, Default, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

const HASH: u8 = b'#';

pub const fn char_to_digit(char: char, radix: u32) -> u32 {
    // If not a digit, a number greater than radix will be created.
    let mut digit = (char as u32).wrapping_sub('0' as u32);
    if radix > 10 {
        assert!(radix <= 36, "to_digit: radix is too high (maximum 36)");
        if digit < 10 {
            return digit;
        }

        // Force the 6th bit to be set to ensure ascii is lower case.
        digit = (char as u32 | 0b10_0000)
            .wrapping_sub('a' as u32)
            .saturating_add(10);
    }
    // FIXME: once then_some is const fn, use it here
    if digit < radix {
        digit
    } else {
        panic!("can not parse char in a radix string");
    }
}

impl Color {
    /// Creates a new color from a hex string. #RGB and #RRGGBB formats are supported.
    /// Available for const contexts.
    pub const fn hex(hex_str: &str) -> Self {
        let buffer = hex_str.as_bytes();

        if buffer[0] != HASH {
            panic!("hex color must start with #");
        }

        match buffer.len() {
            4 => {
                let r = char_to_digit(buffer[1] as char, 16) as u8;
                let g = char_to_digit(buffer[2] as char, 16) as u8;
                let b = char_to_digit(buffer[3] as char, 16) as u8;

                Color {
                    r: (r + r * 16),
                    g: (g + g * 16),
                    b: (b + b * 16),
                }
            }
            7 => {
                let r = char_to_digit(buffer[1] as char, 16) as u8;
                let r1 = char_to_digit(buffer[2] as char, 16) as u8;
                let g = char_to_digit(buffer[3] as char, 16) as u8;
                let g1 = char_to_digit(buffer[4] as char, 16) as u8;
                let b = char_to_digit(buffer[5] as char, 16) as u8;
                let b1 = char_to_digit(buffer[6] as char, 16) as u8;

                Color {
                    r: (r * 16 + r1),
                    g: (g * 16 + g1),
                    b: (b * 16 + b1),
                }
            }
            _ => panic!("Only #RGB or #RRGGBB hex formats are supported"),
        }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b }
    }
}

struct ColorF32 {
    r: f32,
    g: f32,
    b: f32,
}

impl From<Color> for ColorF32 {
    fn from(Color { r, g, b }: Color) -> Self {
        ColorF32 {
            r: r as f32,
            g: g as f32,
            b: b as f32,
        }
    }
}

impl Animatable for Color {
    fn apply_progress(&self, to: &Self, progress: f32) -> Self {
        let from = ColorF32::from(*self);
        let to = ColorF32::from(*to);

        Color {
            r: (from.r + ((to.r - from.r) * progress)) as u8,
            g: (from.g + ((to.g - from.g) * progress)) as u8,
            b: (from.b + ((to.b - from.b) * progress)) as u8,
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        let Color { r, g, b } = self;

        write!(formatter, "rgb({r}, {g}, {b})",)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    pub fn char_to_digit_test() {
        assert_eq!(char_to_digit('f', 16), 15);
        assert_eq!(char_to_digit('F', 16), 15);
        assert_eq!(char_to_digit('9', 16), 9);
    }

    #[test]
    pub fn hex_parsing() {
        assert_eq!(Color::hex("#f0f"), Color::rgb(255, 0, 255));
        assert_eq!(Color::hex("#ff00ff"), Color::rgb(255, 0, 255));
        assert_eq!(Color::hex("#ef4444"), Color::rgb(239, 68, 68));
        assert_eq!(Color::hex("#1c1917"), Color::rgb(28, 25, 23));
    }
}
