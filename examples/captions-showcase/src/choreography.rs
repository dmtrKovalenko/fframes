//! Authored paths and optical replicas, independent of subtitle text or cue identity.
//! All path offsets are relative to the caption engine's measured final glyph pose.
use fframes::{
    Svgr,
    animation::{AnimationRuntime, Easing},
    captions::{CaptionError, CaptionWordScope},
    svgr,
};
use serde::Deserialize;

fn one() -> f64 {
    1.0
}
fn duration() -> f64 {
    0.45
}
fn curve() -> [f64; 4] {
    [0.22, 0.75, 0.24, 1.0]
}
fn line_width() -> f64 {
    1.4
}
fn faint() -> f64 {
    0.22
}
fn silver() -> String {
    "#a8cfdf".into()
}
fn yes() -> bool {
    true
}
fn retirement() -> f64 {
    0.18
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pose {
    pub x: f64,
    pub y: f64,
    pub rotate: f64,
    pub scale: f64,
    /// Per-axis factors multiply `scale`; all three settle to one.
    pub scale_x: f64,
    pub scale_y: f64,
    pub opacity: f64,
}

impl Default for Pose {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            rotate: 0.0,
            scale: 1.0,
            scale_x: 1.0,
            scale_y: 1.0,
            opacity: 1.0,
        }
    }
}

/// A character-indexed path. No text is replaced and spoken intervals stay intact.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlyphMotion {
    pub index: usize,
    #[serde(default)]
    pub delay: f64,
    #[serde(default = "duration")]
    pub duration: f64,
    #[serde(default)]
    pub from: Pose,
    /// Absolute offsets relative to the final glyph, not relative to `from`.
    #[serde(default)]
    pub control1: Option<[f64; 2]>,
    #[serde(default)]
    pub control2: Option<[f64; 2]>,
    #[serde(default = "curve")]
    pub easing: [f64; 4],
}

/// A filled sample of a glyph's actual future trajectory. The current glyph
/// catches up to these poses; they retire instead of lingering as a second word.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Echo {
    pub future_seconds: f64,
    #[serde(default = "faint")]
    pub opacity: f64,
    #[serde(default = "silver")]
    pub fill: String,
    /// Smooth fade during the final part of the primary glyph's path.
    #[serde(default = "retirement")]
    pub retire_seconds: f64,
}

pub fn validate_echoes(echoes: &[Echo], path: &str) -> Result<(), CaptionError> {
    if echoes.len() > 32 {
        return Err(CaptionError::invalid(
            path,
            "at most thirty-two echoes per word",
        ));
    }
    for (index, echo) in echoes.iter().enumerate() {
        let p = format!("{path}[{index}]");
        finite(
            echo.future_seconds,
            0.001,
            2.0,
            format!("{p}.future_seconds"),
        )?;
        finite(echo.opacity, 0.0, 1.0, format!("{p}.opacity"))?;
        finite(
            echo.retire_seconds,
            0.01,
            2.0,
            format!("{p}.retire_seconds"),
        )?;
        if echo.fill.trim().is_empty() {
            return Err(CaptionError::invalid(
                format!("{p}.fill"),
                "must be a nonempty color",
            ));
        }
    }
    Ok(())
}

/// An exact glyph copy behind the word, outlined and optionally filled. A
/// collapsing copy can hold its authored pose before converging and fading.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replica {
    pub offset: [f64; 2],
    #[serde(default)]
    pub rotate: f64,
    #[serde(default = "one")]
    pub scale: f64,
    #[serde(default = "silver")]
    pub color: String,
    /// A solid glyph-masked face beneath the outline; absent keeps it hollow.
    #[serde(default)]
    pub fill: Option<String>,
    #[serde(default = "faint")]
    pub opacity: f64,
    #[serde(default = "line_width")]
    pub stroke_width: f64,
    /// Absolute caption-clock seconds.
    pub start: f64,
    #[serde(default = "duration")]
    pub duration: f64,
    /// Keep a collapsing replica at its full authored pose before its duration
    /// begins. Non-collapsing replicas keep their existing entrance behavior.
    #[serde(default)]
    pub hold_seconds: f64,
    #[serde(default = "yes")]
    pub collapse: bool,
}

pub struct Choreography {
    paths: Vec<Option<Path>>,
    travel_padding: f64,
    transform_padding: f64,
    settled_at: f64,
}

struct Path {
    delay: f64,
    duration: f64,
    from: Pose,
    control1: [f64; 2],
    control2: [f64; 2],
    easing: AnimationRuntime,
}

fn finite(value: f64, low: f64, high: f64, path: impl Into<String>) -> Result<(), CaptionError> {
    if value.is_finite() && (low..=high).contains(&value) {
        return Ok(());
    }
    Err(CaptionError::invalid(
        path,
        format!("must be finite and between {low} and {high}"),
    ))
}

fn vector(value: [f64; 2], path: &str) -> Result<(), CaptionError> {
    for (axis, value) in value.into_iter().enumerate() {
        finite(value, -4000.0, 4000.0, format!("{path}[{axis}]"))?;
    }
    Ok(())
}

