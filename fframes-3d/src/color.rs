use std::ops::{Add, AddAssign, Mul};

use glam::Vec3;

/// A linear RGB color. Values above 1 are brighter than white. Emissive parts, lights and
/// sparks use them, and bloom makes them glow.
///
/// Constructors that take 8-bit or hex values read them as sRGB and convert them to linear.
/// three.js uses the same color management, so `0xff8800` looks the same in both.
///
/// ```rust
/// use fframes_3d::Rgb;
///
/// let orange = Rgb::hex(0xff8800);
/// let also_orange: Rgb = "#ff8800".into();
/// let hot = Rgb::hex(0x66ccff) * 8.0; // HDR, glows with bloom
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rgb(pub Vec3);

impl Rgb {
    pub const BLACK: Rgb = Rgb(Vec3::ZERO);
    pub const WHITE: Rgb = Rgb(Vec3::ONE);

    /// A color from linear components, without conversion.
    pub const fn linear(r: f32, g: f32, b: f32) -> Self {
        Rgb(Vec3::new(r, g, b))
    }

    /// A color from sRGB `0xRRGGBB`, like three.js `new Color(0xff8800)`.
    pub fn hex(hex: u32) -> Self {
        let [_, r, g, b] = hex.to_be_bytes();
        Self::srgb8(r, g, b)
    }

    /// A color from 8-bit sRGB components.
    pub fn srgb8(r: u8, g: u8, b: u8) -> Self {
        Self::srgb(
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
        )
    }

    /// A color from sRGB components in `0..1`.
    pub fn srgb(r: f32, g: f32, b: f32) -> Self {
        Rgb(Vec3::new(r, g, b).map(srgb_to_linear))
    }

    /// A color from sRGB hue in degrees, and saturation and lightness in `0..1`, like CSS
    /// `hsl()`.
    pub fn hsl(hue: f32, saturation: f32, lightness: f32) -> Self {
        let h = hue.rem_euclid(360.0) / 60.0;
        let c = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
        let x = c * (1.0 - (h % 2.0 - 1.0).abs());
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = lightness - c / 2.0;
        Self::srgb(r + m, g + m, b + m)
    }

    /// Parses an sRGB CSS color `#rgb` or `#rrggbb`. Returns black for other text.
    pub fn parse(css: &str) -> Self {
        let hex = css.trim().trim_start_matches('#');
        match hex.len() {
            6 => u32::from_str_radix(hex, 16).map_or(Self::BLACK, Self::hex),
            3 => u32::from_str_radix(hex, 16).map_or(Self::BLACK, |v| {
                // `#fa0` is `#ffaa00`: 17 * 0xf = 0xff.
                let digit = |shift: u32| ((v >> shift) & 0xf) as u8 * 17;
                Self::srgb8(digit(8), digit(4), digit(0))
            }),
            _ => Self::BLACK,
        }
    }

    pub fn r(self) -> f32 {
        self.0.x
    }

    pub fn g(self) -> f32 {
        self.0.y
    }

    pub fn b(self) -> f32 {
        self.0.z
    }

    /// Linear interpolation from `self` (at `t` = 0) to `other` (at `t` = 1).
    pub fn lerp(self, other: Rgb, t: f32) -> Rgb {
        Rgb(self.0.lerp(other.0, t))
    }

    /// Relative luminance of the linear color.
    pub fn luminance(self) -> f32 {
        self.0.dot(Vec3::new(0.2126, 0.7152, 0.0722))
    }

    /// Converts the color to an 8-bit sRGB [`fframes::Color`], clamped to `0..1`. Use it to
    /// give SVG overlays the color of a material.
    pub fn to_color(self) -> fframes::Color {
        let [r, g, b] = self
            .0
            .clamp(Vec3::ZERO, Vec3::ONE)
            .to_array()
            .map(|c| (linear_to_srgb(c) * 255.0 + 0.5) as u8);
        fframes::Color::rgb(r, g, b)
    }
}

impl From<u32> for Rgb {
    fn from(hex: u32) -> Self {
        Rgb::hex(hex)
    }
}

impl From<&str> for Rgb {
    fn from(css: &str) -> Self {
        Rgb::parse(css)
    }
}

impl From<fframes::Color> for Rgb {
    fn from(color: fframes::Color) -> Self {
        Rgb::srgb8(color.r, color.g, color.b)
    }
}

/// Linear components.
impl From<[f32; 3]> for Rgb {
    fn from(c: [f32; 3]) -> Self {
        Rgb(Vec3::from(c))
    }
}

/// Linear components.
impl From<Vec3> for Rgb {
    fn from(c: Vec3) -> Self {
        Rgb(c)
    }
}

impl Mul<f32> for Rgb {
    type Output = Rgb;

    fn mul(self, rhs: f32) -> Rgb {
        Rgb(self.0 * rhs)
    }
}

impl Mul<Rgb> for Rgb {
    type Output = Rgb;

    fn mul(self, rhs: Rgb) -> Rgb {
        Rgb(self.0 * rhs.0)
    }
}

impl Add for Rgb {
    type Output = Rgb;

    fn add(self, rhs: Rgb) -> Rgb {
        Rgb(self.0 + rhs.0)
    }
}

impl AddAssign for Rgb {
    fn add_assign(&mut self, rhs: Rgb) {
        self.0 += rhs.0;
    }
}

/// Converts one sRGB component in `0..1` to linear.
pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Converts one linear component in `0..1` to sRGB.
pub fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_css_agree() {
        assert_eq!(Rgb::hex(0xff8800), Rgb::parse("#ff8800"));
        assert_eq!(Rgb::hex(0xffaa00), Rgb::parse("#fa0"));
        assert!((Rgb::hex(0x808080).r() - 0.2158).abs() < 1e-3);
    }

    #[test]
    fn round_trips_through_fframes_color() {
        let color = Rgb::hex(0x3366cc).to_color();
        assert_eq!((color.r, color.g, color.b), (0x33, 0x66, 0xcc));
    }
}
