//! Place a source (image, video frame) inside a destination box.
//!
//! ```ignore
//! let photo = ctx.get_image("still.jpg")?;
//! let dest = Rect::from_size(1920.0, 1080.0);
//! let placed = Fit::Cover.place(
//!     photo.metadata.width as f32,
//!     photo.metadata.height as f32,
//!     dest,
//! );
//! fframes::svgr!(
//!     <image href={photo.href()}
//!            x={placed.x} y={placed.y}
//!            width={placed.width} height={placed.height} />
//! )
//! ```
//!
//! [`Fit::Cover`] draws larger than `dest`; clip to [`Placement::clip`] if the overflow
//! should not show. [`PanZoom`] animates a cover-based crop: drive `t` with
//! `frame.animate` from 0 to 1.
use std::fmt;

/// Axis-aligned rectangle in the video's SVG coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    /// A rectangle with its top-left corner at (`x`, `y`).
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// A rectangle at the origin.
    pub const fn from_size(width: f32, height: f32) -> Self {
        Self::new(0.0, 0.0, width, height)
    }

    /// The x coordinate of the right edge.
    pub fn right(self) -> f32 {
        self.x + self.width
    }

    /// The y coordinate of the bottom edge.
    pub fn bottom(self) -> f32 {
        self.y + self.height
    }

    /// The centre point as `(x, y)`.
    pub fn center(self) -> (f32, f32) {
        (self.x + self.width * 0.5, self.y + self.height * 0.5)
    }

    /// True when the width or the height is zero or negative.
    pub fn is_empty(self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

/// How a source rectangle is mapped into [`Rect`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fit {
    /// Scale uniformly until the destination is filled; overflow is cropped.
    #[default]
    Cover,
    /// Scale uniformly until the source fits inside the destination.
    Contain,
    /// Stretch to the destination, ignoring aspect ratio.
    Fill,
    /// Native size, centered on the destination. May overflow.
    None,
    /// Like [`Fit::Contain`], but never larger than the source.
    ScaleDown,
}

/// Where to draw a source after a [`Fit`] or [`PanZoom`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Destination box. Clip the drawing to this when the placement overflows.
    pub clip: Rect,
    /// Horizontal scale of the source (`width / src_width`).
    pub scale_x: f32,
    /// Vertical scale of the source (`height / src_height`).
    pub scale_y: f32,
}

impl Placement {
    /// The drawn area as a [`Rect`], which can extend past [`Placement::clip`].
    pub fn rect(self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height)
    }
}

impl Fit {
    /// Place a source of `src_width` × `src_height` into `dest`.
    pub fn place(self, src_width: f32, src_height: f32, dest: Rect) -> Placement {
        if dest.is_empty() {
            return Placement {
                x: dest.x,
                y: dest.y,
                width: dest.width.max(0.0),
                height: dest.height.max(0.0),
                clip: dest,
                scale_x: 1.0,
                scale_y: 1.0,
            };
        }
        if src_width <= 0.0 || src_height <= 0.0 {
            return Placement {
                x: dest.x,
                y: dest.y,
                width: dest.width,
                height: dest.height,
                clip: dest,
                scale_x: 1.0,
                scale_y: 1.0,
            };
        }

        match self {
            Fit::Fill => Placement {
                x: dest.x,
                y: dest.y,
                width: dest.width,
                height: dest.height,
                clip: dest,
                scale_x: dest.width / src_width,
                scale_y: dest.height / src_height,
            },
            Fit::None => centered(src_width, src_height, 1.0, 1.0, dest),
            Fit::Cover => {
                let scale = (dest.width / src_width).max(dest.height / src_height);
                scaled(src_width, src_height, scale, dest)
            }
            Fit::Contain => {
                let scale = (dest.width / src_width).min(dest.height / src_height);
                scaled(src_width, src_height, scale, dest)
            }
            Fit::ScaleDown => {
                let scale = (dest.width / src_width)
                    .min(dest.height / src_height)
                    .min(1.0);
                scaled(src_width, src_height, scale, dest)
            }
        }
    }
}

fn scaled(src_width: f32, src_height: f32, scale: f32, dest: Rect) -> Placement {
    centered(src_width, src_height, scale, scale, dest)
}

