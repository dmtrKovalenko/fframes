//! Lottie animations (feature `lottie`).
//!
//! [velato](https://github.com/linebender/velato) parses a Lottie JSON file and evaluates it at
//! a time. This module writes each frame as an `svgr!` tree. The CPU backend, the Skia backend
//! and the editor draw it as vector graphics.
//!
//! velato does not support text layers, image layers, time remapping, blend modes, effects or
//! expressions. Those parts of a file are not drawn.

use std::sync::atomic::{AtomicU64, Ordering};

use kurbo::{Affine, Cap, Join, Shape};
use peniko::{BlendMode, Brush, Compose, Gradient, GradientKind};
use velato::{RenderSink, Renderer, model::ImageAsset, model::fixed};

use crate::{Duration, FFramesContext, Frame, Svgr, svgr};

pub use velato::Error as LottieError;

/// A parsed Lottie animation. Put `.json` files into the media folder (`include_media_dir!` or
/// `MediaDirectory`) and play them with [`Frame::play_lottie`]. `include_media_dir!` also makes
/// each file a field of the media struct.
#[derive(Debug, Clone)]
pub struct Lottie {
    composition: velato::Composition,
}

impl Lottie {
    /// Parses a Lottie JSON file.
    pub fn parse(json: impl AsRef<[u8]>) -> Result<Self, LottieError> {
        let mut json: serde_json::Value =
            serde_json::from_slice(json.as_ref()).map_err(LottieError::from)?;
        upgrade_old_format(&mut json);
        Ok(Self {
            composition: velato::Composition::from_json(json)?,
        })
    }

    /// Width of the animation in its own units.
    pub fn width(&self) -> usize {
        self.composition.width
    }

    /// Height of the animation in its own units.
    pub fn height(&self) -> usize {
        self.composition.height
    }

    /// Length of the animation in seconds.
    pub fn seconds(&self) -> f32 {
        let frames = &self.composition.frames;
        ((frames.end - frames.start) / self.composition.frame_rate) as f32
    }

    /// Length of the animation, for `Video::duration` or `Scene::duration`.
    pub fn duration(&self) -> Duration<'static> {
        Duration::Seconds(self.seconds())
    }

    /// The animation at `seconds` from its start, in its own coordinates (`0..width`,
    /// `0..height`). After the end it stays on the last frame. [`Frame::play_lottie`] also sets
    /// the time and the position.
    pub fn render_at(&self, seconds: f32) -> Svgr<'static> {
        let frames = &self.composition.frames;
        // A layer is visible in `in..out` without `out`. The last frame is just before `out`.
        let lottie_frame = (frames.start + f64::from(seconds) * self.composition.frame_rate)
            .clamp(frames.start, frames.end - 0.001);

        let mut sink = SvgrSink::new();
        Renderer::new().append(
            &self.composition,
            lottie_frame,
            Affine::IDENTITY,
            1.0,
            &mut sink,
        );
        sink.finish()
    }
}

/// How [`Frame::play_lottie`] plays and places an animation.
#[derive(Debug, Clone, Copy)]
pub struct LottieOptions {
    /// Second of the video (or of the scene) when the animation starts. Nothing is drawn before.
    pub start: f32,
    /// Playback speed, `2.0` plays twice as fast.
    pub speed: f32,
    /// Starts again at the end. Otherwise the last frame stays.
    pub looping: bool,
    /// Left edge of the animation.
    pub x: f32,
    /// Top edge of the animation.
    pub y: f32,
    /// Width to scale to. With `height` too, the animation fits into that box, centered. Without
    /// both, it keeps the size of the file.
    pub width: Option<f32>,
    /// Height to scale to, see `width`.
    pub height: Option<f32>,
}

impl Default for LottieOptions {
    fn default() -> Self {
        Self {
            start: 0.0,
            speed: 1.0,
            looping: false,
            x: 0.0,
            y: 0.0,
            width: None,
            height: None,
        }
    }
}

impl LottieOptions {
    /// Time in the animation at `video_seconds`, `None` before the start.
    fn time(&self, video_seconds: f32, length: f32) -> Option<f32> {
        let seconds = (video_seconds - self.start) * self.speed;
        if seconds < 0.0 {
            None
        } else if self.looping {
            Some(seconds.rem_euclid(length))
        } else {
            Some(seconds)
        }
    }

    /// Moves and scales an animation of `width` x `height` into place.
    fn transform(&self, width: f32, height: f32) -> String {
        let scale = match (self.width, self.height) {
            (Some(box_width), Some(box_height)) => (box_width / width).min(box_height / height),
            (Some(box_width), None) => box_width / width,
            (None, Some(box_height)) => box_height / height,
            (None, None) => 1.0,
        };
        let x = self.x + self.width.map_or(0.0, |w| (w - width * scale) / 2.0);
        let y = self.y + self.height.map_or(0.0, |h| (h - height * scale) / 2.0);
        format!("translate({x} {y}) scale({scale})")
    }
}