impl Choreography {
    pub fn prepare(
        moves: &[GlyphMotion],
        glyph_count: usize,
        path: &str,
    ) -> Result<Self, CaptionError> {
        let mut paths: Vec<Option<Path>> = (0..glyph_count).map(|_| None).collect();
        for (position, movement) in moves.iter().enumerate() {
            let p = format!("{path}[{position}]");
            if movement.index >= glyph_count {
                return Err(CaptionError::invalid(
                    format!("{p}.index"),
                    "must refer to a character in this word",
                ));
            }
            if paths[movement.index].is_some() {
                return Err(CaptionError::invalid(
                    format!("{p}.index"),
                    "a character may have only one entrance path",
                ));
            }
            finite(movement.delay, 0.0, 10.0, format!("{p}.delay"))?;
            finite(movement.duration, 0.01, 10.0, format!("{p}.duration"))?;
            vector([movement.from.x, movement.from.y], &format!("{p}.from"))?;
            finite(
                movement.from.rotate,
                -720.0,
                720.0,
                format!("{p}.from.rotate"),
            )?;
            finite(movement.from.scale, 0.05, 4.0, format!("{p}.from.scale"))?;
            finite(
                movement.from.scale_x,
                0.05,
                4.0,
                format!("{p}.from.scale_x"),
            )?;
            finite(
                movement.from.scale_y,
                0.05,
                4.0,
                format!("{p}.from.scale_y"),
            )?;
            finite(movement.from.opacity, 0.0, 1.0, format!("{p}.from.opacity"))?;
            let control1 = movement
                .control1
                .unwrap_or([movement.from.x * 2.0 / 3.0, movement.from.y * 2.0 / 3.0]);
            let control2 = movement
                .control2
                .unwrap_or([movement.from.x / 3.0, movement.from.y / 3.0]);
            vector(control1, &format!("{p}.control1"))?;
            vector(control2, &format!("{p}.control2"))?;
            for (index, value) in movement.easing.into_iter().enumerate() {
                finite(value, 0.0, 1.0, format!("{p}.easing[{index}]"))?;
            }
            let [x1, y1, x2, y2] = movement.easing.map(|value| value as f32);
            paths[movement.index] = Some(Path {
                delay: movement.delay,
                duration: movement.duration,
                from: movement.from,
                control1,
                control2,
                easing: AnimationRuntime::new(
                    movement.duration as f32,
                    &Easing::CubicBezier(x1, y1, x2, y2),
                ),
            });
        }
        let travel_padding = paths
            .iter()
            .flatten()
            .flat_map(|path| {
                [
                    path.from.x.abs(),
                    path.from.y.abs(),
                    path.control1[0].abs(),
                    path.control1[1].abs(),
                    path.control2[0].abs(),
                    path.control2[1].abs(),
                ]
            })
            .fold(0.0, f64::max);
        let transform_padding = paths
            .iter()
            .flatten()
            .map(|path| {
                let turn = path.from.rotate.abs().min(90.0).to_radians().sin();
                // Each scale factor interpolates independently. The product of
                // their maxima is conservative even when their directions differ.
                let scale =
                    path.from.scale.max(1.0) * path.from.scale_x.max(path.from.scale_y).max(1.0);
                (turn * scale + (scale - 1.0).max(0.0)) * 1.5
            })
            .fold(0.0, f64::max);
        let settled_at = paths
            .iter()
            .flatten()
            .map(|path| path.delay + path.duration)
            .fold(0.0, f64::max);
        Ok(Self {
            paths,
            travel_padding,
            transform_padding,
            settled_at,
        })
    }

    /// The path is contained in the convex hull of its control points. Reserve
    /// that travel plus a conservative character rotation/scale clearance while
    /// it moves; settled glyphs need only their ordinary shadow region.
    pub fn shadow_clearance(&self, elapsed: f64, font_size: f64) -> f64 {
        if elapsed >= self.settled_at {
            0.0
        } else {
            self.travel_padding + self.transform_padding * font_size
        }
    }

    pub fn pose(&self, index: usize, elapsed: f64) -> Pose {
        let Some(Some(path)) = self.paths.get(index) else {
            return Pose::default();
        };
        let time = elapsed - path.delay;
        if time <= 0.0 {
            return path.from;
        }
        if time >= path.duration {
            return Pose::default();
        }
        let t = f64::from(path.easing.solve(&(time as f32))).clamp(0.0, 1.0);
        let u = 1.0 - t;
        let evaluate =
            |a: f64, b: f64, c: f64| u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * c;
        Pose {
            x: evaluate(path.from.x, path.control1[0], path.control2[0]),
            y: evaluate(path.from.y, path.control1[1], path.control2[1]),
            rotate: path.from.rotate * u,
            scale: 1.0 + (path.from.scale - 1.0) * u,
            scale_x: 1.0 + (path.from.scale_x - 1.0) * u,
            scale_y: 1.0 + (path.from.scale_y - 1.0) * u,
            opacity: 1.0 + (path.from.opacity - 1.0) * u,
        }
    }

