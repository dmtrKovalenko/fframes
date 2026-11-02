//! Word-local editorial marks. Their timing and art direction come from subtitle metadata.
//!
//! Drawn marks stay on the phrase; a glyph echo instead settles and disappears.
//! Geometry is deterministic for a given seed: the hand-drawn character is in the
//! stroke, not random frame jitter. Precision marks use measured, straight edges.
use std::{f64::consts::TAU, fmt::Write};

use fframes::{Svgr, captions::CaptionWordScope, svgr};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AccentKind {
    Circle,
    Underline,
    DoubleUnderline,
    Box,
    Brackets,
    Strike,
    Arrow,
    Burst,
    Marker,
    GlyphOutline,
    /// A precise, persistent rule drawn beneath the word.
    Rule,
    /// Four crisp registration corners, drawn clockwise.
    CornerFrame,
    /// A measured rule and two terminal ticks.
    AlignmentGuide,
    /// A temporary glyph contour that catches up with the word and fades.
    Echo,
}

fn default_width() -> f64 {
    6.0
}

fn default_padding() -> f64 {
    12.0
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Accent {
    pub kind: AccentKind,
    pub color: String,
    /// Absolute subtitle-clock seconds, independent of a word's spoken end.
    pub start: f64,
    pub duration: f64,
    #[serde(default = "default_width")]
    pub width: f64,
    #[serde(default = "default_padding")]
    pub padding: f64,
    #[serde(default)]
    pub seed: u32,
}

#[derive(Clone, Copy)]
struct Point(f64, f64);

struct PencilPath {
    data: String,
    length: f64,
}

impl PencilPath {
    fn through(points: &[Point]) -> Self {
        let mut data = String::new();
        let mut length = 0.0;
        for (index, point) in points.iter().enumerate() {
            let command = if index == 0 { 'M' } else { 'L' };
            // Writing into a String cannot fail.
            let _ = write!(data, "{command}{:.3},{:.3} ", point.0, point.1);
            if let Some(previous) = index.checked_sub(1).map(|i| points[i]) {
                length += (point.0 - previous.0).hypot(point.1 - previous.1);
            }
        }
        Self { data, length }
    }

    fn cubic(start: Point, first: Point, second: Point, end: Point) -> Self {
        // Sample the actual curve used for drawing, so draw-on distance does not
        // depend on a renderer's support for the SVG pathLength attribute.
        let points: Vec<_> = (0..=40)
            .map(|index| {
                let t = f64::from(index) / 40.0;
                let u = 1.0 - t;
                Point(
                    u.powi(3) * start.0
                        + 3.0 * u.powi(2) * t * first.0
                        + 3.0 * u * t.powi(2) * second.0
                        + t.powi(3) * end.0,
                    u.powi(3) * start.1
                        + 3.0 * u.powi(2) * t * first.1
                        + 3.0 * u * t.powi(2) * second.1
                        + t.powi(3) * end.1,
                )
            })
            .collect();
        Self::through(&points)
    }

    fn draw<'a>(self, accent: &Accent, progress: f64, width: f64, opacity: f64) -> Svgr<'a> {
        self.draw_with_caps(accent, progress, width, opacity, "round", "round")
    }

    fn draw_precise<'a>(
        self,
        accent: &Accent,
        progress: f64,
        width: f64,
        opacity: f64,
    ) -> Svgr<'a> {
        self.draw_with_caps(accent, progress, width, opacity, "butt", "miter")
    }

    fn draw_with_caps<'a>(
        self,
        accent: &Accent,
        progress: f64,
        width: f64,
        opacity: f64,
        cap: &'static str,
        join: &'static str,
    ) -> Svgr<'a> {
        if progress <= 0.0 || self.length <= 0.0 {
            return Svgr::empty();
        }
        // A long empty interval ensures only the first stroke is ever visible.
        let dash = format!("{} {}", self.length, self.length + 1.0);
        let offset = self.length * (1.0 - progress.clamp(0.0, 1.0));
        svgr!(<path d={self.data} fill="none" stroke={accent.color.clone()}
            stroke-width={width} stroke-linecap={cap} stroke-linejoin={join}
            stroke-dasharray={dash} stroke-dashoffset={offset} opacity={opacity} />)
    }
}