impl Frame {
    /// Plays the Lottie file `file_name` from the media folder at the time of this frame.
    /// Returns an empty element before `options.start`. A missing file also gives an empty
    /// element, and `inspect` reports it.
    ///
    /// ```rust,ignore
    /// let confetti = frame.play_lottie(ctx, "confetti.json", LottieOptions {
    ///     start: 1.5,
    ///     x: 660.0,
    ///     y: 240.0,
    ///     width: Some(600.0),
    ///     ..Default::default()
    /// });
    /// svgr!(<svg ...>{confetti}</svg>)
    /// ```
    pub fn play_lottie(
        &self,
        ctx: &FFramesContext<'_, '_>,
        file_name: impl AsRef<str>,
        options: LottieOptions,
    ) -> Svgr<'static> {
        let Some(lottie) = ctx.get_lottie(file_name) else {
            return Svgr::empty();
        };
        let Some(seconds) = options.time(self.seconds(), lottie.seconds()) else {
            return Svgr::empty();
        };
        let transform = options.transform(lottie.width() as f32, lottie.height() as f32);

        svgr!(<g transform={transform}>{lottie.render_at(seconds)}</g>)
    }
}

enum Layer {
    /// velato pushes a layer for opacity and for track mattes. It does not draw blend modes.
    Group {
        opacity: f32,
    },
    Clip {
        path: String,
        transform: String,
    },
    /// velato draws a track matte in two layers: a `Normal` layer with the matte, then a `SrcIn`
    /// layer with the content (`SrcOut` for an inverted matte). In SVG this is a `<mask>`.
    Matte {
        inverted: bool,
    },
}

/// Writes the draw calls of velato as `svgr!` nodes. Each open layer keeps its children.
struct SvgrSink {
    stack: Vec<(Layer, Vec<Svgr<'static>>)>,
    id_prefix: String,
    next_id: usize,
}

impl SvgrSink {
    fn new() -> Self {
        // Ids must be unique in the frame, also when the frame shows more than one animation.
        static NEXT_RENDER: AtomicU64 = AtomicU64::new(0);
        let render = NEXT_RENDER.fetch_add(1, Ordering::Relaxed);

        Self {
            stack: vec![(Layer::Group { opacity: 1.0 }, Vec::new())],
            id_prefix: format!("lottie{render}-"),
            next_id: 0,
        }
    }

    fn new_id(&mut self) -> String {
        self.next_id += 1;
        format!("{}{}", self.id_prefix, self.next_id)
    }

    fn children(&mut self) -> &mut Vec<Svgr<'static>> {
        &mut self
            .stack
            .last_mut()
            .expect("the root layer is never popped")
            .1
    }

    fn finish(mut self) -> Svgr<'static> {
        while self.stack.len() > 1 {
            self.pop_layer();
        }
        let children = self
            .stack
            .pop()
            .map(|(_, children)| children)
            .unwrap_or_default();
        svgr!(<g>{children}</g>)
    }

    /// The SVG paint of a brush: a color and its opacity, or a gradient and its definition.
    fn paint(&mut self, brush: &Brush) -> (String, f32, Svgr<'static>) {
        match brush {
            Brush::Solid(color) => {
                let [r, g, b, a] = color.to_rgba8().to_u8_array();
                (
                    format!("#{r:02x}{g:02x}{b:02x}"),
                    f32::from(a) / 255.0,
                    Svgr::empty(),
                )
            }
            Brush::Gradient(gradient) => {
                let id = self.new_id();
                let paint = format!("url(#{id})");
                (paint, 1.0, gradient_definition(id, gradient))
            }
            Brush::Image(_) => ("none".to_owned(), 0.0, Svgr::empty()),
        }
    }
}

impl RenderSink for SvgrSink {
    fn push_layer(
        &mut self,
        blend: impl Into<BlendMode>,
        alpha: f32,
        _transform: Affine,
        _shape: &impl Shape,
    ) {
        let layer = match blend.into().compose {
            Compose::SrcIn => Layer::Matte { inverted: false },
            Compose::SrcOut => Layer::Matte { inverted: true },
            _ => Layer::Group { opacity: alpha },
        };
        self.stack.push((layer, Vec::new()));
    }

    fn push_clip_layer(&mut self, transform: Affine, shape: &impl Shape) {
        let path = shape.to_path(0.1);
        // A degenerate mask has NaN coordinates. It has no area, so it hides its content.
        let layer = if path.is_finite() && transform.is_finite() {
            Layer::Clip {
                path: path.to_svg(),
                transform: svg_transform(transform),
            }
        } else {
            Layer::Group { opacity: 0.0 }
        };
        self.stack.push((layer, Vec::new()));
    }

