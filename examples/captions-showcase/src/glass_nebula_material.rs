use super::LiquidColor;
use fframes::ShaderUniforms;

/// Shaders.com Nebula controls over a cached `FFrames` glyph volume.
/// Colors interpolate in linear sRGB; the upstream color-space selector is not exposed.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GlassNebulaTuning {
    pub core_color: LiquidColor,
    pub gas_color: LiquidColor,
    pub veil_color: LiquidColor,
    /// Maximum optical chord through a rounded glyph stem, in gas-domain units.
    /// This adapts the upstream sphere/flat-shape thickness to thin text strokes.
    pub optical_depth: f32,
    pub density: f32,
    pub cavity: f32,
    pub dust: f32,
    pub gas_scale: f32,
    pub billow: f32,
    pub glow: f32,
    pub seed: f32,
    pub stars: f32,
    pub star_scale: f32,
    pub twinkle: f32,
    pub refraction: f32,
    pub environment: f32,
    pub highlight: f32,
    pub highlight_softness: f32,
    pub light_angle: f32,
    pub speed: f32,
}

impl Default for GlassNebulaTuning {
    fn default() -> Self {
        Self {
            core_color: LiquidColor::from_srgb_bytes([255, 217, 176]),
            gas_color: LiquidColor::from_srgb_bytes([255, 94, 122]),
            veil_color: LiquidColor::from_srgb_bytes([58, 91, 219]),
            optical_depth: 0.65,
            density: 1.0,
            cavity: 0.0,
            dust: 0.55,
            gas_scale: 0.4,
            billow: 0.6,
            glow: 0.0,
            seed: 0.0,
            stars: 0.07,
            star_scale: 1.0,
            twinkle: 0.35,
            refraction: 0.5,
            environment: 0.7,
            highlight: 1.15,
            highlight_softness: 0.5,
            light_angle: 315.0,
            speed: 0.3,
        }
    }
}

impl GlassNebulaTuning {
    pub fn validate(self) -> Result<(), &'static str> {
        for (value, low, high, error) in [
            (
                self.optical_depth,
                0.05,
                1.2,
                "glass_nebula.optical_depth must be finite and within 0.05..=1.2",
            ),
            (
                self.density,
                0.0,
                1.0,
                "glass_nebula.density must be finite and within 0..=1",
            ),
            (
                self.cavity,
                0.0,
                1.0,
                "glass_nebula.cavity must be finite and within 0..=1",
            ),
            (
                self.dust,
                0.0,
                1.0,
                "glass_nebula.dust must be finite and within 0..=1",
            ),
            (
                self.gas_scale,
                0.1,
                1.0,
                "glass_nebula.gas_scale must be finite and within 0.1..=1",
            ),
            (
                self.billow,
                0.0,
                1.0,
                "glass_nebula.billow must be finite and within 0..=1",
            ),
            (
                self.glow,
                0.0,
                2.0,
                "glass_nebula.glow must be finite and within 0..=2",
            ),
            (
                self.seed,
                0.0,
                100.0,
                "glass_nebula.seed must be finite and within 0..=100",
            ),
            (
                self.stars,
                0.0,
                1.0,
                "glass_nebula.stars must be finite and within 0..=1",
            ),
            (
                self.star_scale,
                0.25,
                3.0,
                "glass_nebula.star_scale must be finite and within 0.25..=3",
            ),
            (
                self.twinkle,
                0.0,
                1.0,
                "glass_nebula.twinkle must be finite and within 0..=1",
            ),
            (
                self.refraction,
                0.0,
                2.0,
                "glass_nebula.refraction must be finite and within 0..=2",
            ),
            (
                self.environment,
                0.0,
                2.0,
                "glass_nebula.environment must be finite and within 0..=2",
            ),
            (
                self.highlight,
                0.0,
                4.0,
                "glass_nebula.highlight must be finite and within 0..=4",
            ),
            (
                self.highlight_softness,
                0.0,
                1.0,
                "glass_nebula.highlight_softness must be finite and within 0..=1",
            ),
            (
                self.light_angle,
                0.0,
                360.0,
                "glass_nebula.light_angle must be finite degrees within 0..=360",
            ),
            (
                self.speed,
                0.0,
                4.0,
                "glass_nebula.speed must be finite and within 0..=4",
            ),
        ] {
            if !value.is_finite() || !(low..=high).contains(&value) {
                return Err(error);
            }
        }
        for color in [self.core_color, self.gas_color, self.veil_color] {
            if color
                .0
                .iter()
                .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
            {
                return Err("glass_nebula colors must contain finite linear RGB within 0..=1");
            }
        }
        Ok(())
    }

    pub fn bind(self, uniforms: ShaderUniforms) -> ShaderUniforms {
        uniforms
            .float3(
                "uCoreColor",
                self.core_color.0[0],
                self.core_color.0[1],
                self.core_color.0[2],
            )
            .float3(
                "uGasColor",
                self.gas_color.0[0],
                self.gas_color.0[1],
                self.gas_color.0[2],
            )
            .float3(
                "uVeilColor",
                self.veil_color.0[0],
                self.veil_color.0[1],
                self.veil_color.0[2],
            )
            .float("uDensity", self.density)
            .float("uOpticalDepth", self.optical_depth)
            .float("uCavity", self.cavity)
            .float("uDust", self.dust)
            .float("uGasScale", self.gas_scale)
            .float("uBillow", self.billow)
            .float("uGlow", self.glow)
            .float("uSeed", self.seed)
            .float("uStars", self.stars)
            .float("uStarScale", self.star_scale)
            .float("uTwinkle", self.twinkle)
            .float("uRefraction", self.refraction)
            .float("uEnvironment", self.environment)
            .float("uHighlight", self.highlight)
            .float("uHighlightSoftness", self.highlight_softness)
            .float("uLightAngle", self.light_angle)
            .float("uSpeed", self.speed)
    }
}
