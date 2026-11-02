//! Cue-level relationships: calibrated paths, coherent flow and quiet measurements.
//!
//! All positions and timing are authored subtitle metadata. Curves and arc lengths
//! are prepared once; rendering is a deterministic function of the caption clock.
use std::fmt::Write;

use fframes::{Svgr, captions::CaptionError, svgr};
use serde::Deserialize;

type Point = [f64; 2];

fn one() -> f64 {
    1.0
}
fn fade() -> f64 {
    0.2
}
fn width() -> f64 {
    1.5
}
fn font_size() -> usize {
    20
}
fn family() -> String {
    "DM Sans".into()
}
fn weight() -> u16 {
    500
}
fn count() -> usize {
    3
}
fn speed() -> f64 {
    0.5
}
fn radius() -> f64 {
    2.0
}
fn tail() -> f64 {
    0.08
}
fn trace_radius() -> f64 {
    6.0
}
fn trace_tail() -> f64 {
    0.16
}
fn construction_opacity() -> f64 {
    0.25
}
fn enabled() -> bool {
    true
}
fn lock_fraction() -> f64 {
    0.82
}
fn jaw_size() -> f64 {
    32.0
}
fn tick_count() -> usize {
    7
}
fn pixel_unit() -> String {
    "px".into()
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Layer {
    #[default]
    Under,
    Over,
}

/// Shared flat JSON fields. Times are absolute caption-clock seconds.
#[derive(Debug, Clone, Deserialize)]
pub struct Timing {
    pub start: f64,
    pub duration: f64,
    #[serde(default)]
    pub hold_until: Option<f64>,
    #[serde(default = "fade")]
    pub fade_out: f64,
    #[serde(default = "one")]
    pub opacity: f64,
    #[serde(default)]
    pub layer: Layer,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Graphic {
    Path(PathGraphic),
    Label(LabelGraphic),
    Measure(MeasureGraphic),
}

#[derive(Debug, Clone, Deserialize)]
pub struct PathGraphic {
    #[serde(flatten)]
    pub timing: Timing,
    /// Polyline vertices; a cubic instead requires start, control1, control2, end.
    pub points: Vec<Point>,
    #[serde(default)]
    pub cubic: bool,
    pub color: String,
    #[serde(default = "width")]
    pub width: f64,
    /// A dark companion stroke supports the colored path over bright footage.
    #[serde(default)]
    pub keyline: bool,
    #[serde(default)]
    pub arrow: bool,
    #[serde(default)]
    pub flow: Option<Flow>,
    /// A single luminous front follows this path's reveal, then settles at its destination.
    #[serde(default)]
    pub trace: Option<Trace>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trace {
    #[serde(default = "trace_radius")]
    pub head_radius: f64,
    /// Fraction of the prepared arc length illuminated behind the reveal front.
    #[serde(default = "trace_tail")]
    pub tail: f64,
    /// Seconds for the arrived front to dissipate; the drawn path stays in place.
    #[serde(default = "trace_tail")]
    pub linger: f64,
    #[serde(default)]
    pub color: Option<String>,
    /// For cubic paths, display their actual de Casteljau interpolation scaffold.
    #[serde(default)]
    pub construction: bool,
    #[serde(default = "construction_opacity")]
    pub construction_opacity: f64,
    #[serde(default = "enabled")]
    pub arrival_ring: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flow {
    #[serde(default = "count")]
    pub count: usize,
    /// Completed traversals per second, independent of curve control-point spacing.
    #[serde(default = "speed")]
    pub speed: f64,
    #[serde(default)]
    pub phase: f64,
    #[serde(default = "radius")]
    pub radius: f64,
    /// Tail length as a fraction of the path's measured arc length.
    #[serde(default = "tail")]
    pub tail: f64,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TextAlign {
    #[default]
    Start,
    Middle,
    End,
}

impl TextAlign {
    fn svg(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Middle => "middle",
            Self::End => "end",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LabelGraphic {
    #[serde(flatten)]
    pub timing: Timing,
    pub text: String,
    pub x: f64,
    pub y: f64,
    #[serde(default = "font_size")]
    pub font_size: usize,
    #[serde(default = "family")]
    pub font_family: String,
    #[serde(default = "weight")]
    pub font_weight: u16,
    #[serde(default)]
    pub align: TextAlign,
    pub color: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MeasureGraphic {
    #[serde(flatten)]
    pub timing: Timing,
    pub from: Point,
    pub to: Point,
    /// Signed distance from the measured segment, along its left-hand normal.
    pub offset: f64,
    pub label: String,
    #[serde(default = "font_size")]
    pub font_size: usize,
    #[serde(default = "family")]
    pub font_family: String,
    pub color: String,
    #[serde(default = "width")]
    pub width: f64,
    #[serde(default)]
    pub keyline: bool,
    /// A physical measuring jaw closes from an extended aperture onto these endpoints.
    #[serde(default)]
    pub caliper: Option<Caliper>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MovingEnd {
    From,
    #[default]
    To,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Caliper {
    /// Extra opening in composition pixels before the moving jaw closes.
    pub from_extra: f64,
    #[serde(default)]
    pub moving_end: MovingEnd,
    /// Portion of duration spent closing; the rest confirms the locked dimension.
    #[serde(default = "lock_fraction")]
    pub lock_fraction: f64,
    #[serde(default = "jaw_size")]
    pub jaw_size: f64,
    #[serde(default = "tick_count")]
    pub tick_count: usize,
    #[serde(default = "pixel_unit")]
    pub unit: String,
    #[serde(default)]
    pub precision: usize,
}

#[derive(Debug)]
struct Sample {
    point: Point,
    distance: f64,
}

#[derive(Debug)]
struct Geometry {
    data: String,
    length: f64,
    samples: Vec<Sample>,
}

impl Geometry {
    fn new(points: &[Point], cubic: bool) -> Self {
        let mut data = format!("M{:.4},{:.4}", points[0][0], points[0][1]);
        let evaluated = if cubic {
            let _ = write!(
                data,
                " C{:.4},{:.4} {:.4},{:.4} {:.4},{:.4}",
                points[1][0], points[1][1], points[2][0], points[2][1], points[3][0], points[3][1]
            );
            (0..=128)
                .map(|i| {
                    let t = f64::from(i) / 128.0;
                    let u = 1.0 - t;
                    [0, 1].map(|a| {
                        u.powi(3) * points[0][a]
                            + 3.0 * u.powi(2) * t * points[1][a]
                            + 3.0 * u * t.powi(2) * points[2][a]
                            + t.powi(3) * points[3][a]
                    })
                })
                .collect::<Vec<_>>()
        } else {
            for point in &points[1..] {
                let _ = write!(data, " L{:.4},{:.4}", point[0], point[1]);
            }
            points.to_vec()
        };
        let mut length = 0.0;
        let samples = evaluated
            .iter()
            .enumerate()
            .map(|(i, point)| {
                if i > 0 {
                    length += distance(*point, evaluated[i - 1]);
                }
                Sample {
                    point: *point,
                    distance: length,
                }
            })
            .collect();
        Self {
            data,
            length,
            samples,
        }
    }

    /// Arc-length lookup means flow has a coherent speed on bends and straight lines.
    fn at(&self, fraction: f64) -> (Point, Point) {
        let d = fraction.clamp(0.0, 1.0) * self.length;
        let next = self
            .samples
            .partition_point(|sample| sample.distance < d)
            .max(1)
            .min(self.samples.len() - 1);
        let a = &self.samples[next - 1];
        let b = &self.samples[next];
        let span = b.distance - a.distance;
        let t = if span > 0.0 {
            (d - a.distance) / span
        } else {
            0.0
        };
        let tangent = unit([b.point[0] - a.point[0], b.point[1] - a.point[1]]);
        (
            [
                a.point[0] + (b.point[0] - a.point[0]) * t,
                a.point[1] + (b.point[1] - a.point[1]) * t,
            ],
            tangent,
        )
    }

    /// Cubic samples have uniform parameter spacing, unlike their arc lengths.
    fn cubic_parameter_at(&self, fraction: f64) -> f64 {
        let d = fraction.clamp(0.0, 1.0) * self.length;
        let next = self
            .samples
            .partition_point(|sample| sample.distance < d)
            .max(1)
            .min(self.samples.len() - 1);
        let a = &self.samples[next - 1];
        let span = self.samples[next].distance - a.distance;
        let local = if span > 0.0 {
            (d - a.distance) / span
        } else {
            0.0
        };
        (next as f64 - 1.0 + local) / (self.samples.len() - 1) as f64
    }
}

#[derive(Debug)]
struct PreparedPath {
    graphic: PathGraphic,
    geometry: Geometry,
}

#[derive(Debug)]
struct PreparedMeasure {
    graphic: MeasureGraphic,
    geometry: Geometry,
    extensions: String,
    ticks: String,
    center: Point,
    angle: f64,
    from: Point,
    to: Point,
    tangent: Point,
    normal: Point,
    caliper_rail: String,
    caliper_ticks: String,
}

#[derive(Debug)]
enum PreparedKind {
    Path(PreparedPath),
    Label(LabelGraphic),
    Measure(PreparedMeasure),
}

#[derive(Debug)]
struct PreparedGraphic {
    timing: Timing,
    hold_until: f64,
    kind: PreparedKind,
}

/// Geometry and path strings are immutable after preparation and can be shared by workers.
#[derive(Debug, Default)]
pub struct PreparedGraphics {
    items: Vec<PreparedGraphic>,
}

impl PreparedGraphics {
    pub fn new(graphics: &[Graphic], cue_start: f64, cue_end: f64) -> Result<Self, CaptionError> {
        let mut items = Vec::with_capacity(graphics.len());
        for (i, graphic) in graphics.iter().enumerate() {
            let path = format!("metadata.editorial.graphics[{i}]");
            let timing = match graphic {
                Graphic::Path(g) => &g.timing,
                Graphic::Label(g) => &g.timing,
                Graphic::Measure(g) => &g.timing,
            };
            range(timing.start, cue_start, cue_end, &format!("{path}.start"))?;
            range(
                timing.duration,
                0.001,
                (cue_end - timing.start).max(0.001),
                &format!("{path}.duration"),
            )?;
            let hold_until = timing.hold_until.unwrap_or(cue_end);
            range(
                hold_until,
                timing.start + timing.duration,
                cue_end + 1e-9,
                &format!("{path}.hold_until"),
            )?;
            range(timing.fade_out, 0.001, 5.0, &format!("{path}.fade_out"))?;
            range(timing.opacity, 0.0, 1.0, &format!("{path}.opacity"))?;
            let kind = match graphic {
                Graphic::Path(g) => {
                    if g.points.len() < 2
                        || (g.cubic && g.points.len() != 4)
                        || g.points.len() > 128
                    {
                        return Err(CaptionError::invalid(
                            format!("{path}.points"),
                            "a polyline needs 2–128 points; a cubic needs start, two controls and end",
                        ));
                    }
                    for (j, point) in g.points.iter().enumerate() {
                        validate_point(*point, &format!("{path}.points[{j}]"))?;
                    }
                    range(g.width, 0.1, 20.0, &format!("{path}.width"))?;
                    color(&g.color, &format!("{path}.color"))?;
                    if let Some(flow) = &g.flow {
                        if !(1..=8).contains(&flow.count) {
                            return Err(CaptionError::invalid(
                                format!("{path}.flow.count"),
                                "must be between 1 and 8",
                            ));
                        }
                        range(flow.speed, 0.0, 5.0, &format!("{path}.flow.speed"))?;
                        range(flow.phase, -10.0, 10.0, &format!("{path}.flow.phase"))?;
                        range(flow.radius, 0.25, 12.0, &format!("{path}.flow.radius"))?;
                        range(flow.tail, 0.0, 0.35, &format!("{path}.flow.tail"))?;
                        if let Some(value) = &flow.color {
                            color(value, &format!("{path}.flow.color"))?;
                        }
                    }
                    if let Some(trace) = &g.trace {
                        range(
                            trace.head_radius,
                            1.0,
                            16.0,
                            &format!("{path}.trace.head_radius"),
                        )?;
                        range(trace.tail, 0.0, 0.5, &format!("{path}.trace.tail"))?;
                        range(trace.linger, 0.01, 2.0, &format!("{path}.trace.linger"))?;
                        range(
                            trace.construction_opacity,
                            0.0,
                            1.0,
                            &format!("{path}.trace.construction_opacity"),
                        )?;
                        if trace.construction && !g.cubic {
                            return Err(CaptionError::invalid(
                                format!("{path}.trace.construction"),
                                "de Casteljau construction requires a cubic path",
                            ));
                        }
                        if let Some(value) = &trace.color {
                            color(value, &format!("{path}.trace.color"))?;
                        }
                    }
                    let geometry = Geometry::new(&g.points, g.cubic);
                    range(geometry.length, 0.01, 100_000.0, &format!("{path}.points"))?;
                    PreparedKind::Path(PreparedPath {
                        graphic: g.clone(),
                        geometry,
                    })
                }
                Graphic::Label(g) => {
                    validate_point([g.x, g.y], &path)?;
                    text(&g.text, &g.font_family, g.font_size, &path)?;
                    if !(100..=900).contains(&g.font_weight) {
                        return Err(CaptionError::invalid(
                            format!("{path}.font_weight"),
                            "must be between 100 and 900",
                        ));
                    }
                    color(&g.color, &format!("{path}.color"))?;
                    PreparedKind::Label(g.clone())
                }
                Graphic::Measure(g) => {
                    validate_point(g.from, &format!("{path}.from"))?;
                    validate_point(g.to, &format!("{path}.to"))?;
                    range(distance(g.from, g.to), 1.0, 10000.0, &format!("{path}.to"))?;
                    range(g.offset, -500.0, 500.0, &format!("{path}.offset"))?;
                    range(g.width, 0.1, 20.0, &format!("{path}.width"))?;
                    text(&g.label, &g.font_family, g.font_size, &path)?;
                    color(&g.color, &format!("{path}.color"))?;
                    if let Some(caliper) = &g.caliper {
                        range(
                            caliper.from_extra,
                            0.0,
                            1000.0,
                            &format!("{path}.caliper.from_extra"),
                        )?;
                        range(
                            caliper.lock_fraction,
                            0.2,
                            0.98,
                            &format!("{path}.caliper.lock_fraction"),
                        )?;
                        range(
                            caliper.jaw_size,
                            8.0,
                            120.0,
                            &format!("{path}.caliper.jaw_size"),
                        )?;
                        if caliper.tick_count > 15 || caliper.precision > 2 {
                            return Err(CaptionError::invalid(
                                format!("{path}.caliper"),
                                "tick_count must be 0–15; precision must be 0–2",
                            ));
                        }
                        if caliper.unit.trim().is_empty() || caliper.unit.chars().count() > 16 {
                            return Err(CaptionError::invalid(
                                format!("{path}.caliper.unit"),
                                "must contain 1–16 characters",
                            ));
                        }
                    }
                    PreparedKind::Measure(prepare_measure(g))
                }
            };
            items.push(PreparedGraphic {
                timing: timing.clone(),
                hold_until,
                kind,
            });
        }
        Ok(Self { items })
    }

    pub fn render(&self, layer: Layer, time: f64) -> Svgr<'_> {
        self.items
            .iter()
            .filter(|item| item.timing.layer == layer)
            .map(|item| item.render(time))
            .collect()
    }
}

impl PreparedGraphic {
    fn render(&self, time: f64) -> Svgr<'_> {
        if time < self.timing.start || time >= self.hold_until + self.timing.fade_out {
            return Svgr::empty();
        }
        let elapsed = time - self.timing.start;
        let linear_progress = (elapsed / self.timing.duration).clamp(0.0, 1.0);
        let progress = smooth(linear_progress);
        let opacity = self.timing.opacity
            * (1.0 - smooth(((time - self.hold_until) / self.timing.fade_out).clamp(0.0, 1.0)));
        let content = match &self.kind {
            PreparedKind::Path(path) => path.render(progress, elapsed),
            PreparedKind::Label(label) => svgr!(<g opacity={progress}>
                <text x={label.x} y={label.y} font-family={label.font_family.as_str()}
                    font-size={label.font_size} font-weight={label.font_weight}
                    text-anchor={label.align.svg()} fill={label.color.as_str()}
                    stroke="#050a10" stroke-width="1.4" stroke-linejoin="round" paint-order="stroke fill">{label.text.as_str()}</text>
            </g>),
            PreparedKind::Measure(measure) => measure.render(progress, linear_progress),
        };
        svgr!(<g opacity={opacity}>{content}</g>)
    }
}

impl PreparedPath {
    fn render<'a>(&'a self, progress: f64, elapsed: f64) -> Svgr<'a> {
        let g = &self.graphic;
        let line = draw_supported(&self.geometry, &g.color, g.width, progress, g.keyline);
        let arrow = if g.arrow && progress > 0.01 {
            let (point, tangent) = self.geometry.at(progress);
            let normal = [-tangent[1], tangent[0]];
            let size = 7.0 + 2.0 * g.width;
            let left = [
                point[0] - tangent[0] * size + normal[0] * size * 0.45,
                point[1] - tangent[1] * size + normal[1] * size * 0.45,
            ];
            let right = [
                point[0] - tangent[0] * size - normal[0] * size * 0.45,
                point[1] - tangent[1] * size - normal[1] * size * 0.45,
            ];
            let data = format!(
                "M{},{} L{},{} L{},{}",
                left[0], left[1], point[0], point[1], right[0], right[1]
            );
            let keyline = if g.keyline {
                svgr!(<path d={data.clone()} fill="none" stroke="#050a10" stroke-width={g.width + 3.0}
                    stroke-linecap="round" stroke-linejoin="round" opacity="0.65" />)
            } else {
                Svgr::empty()
            };
            svgr!(<g>{keyline}<path d={data}
                fill="none" stroke={g.color.as_str()} stroke-width={g.width} stroke-linecap="round" stroke-linejoin="round" /></g>)
        } else {
            Svgr::empty()
        };
        let flow = g.flow.as_ref().map(|flow| {
            let color = flow.color.as_deref().unwrap_or(g.color.as_str());
            (0..flow.count).map(|index| {
                let phase = (elapsed * flow.speed + flow.phase + index as f64 / flow.count as f64).rem_euclid(1.0);
                if phase > progress { return Svgr::empty(); }
                let (point, _) = self.geometry.at(phase);
                let intensity = (std::f64::consts::PI * phase).sin().max(0.0).sqrt();
                let tail_length = (self.geometry.length * flow.tail).max(0.1);
                svgr!(<g opacity={intensity}>
                    <path d={self.geometry.data.as_str()} fill="none" stroke={color}
                        stroke-width={g.width * 1.65} stroke-linecap="round"
                        stroke-dasharray={format!("{tail_length} {}", self.geometry.length * 2.0)}
                        stroke-dashoffset={tail_length - phase * self.geometry.length} opacity="0.65" />
                    <circle cx={point[0]} cy={point[1]} r={flow.radius * 2.8} fill={color} opacity="0.13" />
                    <circle cx={point[0]} cy={point[1]} r={flow.radius} fill={color} />
                </g>)
            }).collect::<Svgr<'a>>()
        }).unwrap_or_default();
        let Some(trace) = &g.trace else {
            return svgr!(<g>{line}{arrow}{flow}</g>);
        };
        let front = self.render_trace(trace, progress, elapsed);
        let construction = self.render_construction(trace, progress);
        svgr!(<g>{construction}{line}{arrow}{flow}{front}</g>)
    }

    fn render_trace<'a>(&'a self, trace: &'a Trace, progress: f64, elapsed: f64) -> Svgr<'a> {
        let arrived = ((elapsed - self.graphic.timing.duration) / trace.linger).clamp(0.0, 1.0);
        if progress <= 0.0 || arrived >= 1.0 {
            return Svgr::empty();
        }
        let color = trace
            .color
            .as_deref()
            .unwrap_or(self.graphic.color.as_str());
        let (head, _) = self.geometry.at(progress);
        let opacity = smooth((progress / 0.04).clamp(0.0, 1.0)) * (1.0 - smooth(arrived));
        let tail_length = self.geometry.length * trace.tail;
        let tail = if tail_length > 0.0 {
            let dash = format!("{tail_length} {}", self.geometry.length * 2.0);
            let offset = tail_length - progress * self.geometry.length;
            svgr!(<g>
                <path d={self.geometry.data.as_str()} fill="none" stroke={color}
                    stroke-width={self.graphic.width * 3.5} stroke-linecap="round"
                    stroke-dasharray={dash.clone()} stroke-dashoffset={offset} opacity="0.13" />
                <path d={self.geometry.data.as_str()} fill="none" stroke={color}
                    stroke-width={self.graphic.width * 1.55} stroke-linecap="round"
                    stroke-dasharray={dash} stroke-dashoffset={offset} opacity="0.85" />
            </g>)
        } else {
            Svgr::empty()
        };
        let ring = if trace.arrival_ring && arrived > 0.0 {
            svgr!(<circle cx={head[0]} cy={head[1]}
                r={trace.head_radius * (1.4 + 3.2 * smooth(arrived))}
                fill="none" stroke={color} stroke-width={self.graphic.width}
                opacity={0.8 * (1.0 - arrived)} />)
        } else {
            Svgr::empty()
        };
        let support = if self.graphic.keyline {
            svgr!(<circle cx={head[0]} cy={head[1]} r={trace.head_radius + 2.0}
                fill="#050a10" opacity="0.8" />)
        } else {
            Svgr::empty()
        };
        svgr!(<g opacity={opacity}>
            {tail}{ring}
            <circle cx={head[0]} cy={head[1]} r={trace.head_radius * 3.0} fill={color} opacity="0.12" />
            {support}
            <circle cx={head[0]} cy={head[1]} r={trace.head_radius} fill={color} />
            <circle cx={head[0]} cy={head[1]} r={trace.head_radius * 0.4} fill="#ffffff" opacity="0.95" />
        </g>)
    }

    fn render_construction<'a>(&'a self, trace: &'a Trace, progress: f64) -> Svgr<'a> {
        if !trace.construction || progress <= 0.0 || progress >= 0.97 {
            return Svgr::empty();
        }
        let t = self.geometry.cubic_parameter_at(progress);
        let construction = cubic_construction(&self.graphic.points, t);
        let opacity = trace.construction_opacity
            * smooth((progress / 0.08).clamp(0.0, 1.0))
            * (1.0 - smooth(((progress - 0.65) / 0.32).clamp(0.0, 1.0)));
        let color = trace
            .color
            .as_deref()
            .unwrap_or(self.graphic.color.as_str());
        let nodes = construction
            .level1
            .iter()
            .chain(construction.level2.iter())
            .map(|p| {
                svgr!(<circle cx={p[0]} cy={p[1]} r={self.graphic.width.max(2.0)}
                fill="#050a10" stroke={color} stroke-width={self.graphic.width * 0.65} />)
            })
            .collect::<Svgr<'a>>();
        svgr!(<g opacity={opacity}>
            <path d={polyline_data(&self.graphic.points)} fill="none" stroke={color}
                stroke-width={self.graphic.width * 0.65} opacity="0.25" />
            <path d={polyline_data(&construction.level1)} fill="none" stroke={color}
                stroke-width={self.graphic.width * 0.8} opacity="0.6" />
            <path d={polyline_data(&construction.level2)} fill="none" stroke={color}
                stroke-width={self.graphic.width} />
            {nodes}
            <circle cx={construction.point[0]} cy={construction.point[1]}
                r={self.graphic.width} fill={color} />
        </g>)
    }
}

struct CubicConstruction {
    level1: [Point; 3],
    level2: [Point; 2],
    point: Point,
}

fn cubic_construction(points: &[Point], t: f64) -> CubicConstruction {
    let level1 = [0, 1, 2].map(|i| lerp(points[i], points[i + 1], t));
    let level2 = [0, 1].map(|i| lerp(level1[i], level1[i + 1], t));
    CubicConstruction {
        level1,
        level2,
        point: lerp(level2[0], level2[1], t),
    }
}

fn prepare_measure(g: &MeasureGraphic) -> PreparedMeasure {
    let tangent = unit([g.to[0] - g.from[0], g.to[1] - g.from[1]]);
    let normal = [-tangent[1], tangent[0]];
    let shifted =
        |point: Point, offset: f64| [point[0] + normal[0] * offset, point[1] + normal[1] * offset];
    let a = shifted(g.from, g.offset);
    let b = shifted(g.to, g.offset);
    let beyond = g.offset + 7.0 * if g.offset < 0.0 { -1.0 } else { 1.0 };
    let ea = shifted(g.from, beyond);
    let eb = shifted(g.to, beyond);
    let extensions = format!(
        "M{},{} L{},{} M{},{} L{},{}",
        g.from[0], g.from[1], ea[0], ea[1], g.to[0], g.to[1], eb[0], eb[1]
    );
    let tick_a = shifted(a, -5.0);
    let tick_b = shifted(a, 5.0);
    let tick_c = shifted(b, -5.0);
    let tick_d = shifted(b, 5.0);
    let ticks = format!(
        "M{},{} L{},{} M{},{} L{},{}",
        tick_a[0], tick_a[1], tick_b[0], tick_b[1], tick_c[0], tick_c[1], tick_d[0], tick_d[1]
    );
    let mut angle = tangent[1].atan2(tangent[0]).to_degrees();
    if angle > 90.0 {
        angle -= 180.0;
    }
    if angle < -90.0 {
        angle += 180.0;
    }
    let mut caliper_ticks = String::new();
    let caliper_rail = if let Some(caliper) = &g.caliper {
        let (fixed, moving, direction) = match caliper.moving_end {
            MovingEnd::To => (a, b, 1.0),
            MovingEnd::From => (b, a, -1.0),
        };
        let extended = translate(moving, tangent, direction * caliper.from_extra);
        for i in 0..caliper.tick_count {
            let fraction = (i + 1) as f64 / (caliper.tick_count + 1) as f64;
            let p = lerp(moving, extended, fraction);
            let extent = if i % 2 == 0 { 7.0 } else { 4.0 };
            append_segment(
                &mut caliper_ticks,
                translate(p, normal, -extent),
                translate(p, normal, extent),
            );
        }
        polyline_data(&[fixed, extended])
    } else {
        String::new()
    };
    PreparedMeasure {
        graphic: g.clone(),
        geometry: Geometry::new(&[a, b], false),
        extensions,
        ticks,
        center: [f64::midpoint(a[0], b[0]), f64::midpoint(a[1], b[1])],
        angle,
        from: a,
        to: b,
        tangent,
        normal,
        caliper_rail,
        caliper_ticks,
    }
}

impl PreparedMeasure {
    fn render(&self, progress: f64, linear_progress: f64) -> Svgr<'_> {
        if let Some(caliper) = &self.graphic.caliper {
            return self.render_caliper(caliper, linear_progress);
        }
        let g = &self.graphic;
        let finish = smooth(((progress - 0.55) / 0.45).clamp(0.0, 1.0));
        let line = draw_supported(&self.geometry, &g.color, g.width, progress, g.keyline);
        let support = if g.keyline {
            svgr!(<g opacity="0.65">
                <path d={self.extensions.as_str()} fill="none" stroke="#050a10"
                    stroke-width={g.width * 0.7 + 3.0} opacity={progress * 0.6} />
                <path d={self.ticks.as_str()} fill="none" stroke="#050a10"
                    stroke-width={g.width + 3.0} opacity={finish} />
            </g>)
        } else {
            Svgr::empty()
        };
        svgr!(<g>
            {support}
            <path d={self.extensions.as_str()} fill="none" stroke={g.color.as_str()}
                stroke-width={g.width * 0.7} opacity={progress * 0.6} />
            {line}
            <path d={self.ticks.as_str()} fill="none" stroke={g.color.as_str()} stroke-width={g.width} opacity={finish} />
            <g opacity={finish} transform={format!("translate({} {}) rotate({})", self.center[0], self.center[1], self.angle)}>
                <text x="0" y="-9" text-anchor="middle" font-family={g.font_family.as_str()}
                    font-size={g.font_size} font-weight="500" fill={g.color.as_str()}
                    stroke="#050a10" stroke-width="1.4" stroke-linejoin="round" paint-order="stroke fill">{g.label.as_str()}</text>
            </g>
        </g>)
    }

    fn caliper_pose(&self, caliper: &Caliper, linear_progress: f64) -> CaliperPose {
        let close = smooth((linear_progress / caliper.lock_fraction).clamp(0.0, 1.0));
        let extra = caliper.from_extra * (1.0 - close);
        let (from, to) = match caliper.moving_end {
            MovingEnd::To => (self.from, translate(self.to, self.tangent, extra)),
            MovingEnd::From => (translate(self.from, self.tangent, -extra), self.to),
        };
        CaliperPose {
            from,
            to,
            aperture: self.geometry.length + extra,
            locked: linear_progress >= caliper.lock_fraction,
        }
    }

    fn render_caliper<'a>(&'a self, caliper: &'a Caliper, linear_progress: f64) -> Svgr<'a> {
        if linear_progress <= 0.0 {
            return Svgr::empty();
        }
        let g = &self.graphic;
        let pose = self.caliper_pose(caliper, linear_progress);
        let side = if g.offset < 0.0 { -1.0 } else { 1.0 };
        let mut jaws = String::new();
        for (endpoint, inward) in [(pose.from, 1.0), (pose.to, -1.0)] {
            let outer = translate(endpoint, self.normal, side * 9.0);
            let inner = translate(endpoint, self.normal, -side * caliper.jaw_size);
            append_segment(&mut jaws, outer, inner);
            append_segment(
                &mut jaws,
                inner,
                translate(inner, self.tangent, inward * 12.0),
            );
        }
        let span = polyline_data(&[pose.from, pose.to]);
        let label = if pose.locked {
            g.label.clone()
        } else {
            format!("{:.*} {}", caliper.precision, pose.aperture, caliper.unit)
        };
        let label_center = lerp(pose.from, pose.to, 0.5);
        let visible = smooth((linear_progress / 0.1).clamp(0.0, 1.0));
        let support = if g.keyline {
            svgr!(<g fill="none" stroke="#050a10" stroke-linecap="round" stroke-linejoin="round" opacity="0.8">
                <path d={span.clone()} stroke-width={g.width + 3.0} />
                <path d={jaws.clone()} stroke-width={g.width * 1.6 + 3.0} />
            </g>)
        } else {
            Svgr::empty()
        };
        // A small contact ring is a single confirmation, never a repeating pulse.
        let confirm = ((linear_progress - caliper.lock_fraction) / (1.0 - caliper.lock_fraction))
            .clamp(0.0, 1.0);
        let contact = if pose.locked && confirm < 1.0 {
            let endpoint = match caliper.moving_end {
                MovingEnd::To => pose.to,
                MovingEnd::From => pose.from,
            };
            svgr!(<circle cx={endpoint[0]} cy={endpoint[1]} r={4.0 + 12.0 * smooth(confirm)}
                fill="none" stroke={g.color.as_str()} stroke-width={g.width}
                opacity={0.8 * (1.0 - confirm)} />)
        } else {
            Svgr::empty()
        };
        svgr!(<g opacity={visible}>
            <path d={self.caliper_rail.as_str()} fill="none" stroke={g.color.as_str()}
                stroke-width={g.width * 0.65} opacity="0.3" />
            <path d={self.caliper_ticks.as_str()} fill="none" stroke={g.color.as_str()}
                stroke-width={g.width * 0.7} opacity="0.55" />
            {support}
            <path d={span} fill="none" stroke={g.color.as_str()} stroke-width={g.width}
                stroke-linecap="round" opacity="0.8" />
            <path d={jaws} fill="none" stroke={g.color.as_str()} stroke-width={g.width * 1.6}
                stroke-linecap="round" stroke-linejoin="round" />
            {contact}
            <g transform={format!("translate({} {}) rotate({})", label_center[0], label_center[1], self.angle)}>
                <text x="0" y={-11.0 * side} text-anchor="middle" font-family={g.font_family.as_str()}
                    font-size={g.font_size} font-weight="500" fill={g.color.as_str()}
                    stroke="#050a10" stroke-width="2.4" stroke-linejoin="round" paint-order="stroke fill">{label}</text>
            </g>
        </g>)
    }
}

#[derive(Debug)]
struct CaliperPose {
    from: Point,
    to: Point,
    aperture: f64,
    locked: bool,
}

fn draw_supported<'a>(
    geometry: &'a Geometry,
    color: &'a str,
    width: f64,
    progress: f64,
    keyline: bool,
) -> Svgr<'a> {
    let line = draw(geometry, color, width, progress);
    if !keyline {
        return line;
    }
    let support = draw(geometry, "#050a10", width + 3.0, progress);
    svgr!(<g><g opacity="0.65">{support}</g>{line}</g>)
}

fn draw<'a>(geometry: &'a Geometry, color: &'a str, width: f64, progress: f64) -> Svgr<'a> {
    if progress <= 0.0 {
        return Svgr::empty();
    }
    svgr!(<path d={geometry.data.as_str()} fill="none" stroke={color} stroke-width={width}
        stroke-linecap="round" stroke-linejoin="round"
        stroke-dasharray={format!("{} {}", geometry.length, geometry.length)}
        stroke-dashoffset={geometry.length * (1.0 - progress)} />)
}

fn smooth(value: f64) -> f64 {
    value * value * (3.0 - 2.0 * value)
}
fn lerp(a: Point, b: Point, t: f64) -> Point {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}
fn translate(point: Point, direction: Point, distance: f64) -> Point {
    [
        point[0] + direction[0] * distance,
        point[1] + direction[1] * distance,
    ]
}
fn polyline_data(points: &[Point]) -> String {
    let mut data = format!("M{},{}", points[0][0], points[0][1]);
    for point in &points[1..] {
        let _ = write!(data, " L{},{}", point[0], point[1]);
    }
    data
}
fn append_segment(data: &mut String, from: Point, to: Point) {
    let _ = write!(data, " M{},{} L{},{}", from[0], from[1], to[0], to[1]);
}
fn distance(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}
fn unit(point: Point) -> Point {
    let length = point[0].hypot(point[1]);
    if length > 1e-9 {
        [point[0] / length, point[1] / length]
    } else {
        [1.0, 0.0]
    }
}
fn range(value: f64, min: f64, max: f64, path: &str) -> Result<(), CaptionError> {
    if !value.is_finite() || value < min || value > max {
        return Err(CaptionError::invalid(
            path,
            format!("must be finite and between {min} and {max}"),
        ));
    }
    Ok(())
}
fn validate_point(point: Point, path: &str) -> Result<(), CaptionError> {
    range(point[0], -3840.0, 5760.0, &format!("{path}[0]"))?;
    range(point[1], -2160.0, 3240.0, &format!("{path}[1]"))
}
fn color(value: &str, path: &str) -> Result<(), CaptionError> {
    if value.trim().is_empty() {
        return Err(CaptionError::invalid(path, "color cannot be empty"));
    }
    Ok(())
}
fn text(value: &str, family: &str, size: usize, path: &str) -> Result<(), CaptionError> {
    if value.trim().is_empty() || family.trim().is_empty() || !(8..=120).contains(&size) {
        return Err(CaptionError::invalid(
            path,
            "text and font family must be nonempty; font size must be 8–120 pixels",
        ));
    }
    Ok(())
}