    /// Sample the same prepared path in the future. `elapsed` is relative to
    /// the word's authored visual start, just as for `pose`. Seeking is stateless.
    pub fn echo_pose(&self, index: usize, elapsed: f64, echo: &Echo) -> Option<Pose> {
        let path = self.paths.get(index)?.as_ref()?;
        if elapsed < 0.0 || elapsed >= path.delay + path.duration {
            return None;
        }
        let remaining = path.delay + path.duration - elapsed;
        let fade = (remaining / echo.retire_seconds).clamp(0.0, 1.0);
        let fade = fade * fade * (3.0 - 2.0 * fade);
        let mut pose = self.pose(index, elapsed + echo.future_seconds);
        pose.opacity *= echo.opacity * fade;
        (pose.opacity > 0.0).then_some(pose)
    }
}

pub fn validate_replicas(
    replicas: &[Replica],
    cue_start: f64,
    cue_end: f64,
    path: &str,
) -> Result<(), CaptionError> {
    if replicas.len() > 16 {
        return Err(CaptionError::invalid(
            path,
            "at most sixteen replicas per word",
        ));
    }
    for (index, copy) in replicas.iter().enumerate() {
        let p = format!("{path}[{index}]");
        vector(copy.offset, &format!("{p}.offset"))?;
        finite(copy.rotate, -360.0, 360.0, format!("{p}.rotate"))?;
        finite(copy.scale, 0.1, 4.0, format!("{p}.scale"))?;
        finite(copy.opacity, 0.0, 1.0, format!("{p}.opacity"))?;
        finite(copy.stroke_width, 0.2, 20.0, format!("{p}.stroke_width"))?;
        finite(copy.start, cue_start, cue_end, format!("{p}.start"))?;
        finite(copy.duration, 0.01, 10.0, format!("{p}.duration"))?;
        finite(copy.hold_seconds, 0.0, 10.0, format!("{p}.hold_seconds"))?;
        if copy
            .fill
            .as_ref()
            .is_some_and(|fill| fill.trim().is_empty())
        {
            return Err(CaptionError::invalid(
                format!("{p}.fill"),
                "must be a nonempty color when provided",
            ));
        }
    }
    Ok(())
}

fn replica_phase(copy: &Replica, time: f64) -> Option<(f64, f64)> {
    if time < copy.start {
        return None;
    }
    let elapsed = time
        - copy.start
        - if copy.collapse {
            copy.hold_seconds
        } else {
            0.0
        };
    let progress = (elapsed / copy.duration).clamp(0.0, 1.0);
    if copy.collapse && progress >= 1.0 {
        return None;
    }
    let eased = progress * progress * (3.0 - 2.0 * progress);
    let travel = if copy.collapse { 1.0 - eased } else { 1.0 };
    let opacity = copy.opacity
        * if copy.collapse {
            (1.0 - progress).sqrt()
        } else {
            eased
        };
    Some((travel, opacity))
}

pub fn replicas<'a>(
    scope: &CaptionWordScope<'a, '_>,
    copies: &'a [Replica],
    time: f64,
) -> Svgr<'a> {
    copies.iter().enumerate().filter_map(|(index, copy)| {
        let (travel, opacity) = replica_phase(copy, time)?;
        let x = copy.offset[0] * travel;
        let y = copy.offset[1] * travel;
        let rotation = copy.rotate * travel;
        let scale = 1.0 + (copy.scale - 1.0) * travel;
        let cx = scope.width() * 0.5;
        let cy = scope.height() * 0.5;
        let padding = copy.stroke_width * 2.0 + 4.0;
        let width = scope.width() + padding * 2.0;
        let height = scope.height() + padding * 2.0;
        let id = scope.id(&format!("choreography-replica-{index}"));
        let glyph = scope.mask_text(svgr!(<rect x={-padding} y={-padding} width={width} height={height} fill="white" />));
        let solid = copy.fill.as_ref().map(|fill| scope.mask_text(svgr!(
            <rect x={-padding} y={-padding} width={width} height={height} fill={fill.as_str()} />
        ))).unwrap_or_default();
        Some(svgr!(<g opacity={opacity} transform={format!("translate({x} {y}) translate({cx} {cy}) rotate({rotation}) scale({scale}) translate({} {})", -cx, -cy)}>
            {solid}
            <defs><filter id={id.clone()} filterUnits="userSpaceOnUse" x={-padding} y={-padding}
                width={width} height={height} color-interpolation-filters="sRGB">
                <feMorphology in="SourceAlpha" operator="dilate" radius={copy.stroke_width * 0.5} result="outside" />
                <feMorphology in="SourceAlpha" operator="erode" radius={copy.stroke_width * 0.5} result="inside" />
                <feComposite in="outside" in2="inside" operator="out" result="ring" />
                <feFlood flood-color={copy.color.as_str()} result="ink" />
                <feComposite in="ink" in2="ring" operator="in" />
            </filter></defs>
            <g filter={format!("url(#{id})")}>{glyph}</g>
        </g>))
    }).collect()
}
