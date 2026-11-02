//! Actual glyph deformation using a cached, padded texture from `FFrames`' SVG renderer.
//! Six legacy treatments, sixteen lit materials and fifteen distinct showcase techniques.
//! Every shader samples actual glyph geometry prepared by the `FFrames` text renderer.
use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
};

use fframes::{
    FFramesContext, Shader, ShaderUniforms, Svgr,
    captions::{CaptionWordScope, WordState},
    media::ImageData,
    svgr,
};

type GlyphKey = (String, u32, u32, String, u64);

#[path = "glyph_field.rs"]
mod glyph_field;

#[path = "glass_nebula_material.rs"]
mod glass_nebula_material;
#[path = "goo_material.rs"]
mod goo_material;
#[path = "irradiance_material.rs"]
mod irradiance_material;
pub use glass_nebula_material::GlassNebulaTuning;
pub use goo_material::GooTuning;
pub use irradiance_material::IrradianceTuning;

#[derive(Clone)]
struct GlyphTexture {
    image: ImageData<'static>,
    field: Option<ImageData<'static>>,
}

/// Visual timing is independent of the source word's spoken interval.
#[derive(Clone, Copy)]
pub struct WordDirection {
    /// Keep this word fully present until this caption-clock time, without retiming speech.
    pub hold_until: Option<f64>,
    /// Duration of the departure after the visual release.
    pub exit_seconds: f64,
    /// Make the first frame readable and preserve full brightness through a planned hold.
    pub hero: bool,
    /// Reserve a subtle warm horizon within the final platinum WIN treatment.
    pub warm: bool,
    /// Per-word art direction in [0, 1]: motion phase, particle placement and tint.
    pub variation: f32,
}

/// A #RRGGBB display color converted once to the linear RGB used by `LiquidMetal`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidColor(pub [f32; 3]);

impl<'de> serde::Deserialize<'de> for LiquidColor {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <String as serde::Deserialize>::deserialize(deserializer)?;
        let hex = text
            .strip_prefix('#')
            .filter(|s| s.len() == 6 && s.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .ok_or_else(|| serde::de::Error::custom("liquid color must be #RRGGBB"))?;
        let value = u32::from_str_radix(hex, 16)
            .map_err(|_| serde::de::Error::custom("liquid color must be #RRGGBB"))?;
        Ok(Self::from_srgb_bytes([
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
        ]))
    }
}

impl LiquidColor {
    fn from_srgb_bytes(rgb: [u8; 3]) -> Self {
        Self(rgb.map(|byte| {
            let value = f32::from(byte) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        }))
    }
}

/// Shaders.com `LiquidMetal` controls. Defaults and limits follow the upstream material.
/// Placement and shape come from the caption scope; no separate image loader is involved.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LiquidTuning {
    pub turbulence: f32,
    pub ripple: f32,
    pub warp: f32,
    pub speed: f32,
    pub sharpness: f32,
    pub environment: f32,
    pub env_rotation: f32,
    pub dispersion: f32,
    pub light_angle: f32,
    /// Relative to glyph height; bounded by the cached 32-pixel distance band.
    pub bevel_width: f32,
    pub bevel_shape: f32,
    pub light_color: LiquidColor,
    pub dark_color: LiquidColor,
}

impl Default for LiquidTuning {
    fn default() -> Self {
        Self {
            turbulence: 1.0,
            ripple: 4.0,
            warp: 1.5,
            speed: 0.5,
            sharpness: 0.6,
            environment: 1.5,
            env_rotation: 0.0,
            dispersion: 0.25,
            light_angle: 265.0,
            bevel_width: 0.05,
            bevel_shape: 0.0,
            light_color: LiquidColor::from_srgb_bytes([238, 240, 246]),
            dark_color: LiquidColor::from_srgb_bytes([20, 20, 20]),
        }
    }
}