fn phase(seed: u32, salt: u32) -> f64 {
    let mut bits = seed.wrapping_add(salt.wrapping_mul(0x9e37_79b9));
    bits ^= bits >> 16;
    bits = bits.wrapping_mul(0x85eb_ca6b);
    bits ^= bits >> 13;
    f64::from(bits) / f64::from(u32::MAX) * TAU
}

fn portion(progress: f64, start: f64, end: f64) -> f64 {
    ((progress - start) / (end - start)).clamp(0.0, 1.0)
}

fn oval(width: f64, height: f64, padding: f64, seed: u32, secondary: bool) -> PencilPath {
    let tilt = 0.025 * phase(seed, 2).sin();
    let cx = width * 0.5;
    let cy = height * 0.60;
    let rx = width * 0.53 + padding;
    let ry = height * 0.43 + padding;
    let wobble = phase(seed, 3);
    let start = if secondary { 0.70 } else { 0.57 };
    let turns = if secondary { 0.73 } else { 1.035 };
    let points: Vec<_> = (0..=112)
        .map(|index| {
            let theta = TAU * (start + f64::from(index) / 112.0 * turns);
            let irregularity =
                1.0 + 0.018 * (3.0 * theta + wobble).sin() + 0.009 * (7.0 * theta - wobble).cos();
            let x = rx * theta.cos() * irregularity;
            let y = ry * theta.sin() * (1.0 + 0.025 * (2.0 * theta + wobble).sin());
            Point(
                cx + x * tilt.cos() - y * tilt.sin() + if secondary { 3.5 } else { 0.0 },
                cy + x * tilt.sin() + y * tilt.cos() + if secondary { -2.5 } else { 0.0 },
            )
        })
        .collect();
    PencilPath::through(&points)
}

