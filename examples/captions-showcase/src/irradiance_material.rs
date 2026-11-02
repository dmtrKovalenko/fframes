//! Typed controls for the shaders.com Irradiance glyph adaptation.
use fframes::ShaderUniforms;

use super::LiquidColor;

/// One point light, positioned relative to the unpadded word rectangle.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IrradianceLight {
    pub position: [f32; 2],
    pub color: LiquidColor,
    pub intensity: f32,
}

/// Two-light, stateless adaptation of the upstream edge irradiance material.
/// Reach is in pixels and deliberately stays inside the cached distance band.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IrradianceTuning {
    pub lights: [IrradianceLight; 2],
    pub light_height: f32,
    pub light_range: f32,
    pub reach_px: f32,
    pub core: f32,
    pub wrap: f32,
    pub shadows: bool,
    pub shadow_softness: f32,
    pub body_color: LiquidColor,
    pub body_light: f32,
    pub bevel_width: f32,
    /// Optional deterministic movement of the two lights; zero freezes them.
    pub speed: f32,
    /// Orbit radius in word-height units; zero keeps the exact authored positions.
    pub orbit: f32,
}

impl Default for IrradianceTuning {
    fn default() -> Self {
        Self {
            lights: [
                IrradianceLight {
                    position: [0.12, -0.25],
                    color: LiquidColor::from_srgb_bytes([255, 179, 71]),
                    intensity: 3.0,
                },
                IrradianceLight {
                    position: [0.9, 1.25],
                    color: LiquidColor::from_srgb_bytes([122, 92, 255]),
                    intensity: 2.0,
                },
            ],
            light_height: 0.5,
            light_range: 1.2,
            reach_px: 28.0,
            core: 1.2,
            wrap: 0.1,
            shadows: true,
            shadow_softness: 0.15,
            body_color: LiquidColor::from_srgb_bytes([221, 228, 235]),
            body_light: 0.6,
            bevel_width: 0.035,
            speed: 0.7,
            orbit: 0.12,
        }
    }
}

impl IrradianceTuning {
    pub fn validate(self) -> Result<(), &'static str> {
        for (value, low, high, message) in [
            (
                self.light_height,
                0.05,
                3.0,
                "irradiance.light_height must be finite within 0.05..=3",
            ),
            (
                self.light_range,
                0.1,
                4.0,
                "irradiance.light_range must be finite within 0.1..=4",
            ),
            (
                self.reach_px,
                4.0,
                28.0,
                "irradiance.reach_px must be finite within 4..=28",
            ),
            (
                self.core,
                0.0,
                2.0,
                "irradiance.core must be finite within 0..=2",
            ),
            (
                self.wrap,
                0.0,
                1.0,
                "irradiance.wrap must be finite within 0..=1",
            ),
            (
                self.shadow_softness,
                0.0,
                1.0,
                "irradiance.shadow_softness must be finite within 0..=1",
            ),
            (
                self.body_light,
                0.0,
                2.0,
                "irradiance.body_light must be finite within 0..=2",
            ),
            (
                self.bevel_width,
                0.005,
                0.2,
                "irradiance.bevel_width must be finite within 0.005..=0.2",
            ),
            (
                self.speed,
                0.0,
                4.0,
                "irradiance.speed must be finite within 0..=4",
            ),
            (
                self.orbit,
                0.0,
                0.5,
                "irradiance.orbit must be finite within 0..=0.5",
            ),
        ] {
            if !value.is_finite() || !(low..=high).contains(&value) {
                return Err(message);
            }
        }
        for light in self.lights {
            if light
                .position
                .iter()
                .any(|value| !value.is_finite() || !(-2.0..=3.0).contains(value))
            {
                return Err("irradiance light positions must be finite within -2..=3");
            }
            if !light.intensity.is_finite() || !(0.0..=8.0).contains(&light.intensity) {
                return Err("irradiance light intensity must be finite within 0..=8");
            }
        }
        for color in [self.body_color, self.lights[0].color, self.lights[1].color] {
            if color
                .0
                .iter()
                .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
            {
                return Err("irradiance colors must be finite linear RGB within 0..=1");
            }
        }
        Ok(())
    }

    pub fn bind(self, uniforms: ShaderUniforms) -> ShaderUniforms {
        let a = self.lights[0];
        let b = self.lights[1];
        uniforms
            .float2("uLightPosition0", a.position[0], a.position[1])
            .float3("uLightColor0", a.color.0[0], a.color.0[1], a.color.0[2])
            .float("uLightIntensity0", a.intensity)
            .float2("uLightPosition1", b.position[0], b.position[1])
            .float3("uLightColor1", b.color.0[0], b.color.0[1], b.color.0[2])
            .float("uLightIntensity1", b.intensity)
            .float("uIrradianceHeight", self.light_height)
            .float("uIrradianceRange", self.light_range)
            .float("uIrradianceReach", self.reach_px)
            .float("uIrradianceCore", self.core)
            .float("uIrradianceWrap", self.wrap)
            .float("uIrradianceShadows", if self.shadows { 1.0 } else { 0.0 })
            .float("uIrradianceShadowSoftness", self.shadow_softness)
            .float3(
                "uIrradianceBodyColor",
                self.body_color.0[0],
                self.body_color.0[1],
                self.body_color.0[2],
            )
            .float("uIrradianceBodyLight", self.body_light)
            .float("uIrradianceBevel", self.bevel_width)
            .float("uIrradianceSpeed", self.speed)
            .float("uIrradianceOrbit", self.orbit)
    }
}