impl LiquidTuning {
    pub fn validate(self) -> Result<(), &'static str> {
        for (value, low, high, error) in [
            (
                self.turbulence,
                0.0,
                1.5,
                "liquid.turbulence must be finite and within 0..=1.5",
            ),
            (
                self.ripple,
                0.5,
                8.0,
                "liquid.ripple must be finite and within 0.5..=8",
            ),
            (
                self.warp,
                0.0,
                1.5,
                "liquid.warp must be finite and within 0..=1.5",
            ),
            (
                self.speed,
                0.0,
                4.0,
                "liquid.speed must be finite and within 0..=4",
            ),
            (
                self.sharpness,
                0.0,
                1.0,
                "liquid.sharpness must be finite and within 0..=1",
            ),
            (
                self.environment,
                0.0,
                2.0,
                "liquid.environment must be finite and within 0..=2",
            ),
            (
                self.env_rotation,
                0.0,
                360.0,
                "liquid.env_rotation must be finite degrees within 0..=360",
            ),
            (
                self.dispersion,
                0.0,
                1.0,
                "liquid.dispersion must be finite and within 0..=1",
            ),
            (
                self.light_angle,
                0.0,
                360.0,
                "liquid.light_angle must be finite degrees within 0..=360",
            ),
            (
                self.bevel_width,
                0.005,
                0.2,
                "liquid.bevel_width must be finite and within 0.005..=0.2",
            ),
            (
                self.bevel_shape,
                0.0,
                1.0,
                "liquid.bevel_shape must be finite and within 0..=1",
            ),
        ] {
            if !value.is_finite() || !(low..=high).contains(&value) {
                return Err(error);
            }
        }
        for color in [self.light_color, self.dark_color] {
            if color
                .0
                .iter()
                .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
            {
                return Err("liquid colors must contain finite linear RGB within 0..=1");
            }
        }
        Ok(())
    }
}

/// Data-driven art controls for the two geometric relief materials (25/26).
/// The cached glyph and compiled shader are shared across all settings.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReliefTuning {
    /// Extrusion and recess depth, in [0.5, 1.5]. Bounded by the texture padding.
    pub depth: f32,
    /// Rib/channel spacing, in [0.65, 1.75]. Larger values give broader features.
    pub detail_scale: f32,
    /// Material motion speed, in [0.25, 2]. Does not retime speech or the hold.
    pub speed: f32,
    /// Exposure multiplier, in [0.5, 1.5], before display encoding.
    pub light: f32,
}

impl Default for ReliefTuning {
    fn default() -> Self {
        Self {
            depth: 1.0,
            detail_scale: 1.0,
            speed: 1.0,
            light: 1.0,
        }
    }
}

impl ReliefTuning {
    /// Validate once when reading authored direction, before rendering frames.
    pub fn validate(self) -> Result<(), &'static str> {
        for (value, low, high, error) in [
            (
                self.depth,
                0.5,
                1.5,
                "relief.depth must be finite and within 0.5..=1.5",
            ),
            (
                self.detail_scale,
                0.65,
                1.75,
                "relief.detail_scale must be finite and within 0.65..=1.75",
            ),
            (
                self.speed,
                0.25,
                2.0,
                "relief.speed must be finite and within 0.25..=2.0",
            ),
            (
                self.light,
                0.5,
                1.5,
                "relief.light must be finite and within 0.5..=1.5",
            ),
        ] {
            if !value.is_finite() || !(low..=high).contains(&value) {
                return Err(error);
            }
        }
        Ok(())
    }

    fn bounded(self) -> Self {
        fn value(value: f32, low: f32, high: f32) -> f32 {
            if value.is_finite() {
                value.clamp(low, high)
            } else {
                1.0
            }
        }
        Self {
            depth: value(self.depth, 0.5, 1.5),
            detail_scale: value(self.detail_scale, 0.65, 1.75),
            speed: value(self.speed, 0.25, 2.0),
            light: value(self.light, 0.5, 1.5),
        }
    }
}

/// Optional material controls, resolved once from typed caption metadata.
#[derive(Clone, Copy, Debug, Default)]
pub struct MaterialControls {
    pub relief: ReliefTuning,
    pub liquid: LiquidTuning,
    pub irradiance: IrradianceTuning,
    pub goo: GooTuning,
    pub glass_nebula: GlassNebulaTuning,
}

fn entrance_seconds(duration: f64) -> f64 {
    (duration * 0.28)
        .clamp(0.06, 0.10)
        .min(duration * 0.4)
        .max(0.001)
}

pub struct WordEffects {
    shader: Option<Shader>,
    // Specializing a family lets Skia eliminate the other fifteen material paths.
    // The compiled shader cache then reuses each small program across all its words.
    directed_shaders: Vec<Shader>,
    // Authored font family/weight join the dimensions in the cache key. Reconstruct
    // WordEffects if registered font bytes change in a persistent application.
    glyphs: Mutex<HashMap<GlyphKey, GlyphTexture>>,
}

impl WordEffects {
    /// Whether this renderer can execute glyph-local runtime shaders.
    #[allow(dead_code)] // Consumers without projective glyphs do not need this capability query.
    pub fn shaders_enabled(&self) -> bool {
        self.shader.is_some()
    }

