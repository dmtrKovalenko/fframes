use std::fmt;
use usvgr::svgtree::SvgAttributeValue;

use crate::animation;

/// The basic rgba color type. It is limited to the range of 0-255 because the most of video
/// codecs only support 8-bit color depth.  
///
/// Can be used in fframes animations and as a raw svgr! value.
///
/// @example
/// ```no_run
///use crate::fframes::{Color, Svgr, svgr};
///
///const WHITE: Color = Color::hex("#FFFFFF");
///const SEMI_TRANSPARENT_RED: Color = Color::rgba(255, 0, 0, 128);
///
/// svgr!(<rect fill={WHITE} />);
/// ```
#[derive(Copy, Default, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl From<Color> for usvgr::svgtree::svgrtypes::Color {
    fn from(color: Color) -> Self {
        usvgr::svgtree::svgrtypes::Color::new_rgba(color.r, color.g, color.b, color.a)
    }
}

impl From<Color> for SvgAttributeValue<'static> {
    fn from(color: Color) -> Self {
        SvgAttributeValue::Color(color.into())
    }
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

    if digit < radix {
        digit
    } else {
        panic!(
            "Failed to parse expression as a hex color string. Char should be valid for radix 16 (0-9, a-f)",
        );
    }
}

impl Color {
    pub const BLACK: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    pub const WHITE: Color = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    /// The standard green chroma key color used in video production.
    /// See <https://en.wikipedia.org/wiki/Chroma_key>
    pub const CHROMA_KEY: Color = Color {
        r: 0,
        g: 177,
        b: 64,
        a: 0,
    };

    /// Creates new color from hex number
    pub const fn hex_num(num: u32) -> Self {
        if num <= 0xfff {
            // Handle 3-digit hex (RGB)
            let r = ((num >> 8) & 0xF) as u8;
            let g = ((num >> 4) & 0xF) as u8;
            let b = (num & 0xF) as u8;
            Color {
                r: (r * 16 + r),
                g: (g * 16 + g),
                b: (b * 16 + b),
                a: 255,
            }
        } else if num <= 0xffff {
            // Handle 4-digit hex (RGBA)
            let r = ((num >> 12) & 0xF) as u8;
            let g = ((num >> 8) & 0xF) as u8;
            let b = ((num >> 4) & 0xF) as u8;
            let a = (num & 0xF) as u8;
            Color {
                r: (r * 16 + r),
                g: (g * 16 + g),
                b: (b * 16 + b),
                a: (a * 16 + a),
            }
        } else if num <= 0xffffff {
            // Handle 6-digit hex (RRGGBB)
            Color {
                r: ((num >> 16) & 0xFF) as u8,
                g: ((num >> 8) & 0xFF) as u8,
                b: (num & 0xFF) as u8,
                a: 255,
            }
        } else {
            // Handle 8-digit hex (RRGGBBAA)
            Color {
                r: ((num >> 24) & 0xFF) as u8,
                g: ((num >> 16) & 0xFF) as u8,
                b: ((num >> 8) & 0xFF) as u8,
                a: (num & 0xFF) as u8,
            }
        }
    }

    /// Creates a new color from a hex string. Supports #RGB, #RGBA, #RRGGBB, and #RRGGBBAA formats.
    /// Works as a constant, so it's available for **const variables** and avoids runtime parsing.
    /// If parsing fails, it fallbacks to the solid black color.
    pub const fn hex(hex_str: &str) -> Self {
        let buffer = hex_str.as_bytes();

        if buffer[0] != HASH {
            return Color::BLACK;
        }

        match buffer.len() {
            4 => {
                // #RGB
                let r = char_to_digit(buffer[1] as char, 16) as u8;
                let g = char_to_digit(buffer[2] as char, 16) as u8;
                let b = char_to_digit(buffer[3] as char, 16) as u8;

                Color {
                    r: (r + r * 16),
                    g: (g + g * 16),
                    b: (b + b * 16),
                    a: 255,
                }
            }
            5 => {
                // #RGBA
                let r = char_to_digit(buffer[1] as char, 16) as u8;
                let g = char_to_digit(buffer[2] as char, 16) as u8;
                let b = char_to_digit(buffer[3] as char, 16) as u8;
                let a = char_to_digit(buffer[4] as char, 16) as u8;

                Color {
                    r: (r + r * 16),
                    g: (g + g * 16),
                    b: (b + b * 16),
                    a: (a + a * 16),
                }
            }
            7 => {
                // #RRGGBB
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
                    a: 255,
                }
            }
            9 => {
                // #RRGGBBAA
                let r = char_to_digit(buffer[1] as char, 16) as u8;
                let r1 = char_to_digit(buffer[2] as char, 16) as u8;
                let g = char_to_digit(buffer[3] as char, 16) as u8;
                let g1 = char_to_digit(buffer[4] as char, 16) as u8;
                let b = char_to_digit(buffer[5] as char, 16) as u8;
                let b1 = char_to_digit(buffer[6] as char, 16) as u8;
                let a = char_to_digit(buffer[7] as char, 16) as u8;
                let a1 = char_to_digit(buffer[8] as char, 16) as u8;

                Color {
                    r: (r * 16 + r1),
                    g: (g * 16 + g1),
                    b: (b * 16 + b1),
                    a: (a * 16 + a1),
                }
            }
            _ => Color::BLACK,
        }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Color { r, g, b, a: 255 }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color { r, g, b, a }
    }

    /// Creates a color with the specified alpha, keeping the same RGB values
    pub const fn with_alpha(self, a: u8) -> Self {
        Color { a, ..self }
    }

    /// Returns true if the color is fully transparent (alpha = 0)
    pub const fn is_transparent(self) -> bool {
        self.a == 0
    }

    /// Returns true if the color is fully opaque (alpha = 255)
    pub const fn is_opaque(self) -> bool {
        self.a == 255
    }
}