/// Render an annotation in the current word's local coordinate system.
///
/// Put `Marker` behind the preset text; put the other kinds above it. This helper
/// never consumes the content, guesses font metrics, or changes text placement.
/// Marks persist after drawing, except `Echo`, which settles and disappears after
/// its authored duration. `padding` controls that echo's initial displacement.
pub fn render<'a>(scope: &CaptionWordScope<'a, '_>, accent: &Accent, time: f64) -> Svgr<'a> {
    let width = scope.width();
    let height = scope.height();
    if !time.is_finite()
        || !accent.start.is_finite()
        || !accent.duration.is_finite()
        || accent.duration <= 0.0
        || !accent.width.is_finite()
        || accent.width <= 0.0
        || !accent.padding.is_finite()
        || width <= 0.0
        || height <= 0.0
        || time <= accent.start
    {
        return Svgr::empty();
    }
    let progress = ((time - accent.start) / accent.duration).clamp(0.0, 1.0);
    let padding = accent.padding.max(0.0);
    let weight = accent.width;
    let wobble = phase(accent.seed, 1).sin();
    let left = -padding;
    let right = width + padding;
    let top = height * 0.19 - padding;
    let bottom = height + padding * 0.55;

    match accent.kind {
        AccentKind::Circle => {
            let primary = oval(width, height, padding, accent.seed, false).draw(
                accent,
                portion(progress, 0.0, 0.82),
                weight,
                0.98,
            );
            let retrace = oval(width, height, padding, accent.seed, true).draw(
                accent,
                portion(progress, 0.62, 1.0),
                weight * 0.42,
                0.65,
            );
            svgr!(<g>{primary}{retrace}</g>)
        }
        AccentKind::Underline | AccentKind::DoubleUnderline => {
            let primary = PencilPath::cubic(
                Point(left - 3.0, bottom),
                Point(width * 0.24, bottom + 3.0 + wobble * 2.0),
                Point(width * 0.72, bottom - 4.0),
                Point(right + 5.0, bottom - 2.5),
            )
            .draw(accent, portion(progress, 0.0, 0.72), weight, 0.98);
            let secondary = if accent.kind == AccentKind::DoubleUnderline {
                PencilPath::cubic(
                    Point(left + width * 0.08, bottom + weight * 1.8),
                    Point(width * 0.33, bottom + weight * 2.5),
                    Point(width * 0.71, bottom + weight * 0.9),
                    Point(right - width * 0.03, bottom + weight * 1.3),
                )
                .draw(accent, portion(progress, 0.50, 1.0), weight * 0.55, 0.82)
            } else {
                Svgr::empty()
            };
            svgr!(<g>{primary}{secondary}</g>)
        }
        AccentKind::Box => {
            let main = PencilPath::through(&[
                Point(left - 3.0, top + 3.0),
                Point(right + 2.0, top - 1.5),
                Point(right - 1.0, bottom + 3.0),
                Point(left + 1.0, bottom - 2.0),
                Point(left - 2.0, top - 7.0),
            ])
            .draw(accent, portion(progress, 0.0, 0.86), weight, 0.97);
            let lip = PencilPath::through(&[
                Point(left + 6.0, bottom + 4.0),
                Point(right + 7.0, bottom + 7.0),
            ])
            .draw(accent, portion(progress, 0.74, 1.0), weight * 0.45, 0.68);
            svgr!(<g>{main}{lip}</g>)
        }
        AccentKind::Brackets => {
            let arm = (height * 0.15).max(14.0);
            let first = PencilPath::through(&[
                Point(left + arm, top - 2.0),
                Point(left - 1.0, top),
                Point(left + 1.5, bottom + 2.0),
                Point(left + arm, bottom),
            ])
            .draw(accent, portion(progress, 0.0, 0.58), weight, 0.98);
            let second = PencilPath::through(&[
                Point(right - arm, top + 1.0),
                Point(right + 2.0, top - 2.0),
                Point(right, bottom + 1.0),
                Point(right - arm, bottom + 3.0),
            ])
            .draw(accent, portion(progress, 0.35, 1.0), weight, 0.98);
            svgr!(<g>{first}{second}</g>)
        }
        AccentKind::Strike => {
            let middle = height * 0.62;
            let first = PencilPath::cubic(
                Point(left - 5.0, middle + height * 0.08),
                Point(width * 0.30, middle - height * 0.01),
                Point(width * 0.65, middle + height * 0.01),
                Point(right + 5.0, middle - height * 0.07),
            )
            .draw(accent, portion(progress, 0.0, 0.72), weight, 0.94);
            let second = PencilPath::through(&[
                Point(left + 3.0, middle - height * 0.065),
                Point(right - 2.0, middle + height * 0.09),
            ])
            .draw(accent, portion(progress, 0.48, 1.0), weight * 0.55, 0.78);
            svgr!(<g>{first}{second}</g>)
        }
        AccentKind::Arrow => {
            let reach = (height * 0.45).max(34.0);
            let tip = Point(right + 3.0, height * 0.58);
            let stem = PencilPath::cubic(
                Point(right + reach * 0.86, bottom + reach * 0.57),
                Point(right + reach * 1.30, bottom + reach * 0.10),
                Point(right + reach * 0.75, height * 0.68),
                tip,
            )
            .draw(accent, portion(progress, 0.0, 0.75), weight, 0.96);
            let head = PencilPath::through(&[
                Point(tip.0 + reach * 0.34, tip.1 - reach * 0.19),
                tip,
                Point(tip.0 + reach * 0.24, tip.1 + reach * 0.37),
            ])
            .draw(accent, portion(progress, 0.62, 1.0), weight * 0.93, 0.96);
            svgr!(<g>{stem}{head}</g>)
        }
        AccentKind::Burst => {
            let reach = (height * 0.23).max(18.0);
            [
                (
                    Point(right + 7.0, top + 9.0),
                    Point(right + reach, top - reach * 0.63),
                ),
                (
                    Point(right + 14.0, top + reach * 0.9),
                    Point(right + reach * 1.65, top + reach * 0.57),
                ),
                (
                    Point(width * 0.88, top - 7.0),
                    Point(width * 0.90, top - reach * 1.2),
                ),
                (
                    Point(left - 7.0, bottom - 4.0),
                    Point(left - reach, bottom + reach * 0.38),
                ),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, (start, end))| {
                PencilPath::through(&[start, end]).draw(
                    accent,
                    portion(progress, index as f64 * 0.14, 0.52 + index as f64 * 0.16),
                    weight * if index == 1 { 0.72 } else { 1.0 },
                    0.92,
                )
            })
            .collect()
        }
        AccentKind::Marker => {
            let x = left + (right - left) * progress;
            let y = height * 0.25;
            let h = height * 0.72;
            let nib = (height * 0.10).min((x - left) * 0.4);
            let path = format!(
                "M{left},{y} L{},{} L{x},{} L{},{} L{},{} Z",
                x - nib,
                y - 3.0,
                y + h - 4.0,
                left + nib * 0.4,
                y + h + 2.0,
                left - nib * 0.25,
                y + h * 0.22,
            );
            svgr!(<path d={path} fill={accent.color.clone()} opacity="0.27" />)
        }
        AccentKind::GlyphOutline => glyph_outline(scope, accent, progress),
        AccentKind::Rule => PencilPath::through(&[Point(left, bottom), Point(right, bottom)])
            .draw_precise(accent, progress, weight, 0.95),
        AccentKind::CornerFrame => {
            let arm = (height * 0.13).clamp(14.0, 46.0);
            [
                [
                    Point(left, top + arm),
                    Point(left, top),
                    Point(left + arm, top),
                ],
                [
                    Point(right - arm, top),
                    Point(right, top),
                    Point(right, top + arm),
                ],
                [
                    Point(right, bottom - arm),
                    Point(right, bottom),
                    Point(right - arm, bottom),
                ],
                [
                    Point(left + arm, bottom),
                    Point(left, bottom),
                    Point(left, bottom - arm),
                ],
            ]
            .into_iter()
            .enumerate()
            .map(|(index, points)| {
                PencilPath::through(&points).draw_precise(
                    accent,
                    portion(progress, index as f64 * 0.18, 0.46 + index as f64 * 0.18),
                    weight,
                    0.90,
                )
            })
            .collect()
        }
        AccentKind::AlignmentGuide => {
            let tick = (height * 0.035).clamp(6.0, 12.0);
            let rule = PencilPath::through(&[Point(left, bottom), Point(right, bottom)])
                .draw_precise(accent, portion(progress, 0.0, 0.78), weight, 0.80);
            let ticks = [left, right]
                .into_iter()
                .enumerate()
                .map(|(index, x)| {
                    PencilPath::through(&[Point(x, bottom - tick), Point(x, bottom + tick)])
                        .draw_precise(
                            accent,
                            portion(
                                progress,
                                0.52 + index as f64 * 0.14,
                                0.86 + index as f64 * 0.14,
                            ),
                            weight,
                            0.90,
                        )
                })
                .collect::<Vec<_>>();
            svgr!(<g>{rule}{ticks}</g>)
        }
        AccentKind::Echo => {
            if progress >= 1.0 {
                return Svgr::empty();
            }
            let remaining = (1.0 - progress).powi(3);
            let direction = if accent.seed.is_multiple_of(2) {
                1.0
            } else {
                -1.0
            };
            let offset = padding * remaining * direction;
            let opacity = 0.62 * portion(progress, 0.0, 0.12) * (1.0 - progress);
            let outline = glyph_outline(scope, accent, 1.0);
            svgr!(<g opacity={opacity} transform={format!("translate({offset} 0)")}>{outline}</g>)
        }
    }
}