    pub fn new(shaders: bool) -> Self {
        let source = include_str!("shaders/word-effects.sksl");
        Self {
            shader: shaders.then(|| Shader::sksl(source)),
            directed_shaders: if shaders {
                (0..31)
                    .map(|effect| {
                        let (source, effect) = if effect < 16 {
                            (source, effect)
                        } else if effect < 22 {
                            (include_str!("shaders/showcase-effects.sksl"), effect - 16)
                        } else if effect < 25 {
                            (include_str!("shaders/semantic-effects.sksl"), effect - 22)
                        } else if effect < 27 {
                            (include_str!("shaders/relief-effects.sksl"), effect - 25)
                        } else if effect == 27 {
                            (include_str!("shaders/liquid-metal.sksl"), 0)
                        } else if effect == 28 {
                            (include_str!("shaders/irradiance.sksl"), 0)
                        } else if effect == 29 {
                            (include_str!("shaders/goo.sksl"), 0)
                        } else {
                            (include_str!("shaders/glass-nebula.sksl"), 0)
                        };
                        Shader::sksl(
                            source
                                .replace(
                                    "uniform float uEffect;",
                                    &format!("const float uEffect = {effect}.0;"),
                                )
                                .replace(
                                    "uniform float uDirected;",
                                    "const float uDirected = 1.0;",
                                ),
                        )
                    })
                    .collect()
            } else {
                Vec::new()
            },
            glyphs: Mutex::new(HashMap::new()),
        }
    }

    fn glyph(
        &self,
        scope: &CaptionWordScope<'_, '_>,
        ctx: &FFramesContext<'_, '_>,
        padding: f64,
        width: u32,
        height: u32,
    ) -> Option<GlyphTexture> {
        let layout = scope.word.metadata.get("layout");
        let family = layout
            .and_then(|layout| layout.get("font_family"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let weight = layout
            .and_then(|layout| layout.get("font_weight"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let key = (
            scope.word.text.to_owned(),
            width,
            height,
            family.to_owned(),
            weight,
        );
        if let Some(image) = self
            .glyphs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            return Some(image.clone());
        }
        // The scope makes this mask using the actual font and glyph positions used by captions.
        // Its white alpha is the shader's texture, rather than a rectangular paint layer.
        let glyph = scope.mask_text(svgr!(
            <rect x={-padding} y={-padding} width={width} height={height} fill="white" />
        ));
        let image = ctx.render_svgr(
            svgr!(
                <svg xmlns="http://www.w3.org/2000/svg" width={width} height={height}
                     viewBox={format!("0 0 {width} {height}")}>
                    <g transform={format!("translate({padding} {padding})")}>{glyph}</g>
                </svg>
            ),
            width,
            height,
        )?;
        let texture = GlyphTexture {
            field: glyph_field::from_glyph(&image),
            image,
        };
        self.glyphs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key, texture.clone());
        Some(texture)
    }

    /// Render a material with its typed, cached caption metadata controls.
    pub fn render_directed_with_controls<'a>(
        &'a self,
        scope: &CaptionWordScope<'a, '_>,
        text: Svgr<'a>,
        ctx: &FFramesContext<'_, '_>,
        effect_index: usize,
        direction: WordDirection,
        controls: &MaterialControls,
    ) -> Svgr<'a> {
        let liquid = if controls.liquid.validate().is_ok() {
            controls.liquid
        } else {
            LiquidTuning::default()
        };
        self.render_impl(
            scope,
            text,
            ctx,
            effect_index,
            Some(direction),
            &MaterialControls {
                relief: controls.relief.bounded(),
                liquid,
                ..*controls
            },
        )
    }