struct ColorF32 {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

impl From<Color> for ColorF32 {
    fn from(Color { r, g, b, a }: Color) -> Self {
        ColorF32 {
            r: r as f32,
            g: g as f32,
            b: b as f32,
            a: a as f32,
        }
    }
}

impl animation::Animatable for Color {
    fn apply_progress(&self, to: &Self, progress: f32) -> Self {
        let from = ColorF32::from(*self);
        let to = ColorF32::from(*to);

        Color {
            r: (from.r + ((to.r - from.r) * progress)) as u8,
            g: (from.g + ((to.g - from.g) * progress)) as u8,
            b: (from.b + ((to.b - from.b) * progress)) as u8,
            a: (from.a + ((to.a - from.a) * progress)) as u8,
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        let Color { r, g, b, a } = self;

        if *a == 255 {
            write!(formatter, "rgb({r}, {g}, {b})")
        } else {
            let alpha = *a as f32 / 255.0;
            write!(formatter, "rgba({r}, {g}, {b}, {alpha:.3})")
        }
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
        assert_eq!(Color::hex("#f0f"), Color::rgba(255, 0, 255, 255));
        assert_eq!(Color::hex("#f0f8"), Color::rgba(255, 0, 255, 136));
        assert_eq!(Color::hex("#ff00ff"), Color::rgba(255, 0, 255, 255));
        assert_eq!(Color::hex("#ff00ff80"), Color::rgba(255, 0, 255, 128));
        assert_eq!(Color::hex("#ef4444"), Color::rgba(239, 68, 68, 255));
        assert_eq!(Color::hex("#1c1917"), Color::rgba(28, 25, 23, 255));
    }

    #[test]
    pub fn hex_num_parsing() {
        assert_eq!(Color::hex_num(0xf0f), Color::rgba(255, 0, 255, 255));
        assert_eq!(Color::hex_num(0xf0f8), Color::rgba(255, 0, 255, 136));
        assert_eq!(Color::hex_num(0xff00ff), Color::rgba(255, 0, 255, 255));
        assert_eq!(Color::hex_num(0xff00ff80), Color::rgba(255, 0, 255, 128));
        assert_eq!(Color::hex_num(0xef4444), Color::rgba(239, 68, 68, 255));
        assert_eq!(Color::hex_num(0x1c1917), Color::rgba(28, 25, 23, 255));
    }

    #[test]
    pub fn alpha_methods() {
        let color = Color::rgb(255, 0, 0);
        assert!(color.is_opaque());
        assert!(!color.is_transparent());

        let transparent = color.with_alpha(0);
        assert!(transparent.is_transparent());
        assert!(!transparent.is_opaque());

        let semi_transparent = color.with_alpha(128);
        assert!(!semi_transparent.is_opaque());
        assert!(!semi_transparent.is_transparent());
    }

    #[test]
    pub fn display_formatting() {
        let opaque = Color::rgb(255, 0, 0);
        assert_eq!(format!("{opaque}"), "rgb(255, 0, 0)");

        let transparent = Color::rgba(255, 0, 0, 128);
        assert_eq!(format!("{transparent}"), "rgba(255, 0, 0, 0.502)");
    }
}