fn glyph_outline<'a>(scope: &CaptionWordScope<'a, '_>, accent: &Accent, progress: f64) -> Svgr<'a> {
    let padding = accent.width * 2.0 + 4.0;
    let width = scope.width() + padding * 2.0;
    let height = scope.height() + padding * 2.0;
    let key = format!("pencil-glyph-{}-{}", accent.seed, accent.start.to_bits());
    let filter = scope.id(&format!("{key}-filter"));
    let clip = scope.id(&format!("{key}-clip"));
    let glyph = scope.mask_text(svgr!(
        <rect x={-padding} y={-padding} width={width} height={height} fill="white" />
    ));
    svgr!(
        <defs>
            <filter id={filter.clone()} filterUnits="userSpaceOnUse" x={-padding} y={-padding}
                width={width} height={height} color-interpolation-filters="sRGB">
                <feMorphology in="SourceAlpha" operator="dilate" radius={accent.width * 0.5} result="outer" />
                <feMorphology in="SourceAlpha" operator="erode" radius={accent.width * 0.5} result="inner" />
                <feComposite in="outer" in2="inner" operator="out" result="ring" />
                <feFlood flood-color={accent.color.clone()} result="ink" />
                <feComposite in="ink" in2="ring" operator="in" />
            </filter>
            <clipPath id={clip.clone()} clipPathUnits="userSpaceOnUse">
                <rect x={-padding} y={-padding} width={width * progress} height={height} />
            </clipPath>
        </defs>
        <g clip-path={format!("url(#{clip})")}>
            <g filter={format!("url(#{filter})")}>{glyph}</g>
        </g>
    )
}