    fn render_impl<'a>(
        &'a self,
        scope: &CaptionWordScope<'a, '_>,
        text: Svgr<'a>,
        ctx: &FFramesContext<'_, '_>,
        effect_index: usize,
        direction: Option<WordDirection>,
        controls: &MaterialControls,
    ) -> Svgr<'a> {
        let MaterialControls { relief, liquid, .. } = *controls;
        let effect = effect_index % if direction.is_some() { 31 } else { 6 };
        let Some(start) = scope.word.start else {
            return text;
        };
        if scope.word.state == WordState::Upcoming {
            return svgr!(<g opacity="0.18">{text}</g>);
        }
        let end = scope.word.end.unwrap_or(start);
        let age = (scope.time - start).max(0.0);
        let entrance = entrance_seconds((end - start).max(0.001));
        let exit_seconds = direction.map_or(0.24, |d| {
            if d.exit_seconds.is_finite() {
                d.exit_seconds.max(0.001)
            } else {
                0.24
            }
        });
        // Holds extend picture duration; they never shorten or stretch the source word.
        let release = direction
            .and_then(|d| d.hold_until)
            .filter(|hold| hold.is_finite())
            .unwrap_or(end)
            .max(end);
        let tail = ((scope.time - release) / exit_seconds).clamp(0.0, 1.0);
        let hero = direction.is_some_and(|d| d.hero);
        let warm = direction.is_some_and(|d| d.warm);
        if scope.word.state == WordState::Spoken && tail >= 1.0 {
            return if (direction.is_none() && effect == 3) || hero {
                Svgr::empty()
            } else {
                svgr!(<g opacity="0.28">{text}</g>)
            };
        }
        // Directed materials occupy the glyph plus its cached 32px distance band.
        // Constant padding bounds shader work and lets different treatments reuse
        // the same glyph texture. The legacy deformations retain proportional room.
        let padding = if direction.is_some() && effect >= 16 {
            // Visible fire wisps and released raster fragments need more room than
            // the material library's near-field rims. The glyph cache includes size.
            72.0
        } else if direction.is_some() {
            f64::from(glyph_field::DISTANCE_RANGE) + 4.0
        } else {
            (scope.height() * 0.27).ceil().max(24.0)
        };
        let width = (scope.width() + padding * 2.0).ceil().max(1.0) as u32;
        let height = (scope.height() + padding * 2.0).ceil().max(1.0) as u32;
        let shader = if direction.is_some() {
            self.directed_shaders.get(effect)
        } else {
            self.shader.as_ref()
        };
        if let Some(shader) = shader
            && let Some(glyph) = self.glyph(scope, ctx, padding, width, height)
            && (direction.is_none() || glyph.field.is_some())
        {
            let seed = start as f32 * 17.13 + scope.word.token_index as f32 * 3.7;
            // Legacy geometry gets its short entrance clock; directed lighting, fire and
            // particles use uLightAge and continue moving through a visual hold.
            let shader_age = if direction.is_some() {
                age.min(entrance) + tail * exit_seconds
            } else {
                age
            };
            let mut uniforms = ShaderUniforms::new()
                .image("uGlyph", &glyph.image)
                .image("uField", glyph.field.as_ref().unwrap_or(&glyph.image))
                .float("uDistanceRange", glyph_field::DISTANCE_RANGE)
                .float2("uWordSize", scope.width() as f32, scope.height() as f32)
                .float("uPadding", padding as f32)
                .float("uEffect", effect as f32)
                .float("uProgress", scope.word.progress)
                .float("uAge", shader_age as f32)
                .float("uLightAge", age as f32)
                .float("uTail", tail as f32)
                .float("uSeed", seed)
                .float("uDirected", if direction.is_some() { 1.0 } else { 0.0 })
                .float("uEntrance", (age / entrance).clamp(0.0, 1.0) as f32)
                .float("uHero", if hero { 1.0 } else { 0.0 })
                .float("uWarm", if warm { 1.0 } else { 0.0 })
                .float(
                    "uVariation",
                    direction.map_or(0.0, |d| {
                        if d.variation.is_finite() {
                            d.variation.clamp(0.0, 1.0)
                        } else {
                            0.0
                        }
                    }),
                );
            if direction.is_some() && matches!(effect, 25 | 26) {
                uniforms = uniforms
                    .float("uDepth", relief.depth)
                    .float("uDetailScale", relief.detail_scale)
                    .float("uSpeed", relief.speed)
                    .float("uLight", relief.light);
            }
            if direction.is_some() && effect == 27 {
                uniforms = uniforms
                    .float("uTurbulence", liquid.turbulence)
                    .float("uRipple", liquid.ripple)
                    .float("uWarp", liquid.warp)
                    .float("uSpeed", liquid.speed)
                    .float("uSharpness", liquid.sharpness)
                    .float("uEnvironment", liquid.environment)
                    .float("uEnvRotation", liquid.env_rotation)
                    .float("uDispersion", liquid.dispersion)
                    .float("uLightAngle", liquid.light_angle)
                    .float("uBevelWidth", liquid.bevel_width)
                    .float("uBevelShape", liquid.bevel_shape)
                    .float3(
                        "uLightColor",
                        liquid.light_color.0[0],
                        liquid.light_color.0[1],
                        liquid.light_color.0[2],
                    )
                    .float3(
                        "uDarkColor",
                        liquid.dark_color.0[0],
                        liquid.dark_color.0[1],
                        liquid.dark_color.0[2],
                    );
            }
            if direction.is_some() {
                uniforms = match effect {
                    28 => {
                        let tuning = if controls.irradiance.validate().is_ok() {
                            controls.irradiance
                        } else {
                            IrradianceTuning::default()
                        };
                        tuning.bind(uniforms)
                    }
                    29 => {
                        let tuning = if controls.goo.validate().is_ok() {
                            controls.goo
                        } else {
                            GooTuning::default()
                        };
                        tuning.bind(uniforms)
                    }
                    30 => {
                        let tuning = if controls.glass_nebula.validate().is_ok() {
                            controls.glass_nebula
                        } else {
                            GlassNebulaTuning::default()
                        };
                        tuning.bind(uniforms)
                    }
                    _ => uniforms,
                };
            }
            let image = shader.draw(scope.frame, uniforms);
            return svgr!(<image href={image.href()} x={-padding} y={-padding}
                width={width} height={height} />);
        }
        self.svg_fallback(scope, text, effect, tail, direction)
    }

    fn svg_fallback<'a>(
        &'a self,
        scope: &CaptionWordScope<'a, '_>,
        text: Svgr<'a>,
        effect: usize,
        tail: f64,
        direction: Option<WordDirection>,
    ) -> Svgr<'a> {
        let p = f64::from(scope.word.progress);
        let hero = direction.is_some_and(|d| d.hero);
        let warm = direction.is_some_and(|d| d.warm);
        let directed = direction.is_some();
        let enter = if directed {
            let duration = scope.word.end.unwrap_or(0.0) - scope.word.start.unwrap_or(0.0);
            let progress = (scope.elapsed().unwrap_or(0.0) / entrance_seconds(duration.max(0.001)))
                .clamp(0.0, 1.0);
            1.0 - progress * progress * (3.0 - 2.0 * progress)
        } else {
            (1.0 - p / 0.22).clamp(0.0, 1.0)
        };
        let opacity = if (direction.is_none() && effect == 3) || hero {
            1.0 - tail
        } else {
            1.0 - tail * 0.72
        };
        let cx = scope.width() * 0.5;
        let cy = scope.height() * 0.5;
        let content = if hero || warm {
            scope.mask_text(svgr!(<rect width={scope.width()} height={scope.height()}
                fill={if warm { "#fff4dd" } else { "#f5fbff" }} />))
        } else {
            text
        };
        match effect % 6 {
            0 => {
                let skew = (p * 12.0).sin() * (enter * 7.0 + if directed { 0.0 } else { 1.3 });
                svgr!(<g opacity={opacity} transform={format!("skewX({skew})")}>{content}</g>)
            }
            1 => {
                let dx = (p * 35.0).sin() * (enter * 15.0 + if directed { 0.0 } else { 1.5 });
                let echo_opacity = 0.25 * if directed { enter.max(tail) } else { 1.0 };
                svgr!(<g opacity={opacity}>
                    <g transform={format!("translate({} -2)",dx+5.0)} opacity={echo_opacity}>{content.clone()}</g>
                    <g transform={format!("translate({dx} 0)")}>{content}</g>
                </g>)
            }
            2 => {
                let sy = 1.0 - enter * if hero { 0.0 } else { 0.7 };
                svgr!(<g opacity={opacity} transform={format!("translate(0 {}) scale(1 {sy})",cy*(1.0-sy))}>{content}</g>)
            }
            3 => {
                svgr!(<g opacity={opacity} transform={format!("translate({} {})",tail*if directed { -24.0 } else { 18.0 },-tail*20.0)}>{content}</g>)
            }
            4 => {
                svgr!(<g opacity={opacity*if directed { 1.0 } else { 0.84+0.16*(p*35.0).sin().abs() }}>{content}</g>)
            }
            _ => {
                let scale = 1.0 + enter * if hero { 0.08 } else { 0.16 };
                let echo_opacity = 0.22 * if directed { enter.max(tail) } else { 1.0 };
                svgr!(<g opacity={opacity}>
                    <g transform={format!("translate({} {})",-12.0*enter,7.0*enter)} opacity={echo_opacity}>{content.clone()}</g>
                    <g transform={format!("translate({cx} {cy}) scale({scale}) translate({} {})",-cx,-cy)}>{content}</g>
                </g>)
            }
        }
    }
}