    fn pop_layer(&mut self) {
        // The root layer stays, `finish` takes its children.
        if self.stack.len() < 2 {
            return;
        }
        let Some((layer, children)) = self.stack.pop() else {
            return;
        };

        let node = match layer {
            Layer::Group { opacity } if opacity <= 0.0 => return,
            Layer::Group { opacity } if opacity >= 1.0 => svgr!(<g>{children}</g>),
            Layer::Group { opacity } => svgr!(<g opacity={opacity}>{children}</g>),
            Layer::Clip { path, transform } => {
                let id = self.new_id();
                let url = format!("url(#{id})");
                svgr!(
                    <g>
                        <clipPath id={id}><path d={path} transform={transform} /></clipPath>
                        <g clip-path={url}>{children}</g>
                    </g>
                )
            }
            Layer::Matte { inverted } => {
                // The parent layer holds the matte. The matte becomes the mask of the content.
                let matte = std::mem::take(self.children());
                let id = self.new_id();
                let url = format!("url(#{id})");
                let mask = if inverted {
                    // White shows the content. The filter paints the matte black on top.
                    let filter_id = self.new_id();
                    let filter_url = format!("url(#{filter_id})");
                    svgr!(
                        <mask id={id}>
                            <filter id={filter_id} color-interpolation-filters="sRGB">
                                <feColorMatrix type="matrix" values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 1 0" />
                            </filter>
                            <rect x="-100000" y="-100000" width="200000" height="200000" fill="#fff" />
                            <g filter={filter_url}>{matte}</g>
                        </mask>
                    )
                } else {
                    svgr!(<mask id={id} mask-type="alpha">{matte}</mask>)
                };
                svgr!(<g>{mask}<g mask={url}>{children}</g></g>)
            }
        };
        self.children().push(node);
    }

    fn draw(
        &mut self,
        stroke: Option<&fixed::Stroke>,
        transform: Affine,
        brush: &fixed::Brush,
        shape: &impl Shape,
    ) {
        let path = shape.to_path(0.1);
        // velato gives NaN coordinates to degenerate shapes, for example a shape of zero size.
        if !path.is_finite() || !transform.is_finite() {
            return;
        }
        let d = path.to_svg();
        let transform = svg_transform(transform);
        let (paint, opacity, definition) = self.paint(brush);

        let node = match stroke {
            None => svgr!(
                <g>
                    {definition}
                    <path d={d} transform={transform} fill={paint} fill-opacity={opacity} />
                </g>
            ),
            Some(stroke) => {
                let dash = if stroke.dash_pattern.is_empty() {
                    "none".to_owned()
                } else {
                    let values: Vec<String> =
                        stroke.dash_pattern.iter().map(f64::to_string).collect();
                    values.join(" ")
                };
                let cap = match stroke.start_cap {
                    Cap::Butt => "butt",
                    Cap::Round => "round",
                    Cap::Square => "square",
                };
                let join = match stroke.join {
                    Join::Bevel => "bevel",
                    Join::Miter => "miter",
                    Join::Round => "round",
                };
                svgr!(
                    <g>
                        {definition}
                        <path
                            d={d}
                            transform={transform}
                            fill="none"
                            stroke={paint}
                            stroke-opacity={opacity}
                            stroke-width={stroke.width}
                            stroke-linecap={cap}
                            stroke-linejoin={join}
                            stroke-miterlimit={stroke.miter_limit}
                            stroke-dasharray={dash}
                            stroke-dashoffset={stroke.dash_offset}
                        />
                    </g>
                )
            }
        };
        self.children().push(node);
    }

    // Image layers are not supported.
    fn draw_image(&mut self, _image: &ImageAsset, _transform: Affine, _alpha: f64) {}
}

