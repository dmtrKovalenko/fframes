//! Typed controls for the shaders.com Goo glyph material.
use fframes::ShaderUniforms;

use super::LiquidColor;

/// The upstream seven-charge wet metaball material, bounded to caption geometry.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GooTuning {
    pub goo_color: LiquidColor,
    pub translucency: f32,
    pub absorb: f32,
    pub containment: f32,
    pub edge_softness: f32,
    pub blob_scale: f32,
    pub spread: f32,
    pub merge: f32,
    pub bulge: f32,
    pub threshold: f32,
    pub light_angle: f32,
    pub wetness: f32,
    pub fresnel: f32,
    pub spec_color: LiquidColor,
    pub ambient: f32,
    pub speed: f32,
    pub wobble: f32,
    pub breathe: f32,
    pub seed: f32,
}

impl Default for GooTuning {
    fn default() -> Self {
        Self {
            goo_color: LiquidColor::from_srgb_bytes([55, 201, 90]),
            translucency: 0.6,
            absorb: 2.0,
            containment: 0.9,
            edge_softness: 0.06,
            blob_scale: 0.3,
            spread: 1.0,
            merge: 0.6,
            bulge: 1.0,
            threshold: 0.5,
            light_angle: 300.0,
            wetness: 2.0,
            fresnel: 0.25,
            spec_color: LiquidColor::from_srgb_bytes([255, 255, 255]),
            ambient: 0.25,
            speed: 0.5,
            wobble: 0.5,
            breathe: 0.3,
            seed: 1.0,
        }
    }
}

impl GooTuning {
    pub fn validate(self) -> Result<(), &'static str> {
        for (value, low, high, error) in [
            (
                self.translucency,
                0.0,
                1.0,
                "goo.translucency must be finite within 0..=1",
            ),
            (
                self.absorb,
                0.0,
                6.0,
                "goo.absorb must be finite within 0..=6",
            ),
            (
                self.containment,
                0.0,
                1.0,
                "goo.containment must be finite within 0..=1",
            ),
            (
                self.edge_softness,
                0.0,
                0.3,
                "goo.edge_softness must be finite within 0..=0.3",
            ),
            (
                self.blob_scale,
                0.1,
                1.0,
                "goo.blob_scale must be finite within 0.1..=1",
            ),
            (
                self.spread,
                0.0,
                2.0,
                "goo.spread must be finite within 0..=2",
            ),
            (
                self.merge,
                0.0,
                1.0,
                "goo.merge must be finite within 0..=1",
            ),
            (
                self.bulge,
                0.0,
                1.5,
                "goo.bulge must be finite within 0..=1.5",
            ),
            (
                self.threshold,
                0.2,
                0.9,
                "goo.threshold must be finite within 0.2..=0.9",
            ),
            (
                self.light_angle,
                0.0,
                360.0,
                "goo.light_angle must be finite degrees within 0..=360",
            ),
            (
                self.wetness,
                0.0,
                5.0,
                "goo.wetness must be finite within 0..=5",
            ),
            (
                self.fresnel,
                0.0,
                1.0,
                "goo.fresnel must be finite within 0..=1",
            ),
            (
                self.ambient,
                0.0,
                1.0,
                "goo.ambient must be finite within 0..=1",
            ),
            (
                self.speed,
                0.0,
                2.0,
                "goo.speed must be finite within 0..=2",
            ),
            (
                self.wobble,
                0.0,
                1.0,
                "goo.wobble must be finite within 0..=1",
            ),
            (
                self.breathe,
                0.0,
                1.0,
                "goo.breathe must be finite within 0..=1",
            ),
            (
                self.seed,
                0.0,
                100.0,
                "goo.seed must be finite within 0..=100",
            ),
        ] {
            if !value.is_finite() || !(low..=high).contains(&value) {
                return Err(error);
            }
        }
        for color in [self.goo_color, self.spec_color] {
            if color
                .0
                .iter()
                .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            {
                return Err("goo colors must contain finite linear RGB within 0..=1");
            }
        }
        Ok(())
    }

    pub fn bind(self, uniforms: ShaderUniforms) -> ShaderUniforms {
        uniforms
            .float3(
                "uGooColor",
                self.goo_color.0[0],
                self.goo_color.0[1],
                self.goo_color.0[2],
            )
            .float3(
                "uSpecColor",
                self.spec_color.0[0],
                self.spec_color.0[1],
                self.spec_color.0[2],
            )
            .float("uTranslucency", self.translucency)
            .float("uAbsorb", self.absorb)
            .float("uContainment", self.containment)
            .float("uEdgeSoftness", self.edge_softness)
            .float("uBlobScale", self.blob_scale)
            .float("uSpread", self.spread)
            .float("uMerge", self.merge)
            .float("uBulge", self.bulge)
            .float("uThreshold", self.threshold)
            .float("uLightAngle", self.light_angle)
            .float("uWetness", self.wetness)
            .float("uFresnel", self.fresnel)
            .float("uAmbient", self.ambient)
            .float("uSpeed", self.speed)
            .float("uWobble", self.wobble)
            .float("uBreathe", self.breathe)
            .float("uSeed", self.seed)
    }
}