fn centered(src_width: f32, src_height: f32, scale_x: f32, scale_y: f32, dest: Rect) -> Placement {
    let width = src_width * scale_x;
    let height = src_height * scale_y;
    Placement {
        x: dest.x + (dest.width - width) * 0.5,
        y: dest.y + (dest.height - height) * 0.5,
        width,
        height,
        clip: dest,
        scale_x,
        scale_y,
    }
}

/// A cover-based crop that moves from one focal point and zoom to another.
///
/// `zoom` is relative to [`Fit::Cover`]: `1.0` fills the destination, `1.2` is
/// twenty percent closer, and values below `1.0` are treated as `1.0` so the
/// destination stays covered. `focus` is a point in the source in 0–1 coordinates
/// (`(0.5, 0.5)` is the centre) mapped to the centre of the destination.
///
/// Drive `t` from 0 to 1 with a timeline:
///
/// ```ignore
/// let t = frame.animate(&fframes::timeline!(
///     at 0.0 => 4.0, animate 0.0_f32 => 1.0, Easing::EaseInOut,
/// ));
/// let placed = PanZoom::new()
///     .zoom(1.0, 1.35)
///     .focus((0.4, 0.5), (0.65, 0.4))
///     .place(src_w, src_h, dest, t);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanZoom {
    pub from_zoom: f32,
    pub to_zoom: f32,
    pub from_focus: (f32, f32),
    pub to_focus: (f32, f32),
}

impl Default for PanZoom {
    fn default() -> Self {
        Self {
            from_zoom: 1.0,
            to_zoom: 1.2,
            from_focus: (0.5, 0.5),
            to_focus: (0.5, 0.5),
        }
    }
}

impl PanZoom {
    /// The default crop: centred, zooming from `1.0` to `1.2`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the zoom at `t = 0` and `t = 1`. Values below `1.0` are treated as `1.0`.
    pub fn zoom(mut self, from: f32, to: f32) -> Self {
        self.from_zoom = from;
        self.to_zoom = to;
        self
    }

    /// Set the focal point at `t = 0` and `t = 1`, in 0–1 source coordinates.
    /// Values outside that range are clamped.
    pub fn focus(mut self, from: (f32, f32), to: (f32, f32)) -> Self {
        self.from_focus = from;
        self.to_focus = to;
        self
    }

    /// Place the source at progress `t` in 0–1 (values outside are clamped).
    pub fn place(self, src_width: f32, src_height: f32, dest: Rect, t: f32) -> Placement {
        if dest.is_empty() || src_width <= 0.0 || src_height <= 0.0 {
            return Fit::Fill.place(src_width, src_height, dest);
        }

        let t = t.clamp(0.0, 1.0);
        // Below cover scale the drawing would leave part of `dest` uncovered.
        let zoom = lerp(self.from_zoom, self.to_zoom, t).max(1.0);
        let focus_x = lerp(self.from_focus.0, self.to_focus.0, t).clamp(0.0, 1.0);
        let focus_y = lerp(self.from_focus.1, self.to_focus.1, t).clamp(0.0, 1.0);

        let cover = (dest.width / src_width).max(dest.height / src_height);
        let scale = cover * zoom;
        let width = src_width * scale;
        let height = src_height * scale;
        let (cx, cy) = dest.center();
        let mut x = cx - focus_x * width;
        let mut y = cy - focus_y * height;

        x = clamp_axis(x, width, dest.x, dest.width);
        y = clamp_axis(y, height, dest.y, dest.height);

        Placement {
            x,
            y,
            width,
            height,
            clip: dest,
            scale_x: scale,
            scale_y: scale,
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Keep `dest` covered when the drawing is larger; otherwise centre it.
fn clamp_axis(origin: f32, size: f32, dest_origin: f32, dest_size: f32) -> f32 {
    if size >= dest_size {
        origin.clamp(dest_origin + dest_size - size, dest_origin)
    } else {
        dest_origin + (dest_size - size) * 0.5
    }
}

impl fmt::Display for Fit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Fit::Cover => "cover",
            Fit::Contain => "contain",
            Fit::Fill => "fill",
            Fit::None => "none",
            Fit::ScaleDown => "scale-down",
        })
    }
}
