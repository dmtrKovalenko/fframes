//! A caption word as a constrained projective plane.
//!
//! The source is the actual caption drawing, captured once through `FFrames`. A
//! homography changes the plane, not the spelling, font metrics or speech clock.
use std::sync::{LazyLock, OnceLock};

use fframes::{
    FFramesContext, Shader, ShaderUniforms, Svgr,
    captions::{CaptionError, CaptionWordScope},
    media::ImageData,
    svgr,
};
use serde::Deserialize;

const SOURCE_PADDING: f64 = 32.0;
const OUTPUT_PADDING: f64 = 48.0;
const REST: [[f64; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

static PLANE: LazyLock<Shader> =
    LazyLock::new(|| Shader::sksl(include_str!("shaders/word-plane.sksl")));

fn duration() -> f64 {
    0.45
}
fn easing() -> [f64; 4] {
    [0.22, 0.78, 0.24, 1.0]
}
fn wire_color() -> String {
    "#b8dbea".into()
}
fn wire_width() -> f64 {
    1.5
}
fn wire_opacity() -> f64 {
    0.6
}
fn wire_fade() -> f64 {
    0.2
}
fn full_light() -> f64 {
    1.0
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaneMotion {
    /// Absolute visual-clock seconds. Spoken word intervals are unaffected.
    pub start: f64,
    #[serde(default = "duration")]
    pub duration: f64,
    /// Offsets from TL, TR, BR, BL, as fractions of measured word width/height.
    /// Example: [-0.08, 0.12] moves that corner left 8% and down 12%.
    #[serde(default)]
    pub corners: [[f64; 2]; 4],
    /// Optional contiguous portions of one source word. Each portion has an
    /// independent projective pose and delay, resolving to the same flat word.
    /// Empty retains the original whole-word plane behavior.
    #[serde(default)]
    pub panels: Vec<PlanePanel>,
    #[serde(default = "easing")]
    pub easing: [f64; 4],
    #[serde(default = "wire_color")]
    pub wire_color: String,
    #[serde(default = "wire_width")]
    pub wire_width: f64,
    #[serde(default = "wire_opacity")]
    pub wire_opacity: f64,
    #[serde(default)]
    pub handles: bool,
    #[serde(default = "wire_fade")]
    pub wire_fade: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanePanel {
    /// Horizontal source interval in whole-word coordinates; panels partition
    /// [0, 1] in order. The last panel also retains the right font overhang.
    pub range: [f64; 2],
    /// TL, TR, BR, BL offsets from this panel's resting rectangle, measured in
    /// whole-word width/height units, not fractions of the narrower panel.
    pub corners: [[f64; 2]; 4],
    #[serde(default)]
    pub delay: f64,
    /// Defaults to the parent plane's duration; starts at parent start + delay.
    pub duration: Option<f64>,
    /// Authored initial face brightness, resolving to 1 with the panel. This
    /// is a small lighting cue, not reconstructed physical lighting.
    #[serde(default = "full_light")]
    pub shade: f64,
}

fn finite(value: f64, min: f64, max: f64, path: impl Into<String>) -> Result<(), CaptionError> {
    if value.is_finite() && (min..=max).contains(&value) {
        Ok(())
    } else {
        Err(CaptionError::invalid(
            path,
            format!("must be finite and between {min} and {max}"),
        ))
    }
}

impl PlaneMotion {
    pub fn validate(&self, cue_start: f64, cue_end: f64, path: &str) -> Result<(), CaptionError> {
        finite(self.start, cue_start, cue_end, format!("{path}.start"))?;
        finite(self.duration, 0.01, 3.0, format!("{path}.duration"))?;
        if self.start + self.settle_after() > cue_end + 1e-9 {
            return Err(CaptionError::invalid(
                format!("{path}.duration"),
                "the plane must settle before the cue ends",
            ));
        }
        for (corner, values) in self.corners.iter().enumerate() {
            for (axis, value) in values.iter().enumerate() {
                finite(
                    *value,
                    -0.8,
                    0.8,
                    format!("{path}.corners[{corner}][{axis}]"),
                )?;
            }
        }
        for (index, value) in self.easing.iter().enumerate() {
            finite(*value, 0.0, 1.0, format!("{path}.easing[{index}]"))?;
        }
        finite(self.wire_width, 0.0, 8.0, format!("{path}.wire_width"))?;
        finite(self.wire_opacity, 0.0, 1.0, format!("{path}.wire_opacity"))?;
        finite(self.wire_fade, 0.01, 2.0, format!("{path}.wire_fade"))?;
        if self.wire_color.trim().is_empty() {
            return Err(CaptionError::invalid(
                format!("{path}.wire_color"),
                "must not be empty",
            ));
        }
        // Each adjacent-edge cross product is quadratic in the interpolation
        // parameter. Checking its interior minimum proves no intermediate fold,
        // unlike testing only the initial and final quad or a few time samples.
        if !unfolded_throughout(self.corners) {
            return Err(CaptionError::invalid(
                format!("{path}.corners"),
                "the plane must remain strictly convex and correctly wound throughout its motion",
            ));
        }
        if !self.panels.is_empty() {
            if self.panels.len() > 12 || self.corners != [[0.0; 2]; 4] {
                return Err(CaptionError::invalid(
                    format!("{path}.panels"),
                    "use at most 12 panels and omit the parent corners when panels are present",
                ));
            }
            let mut edge = 0.0;
            for (index, panel) in self.panels.iter().enumerate() {
                let field = format!("{path}.panels[{index}]");
                for (axis, value) in panel.range.iter().enumerate() {
                    finite(*value, 0.0, 1.0, format!("{field}.range[{axis}]"))?;
                }
                if (panel.range[0] - edge).abs() > 1e-9 || panel.range[1] - panel.range[0] < 0.04 {
                    return Err(CaptionError::invalid(
                        format!("{field}.range"),
                        "panels must partition [0,1] in order, without gaps or overlaps, each at least 0.04 wide",
                    ));
                }
                edge = panel.range[1];
                finite(panel.delay, 0.0, 3.0, format!("{field}.delay"))?;
                finite(panel.shade, 0.5, 1.0, format!("{field}.shade"))?;
                finite(
                    panel.duration.unwrap_or(self.duration),
                    0.01,
                    3.0,
                    format!("{field}.duration"),
                )?;
                for (corner, values) in panel.corners.iter().enumerate() {
                    for (axis, value) in values.iter().enumerate() {
                        finite(
                            *value,
                            -0.8,
                            0.8,
                            format!("{field}.corners[{corner}][{axis}]"),
                        )?;
                    }
                }
                if !unfolded_panel(panel.range, panel.corners) {
                    return Err(CaptionError::invalid(
                        format!("{field}.corners"),
                        "each panel must remain strictly convex and correctly wound throughout its motion",
                    ));
                }
            }
            if (edge - 1.0).abs() > 1e-9 {
                return Err(CaptionError::invalid(
                    format!("{path}.panels"),
                    "the final panel must end at 1",
                ));
            }
        }
        Ok(())
    }

    fn settle_after(&self) -> f64 {
        if self.panels.is_empty() {
            self.duration
        } else {
            self.panels
                .iter()
                .map(|panel| panel.delay + panel.duration.unwrap_or(self.duration))
                .fold(0.0, f64::max)
        }
    }

    pub fn shadow_clearance(&self, width: f64, height: f64) -> f64 {
        (0..=32)
            .flat_map(|index| {
                self.poses(self.start + self.settle_after() * f64::from(index) / 32.0)
            })
            .filter_map(|pose| {
                panel_matrix(pose.points, pose.range)
                    .and_then(|matrix| panel_bounds(matrix, width, height, pose.range))
            })
            .map(|[left, top, right, bottom]| {
                (-left).max(-top).max(right - width).max(bottom - height)
            })
            .fold(OUTPUT_PADDING, f64::max)
            + OUTPUT_PADDING
    }

    fn progress(&self, time: f64) -> f64 {
        if time <= self.start {
            return 0.0;
        }
        if time >= self.start + self.duration {
            return 1.0;
        }
        let position = ((time - self.start) / self.duration).clamp(0.0, 1.0);
        cubic_ease(position, self.easing)
    }

    fn quad(&self, time: f64) -> [[f64; 2]; 4] {
        quad(self.corners, 1.0 - self.progress(time))
    }

    fn poses(&self, time: f64) -> Vec<PanelPose> {
        if self.panels.is_empty() {
            return vec![PanelPose {
                range: [0.0, 1.0],
                points: self.quad(time),
                shade: 1.0,
            }];
        }
        self.panels
            .iter()
            .enumerate()
            .map(|(index, panel)| {
                // Validation permits numerical tolerance; use one canonical
                // boundary so adjacent source clips always have one owner.
                let range = [
                    if index == 0 {
                        0.0
                    } else {
                        self.panels[index - 1].range[1]
                    },
                    if index + 1 == self.panels.len() {
                        1.0
                    } else {
                        panel.range[1]
                    },
                ];
                let position = ((time - self.start - panel.delay)
                    / panel.duration.unwrap_or(self.duration))
                .clamp(0.0, 1.0);
                let remaining = 1.0 - cubic_ease(position, self.easing);
                PanelPose {
                    range,
                    points: panel_quad(range, panel.corners, remaining),
                    shade: 1.0 - (1.0 - panel.shade) * remaining,
                }
            })
            .collect()
    }
}

struct PanelPose {
    range: [f64; 2],
    points: [[f64; 2]; 4],
    shade: f64,
}

fn cubic_coordinate(t: f64, first: f64, second: f64) -> f64 {
    let remaining = 1.0 - t;
    3.0 * remaining * remaining * t * first + 3.0 * remaining * t * t * second + t * t * t
}

fn cubic_ease(position: f64, [x1, y1, x2, y2]: [f64; 4]) -> f64 {
    if position <= 0.0 || position >= 1.0 {
        return position;
    }
    let (mut lower, mut upper) = (0.0, 1.0);
    for _ in 0..24 {
        let middle = f64::midpoint(lower, upper);
        if cubic_coordinate(middle, x1, x2) < position {
            lower = middle;
        } else {
            upper = middle;
        }
    }
    cubic_coordinate(f64::midpoint(lower, upper), y1, y2)
}

fn quad(offsets: [[f64; 2]; 4], remaining: f64) -> [[f64; 2]; 4] {
    panel_quad([0.0, 1.0], offsets, remaining)
}

fn panel_quad([left, right]: [f64; 2], offsets: [[f64; 2]; 4], remaining: f64) -> [[f64; 2]; 4] {
    let rest = [[left, 0.0], [right, 0.0], [right, 1.0], [left, 1.0]];
    std::array::from_fn(|index| {
        [
            rest[index][0] + offsets[index][0] * remaining,
            rest[index][1] + offsets[index][1] * remaining,
        ]
    })
}

fn turn(points: [[f64; 2]; 4], corner: usize) -> f64 {
    let a = points[corner];
    let b = points[(corner + 1) % 4];
    let c = points[(corner + 2) % 4];
    (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
}

fn unfolded_throughout(offsets: [[f64; 2]; 4]) -> bool {
    unfolded_panel([0.0, 1.0], offsets)
}

fn unfolded_panel(range: [f64; 2], offsets: [[f64; 2]; 4]) -> bool {
    (0..4).all(|corner| {
        let start = turn(panel_quad(range, offsets, 0.0), corner);
        let middle = turn(panel_quad(range, offsets, 0.5), corner);
        let end = turn(panel_quad(range, offsets, 1.0), corner);
        let quadratic = 2.0 * (end + start - 2.0 * middle);
        let linear = end - start - quadratic;
        let at = if quadratic > 0.0 {
            (-linear / (2.0 * quadratic)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let minimum = start
            .min(end)
            .min(quadratic * at * at + linear * at + start);
        minimum.is_finite() && minimum > 0.015 * (range[1] - range[0])
    })
}

/// Convert local-panel coordinates into global source coordinates, so the
/// inverse shader always samples the same original word texture.
fn panel_matrix(points: [[f64; 2]; 4], [left, right]: [f64; 2]) -> Option<[f64; 9]> {
    if points == panel_quad([left, right], [[0.0; 2]; 4], 0.0) {
        return Some([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);
    }
    let local = square_to_quad(points)?;
    let span = right - left;
    if span <= 0.0 {
        return None;
    }
    Some([
        local[0] / span,
        local[1],
        local[2] - left * local[0] / span,
        local[3] / span,
        local[4],
        local[5] - left * local[3] / span,
        local[6] / span,
        local[7],
        local[8] - left * local[6] / span,
    ])
}

fn square_to_quad(points: [[f64; 2]; 4]) -> Option<[f64; 9]> {
    let [[x0, y0], [x1, y1], [x2, y2], [x3, y3]] = points;
    let (sx, sy) = (x0 - x1 + x2 - x3, y0 - y1 + y2 - y3);
    let (g, h) = if sx.abs() + sy.abs() < 1e-12 {
        (0.0, 0.0)
    } else {
        let (dx1, dx2, dy1, dy2) = (x1 - x2, x3 - x2, y1 - y2, y3 - y2);
        let determinant = dx1 * dy2 - dx2 * dy1;
        if determinant.abs() < 1e-12 {
            return None;
        }
        (
            (sx * dy2 - dx2 * sy) / determinant,
            (dx1 * sy - sx * dy1) / determinant,
        )
    };
    Some([
        x1 - x0 + g * x1,
        x3 - x0 + h * x3,
        x0,
        y1 - y0 + g * y1,
        y3 - y0 + h * y3,
        y0,
        g,
        h,
        1.0,
    ])
}

fn inverse([a, b, c, d, e, f, g, h, i]: [f64; 9]) -> Option<[f64; 9]> {
    let adjugate = [
        e * i - f * h,
        c * h - b * i,
        b * f - c * e,
        f * g - d * i,
        a * i - c * g,
        c * d - a * f,
        d * h - e * g,
        b * g - a * h,
        a * e - b * d,
    ];
    let determinant = a * adjugate[0] + b * adjugate[3] + c * adjugate[6];
    if !determinant.is_finite() || determinant.abs() < 1e-12 {
        return None;
    }
    Some(adjugate.map(|value| value / determinant))
}

fn panel_bounds(
    matrix: [f64; 9],
    width: f64,
    height: f64,
    [left, right]: [f64; 2],
) -> Option<[f64; 4]> {
    let px = SOURCE_PADDING / width;
    let py = SOURCE_PADDING / height;
    let left = if left == 0.0 { -px } else { left };
    let right = if right == 1.0 { 1.0 + px } else { right };
    let padded = [
        [left, -py],
        [right, -py],
        [right, 1.0 + py],
        [left, 1.0 + py],
    ];
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for [x, y] in padded {
        let denominator = matrix[6] * x + matrix[7] * y + matrix[8];
        // No projection horizon may cross the captured source region. A caller
        // gets ordinary readable type if an extreme small-word pose violates it.
        if !denominator.is_finite() || denominator < 0.015 {
            return None;
        }
        let projected_x = (matrix[0] * x + matrix[1] * y + matrix[2]) / denominator * width;
        let projected_y = (matrix[3] * x + matrix[4] * y + matrix[5]) / denominator * height;
        bounds[0] = bounds[0].min(projected_x);
        bounds[1] = bounds[1].min(projected_y);
        bounds[2] = bounds[2].max(projected_x);
        bounds[3] = bounds[3].max(projected_y);
    }
    // Integer origins keep the same output pixel grid as the plane moves.
    bounds = [
        bounds[0].floor() - 8.0,
        bounds[1].floor() - 8.0,
        bounds[2].ceil() + 8.0,
        bounds[3].ceil() + 8.0,
    ];
    if bounds.iter().all(|value| value.is_finite())
        && bounds[2] - bounds[0] <= 8192.0
        && bounds[3] - bounds[1] <= 8192.0
    {
        Some(bounds)
    } else {
        None
    }
}

struct SourceTexture {
    image: ImageData<'static>,
    width: u32,
    height: u32,
}

/// One immutable word drawing, shared by arbitrary frame order and render workers.
pub struct PlaneRenderer {
    source: OnceLock<SourceTexture>,
}

impl PlaneRenderer {
    pub fn new() -> Self {
        Self {
            source: OnceLock::new(),
        }
    }

    pub fn render<'a>(
        &'a self,
        scope: &CaptionWordScope<'a, '_>,
        text: &Svgr<'a>,
        ctx: &FFramesContext<'_, '_>,
        motion: &'a PlaneMotion,
        time: f64,
        enabled: bool,
    ) -> Svgr<'a> {
        if !enabled || scope.width() <= 0.0 || scope.height() <= 0.0 {
            return text.clone();
        }
        let width = scope.width();
        let height = scope.height();
        let poses = motion.poses(time);
        // The settled word uses one identity sample of the same cached texture.
        // This preserves outlines and avoids a subpixel filtering seam between
        // adjacent regions. Wires keep their original geometry while fading.
        let settled = [PanelPose {
            range: [0.0, 1.0],
            points: REST,
            shade: 1.0,
        }];
        let visible_poses =
            if !motion.panels.is_empty() && time >= motion.start + motion.settle_after() {
                settled.as_slice()
            } else {
                poses.as_slice()
            };
        let prepared: Option<Vec<_>> = visible_poses
            .iter()
            .map(|pose| {
                let forward = panel_matrix(pose.points, pose.range)?;
                Some((
                    pose.range,
                    pose.shade,
                    inverse(forward)?,
                    panel_bounds(forward, width, height, pose.range)?,
                ))
            })
            .collect();
        let Some(prepared) = prepared else {
            return text.clone();
        };
        if self.source.get().is_none() {
            let texture_width = (width + 2.0 * SOURCE_PADDING).ceil() as u32;
            let texture_height = (height + 2.0 * SOURCE_PADDING).ceil() as u32;
            let Some(image) = ctx.render_svgr(svgr!(
                <svg xmlns="http://www.w3.org/2000/svg" width={texture_width} height={texture_height}
                    viewBox={format!("0 0 {texture_width} {texture_height}")}>
                    <g transform={format!("translate({SOURCE_PADDING} {SOURCE_PADDING})")}>
                        {text.clone()}
                    </g>
                </svg>
            ), texture_width, texture_height) else { return text.clone(); };
            // Concurrent first captures are identical. Cache only a successful
            // capture, so missing fonts can retry without locking panel draws.
            let _ = self.source.set(SourceTexture {
                image,
                width: texture_width,
                height: texture_height,
            });
        }
        let Some(texture) = self.source.get() else {
            return text.clone();
        };
        let images: Vec<_> = prepared.into_iter().map(|(range, shade, matrix, [origin_x, origin_y, end_x, end_y])| {
        let image = PLANE.draw(
            scope.frame,
            ShaderUniforms::new()
                .image("uSource", &texture.image)
                .float2("uWordSize", width as f32, height as f32)
                .float2("uTextureSize", texture.width as f32, texture.height as f32)
                .float2("uOrigin", origin_x as f32, origin_y as f32)
                .float("uPadding", SOURCE_PADDING as f32)
                .float2("uSourceRange", range[0] as f32, range[1] as f32)
                .float("uShade", shade as f32)
                .float3(
                    "uInverse0",
                    matrix[0] as f32,
                    matrix[1] as f32,
                    matrix[2] as f32,
                )
                .float3(
                    "uInverse1",
                    matrix[3] as f32,
                    matrix[4] as f32,
                    matrix[5] as f32,
                )
                .float3(
                    "uInverse2",
                    matrix[6] as f32,
                    matrix[7] as f32,
                    matrix[8] as f32,
                ),
        );
        svgr!(<image href={image.href()} x={origin_x} y={origin_y} width={end_x-origin_x} height={end_y-origin_y} />)
        }).collect();
        let elapsed = time - motion.start;
        let wire_opacity = motion.wire_opacity
            * (1.0 - ((elapsed - motion.settle_after()) / motion.wire_fade).clamp(0.0, 1.0));
        let wires: Vec<_> = if wire_opacity > 0.0 && motion.wire_width > 0.0 {
            poses.iter().map(|pose| {
            let vertices = pose.points.map(|point| [point[0] * width, point[1] * height]);
            let path = if motion.panels.is_empty() { format!(
                "M{} {} L{} {} L{} {} L{} {} Z",
                vertices[0][0],
                vertices[0][1],
                vertices[1][0],
                vertices[1][1],
                vertices[2][0],
                vertices[2][1],
                vertices[3][0],
                vertices[3][1]
            ) } else {
                // Interior fold edges, not opaque panel cards or outer boxes.
                let mut edges = String::new();
                if pose.range[0] > 0.0 {
                    let _ = std::fmt::Write::write_fmt(&mut edges, format_args!("M{} {} L{} {} ", vertices[0][0], vertices[0][1], vertices[3][0], vertices[3][1]));
                }
                if pose.range[1] < 1.0 {
                    let _ = std::fmt::Write::write_fmt(&mut edges, format_args!("M{} {} L{} {}", vertices[1][0], vertices[1][1], vertices[2][0], vertices[2][1]));
                }
                edges
            };
            let handles: Vec<_> = if motion.handles {
                vertices.into_iter().map(|[x,y]| svgr!(
                    <rect x={x-3.0} y={y-3.0} width="6" height="6" fill={motion.wire_color.as_str()} />
                )).collect()
            } else {
                Vec::new()
            };
            svgr!(<g opacity={wire_opacity}>
                <path d={path} fill="none" stroke={motion.wire_color.as_str()} stroke-width={motion.wire_width} />
                {handles}
            </g>)
          }).collect()
        } else {
            Vec::new()
        };
        svgr!(<g>
            {images}
            {wires}
        </g>)
    }
}