/// Changes the JSON of old files to the format that velato reads:
/// - Before bodymovin 5.5, a keyframe gives its end value in `e`, and the last keyframe has no
///   `s`. velato needs `s` on every keyframe, and as an array.
/// - bodymovin 4 gives fill and stroke colors in `0..255`. velato needs `0..1`.
/// - velato does not draw solid layers. They become shape layers with a rectangle.
fn upgrade_old_format(value: &mut serde_json::Value) {
    use serde_json::{Value, json};

    match value {
        Value::Object(object) => {
            if object.get("ty").and_then(Value::as_u64) == Some(1)
                && let Some(color) = object.get("sc").and_then(Value::as_str)
            {
                let hex = u32::from_str_radix(color.trim_start_matches('#'), 16).unwrap_or(0);
                let [_, r, g, b] = hex.to_be_bytes().map(|c| f64::from(c) / 255.0);
                let width = object.get("sw").and_then(Value::as_f64).unwrap_or(0.0);
                let height = object.get("sh").and_then(Value::as_f64).unwrap_or(0.0);
                object.insert("ty".to_owned(), json!(4));
                object.insert(
                    "shapes".to_owned(),
                    json!([
                        { "ty": "rc", "d": 1, "p": { "a": 0, "k": [width / 2.0, height / 2.0] },
                          "s": { "a": 0, "k": [width, height] }, "r": { "a": 0, "k": 0 } },
                        { "ty": "fl", "c": { "a": 0, "k": [r, g, b, 1] },
                          "o": { "a": 0, "k": 100 }, "r": 1 },
                    ]),
                );
            }
            if matches!(object.get("ty").and_then(Value::as_str), Some("fl" | "st"))
                && let Some(color) = object.get_mut("c").and_then(|color| color.get_mut("k"))
            {
                normalize_color(color);
            }
            if let Some(Value::Array(keyframes)) = object.get_mut("k")
                && keyframes.iter().all(|keyframe| keyframe.get("t").is_some())
            {
                let mut previous_end = None;
                for keyframe in keyframes.iter_mut().filter_map(Value::as_object_mut) {
                    for key in ["s", "e"] {
                        if let Some(number @ Value::Number(_)) = keyframe.get_mut(key) {
                            *number = Value::Array(vec![number.take()]);
                        }
                    }
                    if !keyframe.contains_key("s")
                        && let Some(end) = previous_end.take()
                    {
                        keyframe.insert("s".to_owned(), end);
                    }
                    previous_end = keyframe.get("e").or_else(|| keyframe.get("s")).cloned();
                }
            }
            object.values_mut().for_each(upgrade_old_format);
        }
        Value::Array(values) => values.iter_mut().for_each(upgrade_old_format),
        _ => {}
    }
}

/// Divides a static or animated color by 255 when one of its values is above 1.
fn normalize_color(color: &mut serde_json::Value) {
    use serde_json::Value;

    // A static color is an array of numbers. An animated color has its values in `s` and `e`.
    let mut values: Vec<&mut Value> = match color {
        Value::Array(items) if items.iter().all(Value::is_number) => vec![color],
        Value::Array(keyframes) => keyframes
            .iter_mut()
            .filter_map(Value::as_object_mut)
            .flat_map(|keyframe| keyframe.iter_mut())
            .filter(|(key, _)| *key == "s" || *key == "e")
            .map(|(_, value)| value)
            .collect(),
        _ => return,
    };
    let numbers = |value: &mut Value| -> Vec<f64> {
        value.as_array().map_or_else(Vec::new, |items| {
            items.iter().filter_map(Value::as_f64).collect()
        })
    };
    if !values
        .iter_mut()
        .any(|value| numbers(value).iter().any(|n| *n > 1.0))
    {
        return;
    }
    for value in values {
        let scaled = numbers(value)
            .iter()
            .map(|n| Value::from(n / 255.0))
            .collect();
        *value = Value::Array(scaled);
    }
}

fn svg_transform(transform: Affine) -> String {
    let [a, b, c, d, e, f] = transform.as_coeffs();
    format!("matrix({a} {b} {c} {d} {e} {f})")
}

fn gradient_definition(id: String, gradient: &Gradient) -> Svgr<'static> {
    let stops: Vec<Svgr<'static>> = gradient
        .stops
        .iter()
        .map(|stop| {
            let [r, g, b, a] = stop
                .color
                .to_alpha_color::<peniko::color::Srgb>()
                .to_rgba8()
                .to_u8_array();
            let color = format!("#{r:02x}{g:02x}{b:02x}");
            let opacity = f32::from(a) / 255.0;
            svgr!(<stop offset={stop.offset} stop-color={color} stop-opacity={opacity} />)
        })
        .collect();

    match gradient.kind {
        GradientKind::Radial(radial) => svgr!(
            <radialGradient
                id={id}
                gradientUnits="userSpaceOnUse"
                cx={radial.end_center.x}
                cy={radial.end_center.y}
                r={radial.end_radius}
                fx={radial.start_center.x}
                fy={radial.start_center.y}
            >
                {stops}
            </radialGradient>
        ),
        GradientKind::Linear(linear) => svgr!(
            <linearGradient
                id={id}
                gradientUnits="userSpaceOnUse"
                x1={linear.start.x}
                y1={linear.start.y}
                x2={linear.end.x}
                y2={linear.end.y}
            >
                {stops}
            </linearGradient>
        ),
        // Lottie has no sweep gradients. velato makes only linear and radial ones.
        GradientKind::Sweep(_) => svgr!(<linearGradient id={id}>{stops}</linearGradient>),
    }
}
