use glam::Vec3;

use crate::Rgb;
use crate::math::Aabb;

/// A light source. Add it to a scene with [`Node::light`](crate::Node::light).
///
/// Point and spot lights shine from the world position of their node. The rotation of the
/// node turns the direction of directional and spot lights.
///
/// Intensities follow the physically based mode of three.js. A directional light of
/// intensity π lights a white surface that faces it to full white. The light of point and
/// spot lights falls off with the inverse square of the distance (candela).
#[derive(Debug, Clone, PartialEq)]
pub enum Light {
    /// Light from all directions, three.js `AmbientLight`.
    Ambient {
        color: Rgb,
        intensity: f32,
    },
    /// The sky color from above and the ground color from below, three.js `HemisphereLight`.
    Hemisphere {
        sky: Rgb,
        ground: Rgb,
        intensity: f32,
    },
    Directional(DirectionalLight),
    Point(PointLight),
    Spot(SpotLight),
}

/// Parallel rays as from the sun, three.js `DirectionalLight`. It is the only light that
/// casts shadows.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectionalLight {
    pub color: Rgb,
    pub intensity: f32,
    /// The unit direction that the light travels, in the space of the node.
    pub direction: Vec3,
    /// The shadow settings. `None` casts no shadows.
    pub shadow: Option<Shadow>,
}

/// The shadow map settings of a [`DirectionalLight`], three.js `light.shadow`.
#[derive(Debug, Clone, PartialEq)]
pub struct Shadow {
    /// The size of the square shadow map in texels. Default 2048.
    pub map_size: u32,
    /// The radius of the soft shadow filter in shadow map texels. Default 1.5.
    pub softness: f32,
    /// The darkness of the shadow, `0..1`. Default 1.
    pub strength: f32,
    /// The depth bias in shadow map texels. It prevents shadow acne on lit surfaces.
    /// Default 1.5.
    pub bias: f32,
    /// The world box that receives shadows. `None`, the default, fits the box of all
    /// shadow casters. Set it when the casters are far apart.
    pub bounds: Option<Aabb>,
}

impl Default for Shadow {
    fn default() -> Self {
        Self {
            map_size: 2048,
            softness: 1.5,
            strength: 1.0,
            bias: 1.5,
            bounds: None,
        }
    }
}

/// A light bulb, three.js `PointLight`.
#[derive(Debug, Clone, PartialEq)]
pub struct PointLight {
    pub color: Rgb,
    pub intensity: f32,
    /// The range in meters. The light fades out smoothly towards it. 0 gives no limit.
    pub distance: f32,
    /// The exponent of the falloff. 2 is physically correct.
    pub decay: f32,
}

/// A cone of light, three.js `SpotLight`.
#[derive(Debug, Clone, PartialEq)]
pub struct SpotLight {
    pub color: Rgb,
    pub intensity: f32,
    /// The range in meters. The light fades out smoothly towards it. 0 gives no limit.
    pub distance: f32,
    /// The exponent of the falloff. 2 is physically correct.
    pub decay: f32,
    /// Half the angle of the cone in radians.
    pub angle: f32,
    /// The part of the cone that fades out, `0..1`.
    pub penumbra: f32,
    /// The unit direction of the cone axis, in the space of the node.
    pub direction: Vec3,
}

impl Light {
    /// Light from all directions, three.js `AmbientLight`.
    pub fn ambient(color: impl Into<Rgb>, intensity: f32) -> Self {
        Light::Ambient {
            color: color.into(),
            intensity,
        }
    }

    /// Sky light from above and ground light from below, three.js `HemisphereLight`.
    pub fn hemisphere(sky: impl Into<Rgb>, ground: impl Into<Rgb>, intensity: f32) -> Self {
        Light::Hemisphere {
            sky: sky.into(),
            ground: ground.into(),
            intensity,
        }
    }

    /// A directional light that shines from `from` towards the origin, without shadows.
    ///
    /// It is a three.js `DirectionalLight` at the position `from` with the default target.
    /// For shadows, use [`Light::sun`] or [`Light::with_shadow`].
    pub fn directional(color: impl Into<Rgb>, intensity: f32, from: Vec3) -> Self {
        Light::Directional(DirectionalLight {
            color: color.into(),
            intensity,
            direction: (-from).normalize_or(Vec3::NEG_Y),
            shadow: None,
        })
    }

    /// A directional light that casts soft shadows, see [`Light::directional`].
    pub fn sun(color: impl Into<Rgb>, intensity: f32, from: Vec3) -> Self {
        Self::directional(color, intensity, from).with_shadow(Shadow::default())
    }

    /// A point light with decay 2. `distance` is the range in meters, 0 for no limit.
    pub fn point(color: impl Into<Rgb>, intensity: f32, distance: f32) -> Self {
        Light::Point(PointLight {
            color: color.into(),
            intensity,
            distance,
            decay: 2.0,
        })
    }

    /// A spot light that points down -Y, with no range limit and decay 2.
    ///
    /// `angle` is half the cone angle in radians. `penumbra` is the part of the cone that
    /// fades out, `0..1`. Aim the light with [`Light::pointing`] or turn its node.
    pub fn spot(color: impl Into<Rgb>, intensity: f32, angle: f32, penumbra: f32) -> Self {
        Light::Spot(SpotLight {
            color: color.into(),
            intensity,
            distance: 0.0,
            decay: 2.0,
            angle,
            penumbra,
            direction: Vec3::NEG_Y,
        })
    }

    /// Sets the shadow of a directional light. Other lights ignore it.
    pub fn with_shadow(mut self, shadow: Shadow) -> Self {
        if let Light::Directional(light) = &mut self {
            light.shadow = Some(shadow);
        }
        self
    }

    /// Sets the direction of a directional or spot light. Other lights ignore it.
    pub fn pointing(mut self, direction: Vec3) -> Self {
        if let Light::Directional(DirectionalLight { direction: d, .. })
        | Light::Spot(SpotLight { direction: d, .. }) = &mut self
        {
            *d = direction.normalize_or(Vec3::NEG_Y);
        }
        self
    }

    /// The intensity, mutable. Use it for flicker and fades.
    pub fn intensity_mut(&mut self) -> &mut f32 {
        match self {
            Light::Ambient { intensity, .. }
            | Light::Hemisphere { intensity, .. }
            | Light::Directional(DirectionalLight { intensity, .. })
            | Light::Point(PointLight { intensity, .. })
            | Light::Spot(SpotLight { intensity, .. }) => intensity,
        }
    }
}
