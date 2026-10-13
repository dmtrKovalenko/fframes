use std::sync::Arc;

use crate::{Rgb, Texture};

/// The side of a triangle that the renderer draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Side {
    /// The side where the vertices go counterclockwise. The default.
    #[default]
    Front,
    /// The other side.
    Back,
    /// Both sides.
    Double,
}

/// How a transparent surface or a particle combines with the color behind it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Blending {
    /// Alpha blending. The default.
    #[default]
    Normal,
    /// Adds light, for glows, energy, sparks and holograms. It never makes a color darker.
    Additive,
}

/// The lighting model of a [`Material`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shading {
    /// Physically based metal and roughness, three.js `MeshStandardMaterial`. With
    /// clearcoat it is `MeshPhysicalMaterial`. The default.
    #[default]
    Standard,
    /// No lighting, three.js `MeshBasicMaterial`. The surface shows its color and its
    /// emissive light.
    Unlit,
    /// Invisible except for the shadows that fall on it, three.js `ShadowMaterial`. Put it
    /// on a floor to ground objects over an SVG background.
    ShadowCatcher,
    /// World space normals as colors, for debugging.
    Normals,
}

/// A procedural pattern that the surface emits as light, on top of its shading.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// Grid lines every `cell` meters, `line_width` meters wide. The lines fade out beyond
    /// `fade_distance` meters from the camera. They lie in the plane that the surface
    /// faces, so the grid works on floors and on walls.
    Grid {
        cell: f32,
        line_width: f32,
        color: Rgb,
        fade_distance: f32,
    },
}

/// The look of a surface, after three.js `MeshStandardMaterial`.
///
/// A material is a plain value, and each mesh owns its copy. To animate a property, set it
/// on the scene of the frame: `scene[id].material_mut().unwrap().emissive_intensity = glow`.
///
/// ```rust
/// use fframes_3d::{Material, Rgb};
///
/// let steel = Material::metal(0xb8c0cc, 0.3);
/// let paint = Material::standard(0xff5500).roughness(0.45).clearcoat(1.0);
/// let visor = Material::standard(0x111111).emissive(0x33ddff, 6.0);
/// let ghost = Material::standard(0x88ccff).opacity(0.3).additive();
/// ```
#[derive(Debug, Clone)]
pub struct Material {
    /// Default [`Shading::Standard`].
    pub shading: Shading,
    /// The base color. Default white.
    pub color: Rgb,
    /// `0..1`, default 1. Below 1, or with [`Material::transparent`], the renderer blends
    /// the surface after the opaque geometry, sorted back to front.
    pub opacity: f32,
    /// `0..1`, default 0.
    pub metalness: f32,
    /// `0..1`, default 1. [`Material::standard`] sets 0.5.
    pub roughness: f32,
    /// The color of the emitted light. Default black.
    pub emissive: Rgb,
    /// The multiplier for `emissive`. Default 1.
    pub emissive_intensity: f32,
    /// A second glossy layer on top, as on car paint, `0..1`. Default 0.
    pub clearcoat: f32,
    /// The roughness of the clearcoat. Default 0.05.
    pub clearcoat_roughness: f32,
    /// How much of the [`Reflector`](crate::Reflector) of the scene the surface shows instead
    /// of the environment reflection, `0..1`. Default 0. Roughness blurs it. Only surfaces on
    /// the plane of the reflector can use it.
    pub reflection: f32,
    /// The multiplier for the reflections and the ambient light of the environment.
    /// Default 1.
    pub env_intensity: f32,
    /// The light emitted at grazing angles (Fresnel rim). Default black, which disables it.
    pub rim: Rgb,
    /// The exponent of the rim falloff. A larger value gives a thinner rim. Default 3.
    pub rim_power: f32,
    /// Shading with the face normal of each triangle, for a faceted look. Default `false`.
    pub flat_shading: bool,
    /// Default [`Side::Front`].
    pub side: Side,
    /// Default [`Blending::Normal`].
    pub blending: Blending,
    /// Blending also at opacity 1. Default `false`.
    pub transparent: bool,
    /// Blended surfaces write depth only when this is set. Default `false`.
    pub depth_write: bool,
    /// The base color texture, three.js `map`. It multiplies `color`.
    pub map: Option<Arc<Texture>>,
    /// The emissive texture, three.js `emissiveMap`. It multiplies `emissive`.
    pub emissive_map: Option<Arc<Texture>>,
    /// The glTF metallic-roughness texture: roughness in green, metalness in blue.
    pub metal_rough_map: Option<Arc<Texture>>,
    /// Default `None`.
    pub pattern: Option<Pattern>,
    /// Whether the scene fog applies. Default `true`.
    pub fog: bool,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            shading: Shading::Standard,
            color: Rgb::WHITE,
            opacity: 1.0,
            metalness: 0.0,
            roughness: 1.0,
            emissive: Rgb::BLACK,
            emissive_intensity: 1.0,
            clearcoat: 0.0,
            clearcoat_roughness: 0.05,
            reflection: 0.0,
            env_intensity: 1.0,
            rim: Rgb::BLACK,
            rim_power: 3.0,
            flat_shading: false,
            side: Side::Front,
            blending: Blending::Normal,
            transparent: false,
            depth_write: false,
            map: None,
            emissive_map: None,
            metal_rough_map: None,
            pattern: None,
            fog: true,
        }
    }
}

impl Material {
    /// A dielectric such as plastic, paint or rubber, with roughness 0.5.
    pub fn standard(color: impl Into<Rgb>) -> Self {
        Self {
            color: color.into(),
            roughness: 0.5,
            ..Default::default()
        }
    }

    /// A metal. Its reflections take its color. `roughness` is `0..1`.
    pub fn metal(color: impl Into<Rgb>, roughness: f32) -> Self {
        Self {
            color: color.into(),
            metalness: 1.0,
            roughness,
            ..Default::default()
        }
    }

    /// A surface without lighting, three.js `MeshBasicMaterial`.
    pub fn unlit(color: impl Into<Rgb>) -> Self {
        Self {
            shading: Shading::Unlit,
            color: color.into(),
            ..Default::default()
        }
    }

    /// Pure light: unlit, additive and without fog. It emits `color * intensity`, and bright
    /// values bloom.
    pub fn glow(color: impl Into<Rgb>, intensity: f32) -> Self {
        Self::unlit(color.into() * intensity).additive().no_fog()
    }

    /// A surface that shows only the shadows on it, three.js `ShadowMaterial`. `opacity` is
    /// the darkness of the shadows.
    pub fn shadow_catcher(opacity: f32) -> Self {
        Self {
            shading: Shading::ShadowCatcher,
            opacity,
            ..Default::default()
        }
    }

    /// World space normals as colors, for debugging.
    pub fn normals() -> Self {
        Self {
            shading: Shading::Normals,
            ..Default::default()
        }
    }

    /// Sets the base color.
    pub fn color(mut self, color: impl Into<Rgb>) -> Self {
        self.color = color.into();
        self
    }

    /// Sets the metalness, `0..1`.
    pub fn metalness(mut self, metalness: f32) -> Self {
        self.metalness = metalness;
        self
    }

    /// Sets the roughness, `0..1`.
    pub fn roughness(mut self, roughness: f32) -> Self {
        self.roughness = roughness;
        self
    }

    /// Sets the emitted light to `color * intensity`. Lighting does not change it. Above
    /// about 1 it blooms.
    pub fn emissive(mut self, color: impl Into<Rgb>, intensity: f32) -> Self {
        self.emissive = color.into();
        self.emissive_intensity = intensity;
        self
    }

    /// Sets the glossy top layer, `0..1`.
    pub fn clearcoat(mut self, amount: f32) -> Self {
        self.clearcoat = amount;
        self
    }

    /// Sets the multiplier for the light of the environment.
    pub fn env_intensity(mut self, intensity: f32) -> Self {
        self.env_intensity = intensity;
        self
    }

    /// Shows the scene mirrored in its [`Reflector`](crate::Reflector) instead of the
    /// environment reflection. `amount` is `0..1`, and roughness blurs the mirror image.
    pub fn reflection(mut self, amount: f32) -> Self {
        self.reflection = amount.clamp(0.0, 1.0);
        self
    }

    /// Adds a Fresnel rim light: the surface emits `color` where it turns away from the
    /// camera. A larger `power` gives a thinner rim.
    pub fn rim(mut self, color: impl Into<Rgb>, power: f32) -> Self {
        self.rim = color.into();
        self.rim_power = power;
        self
    }

    /// Sets the opacity, `0..1`.
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    /// Makes the surface add its light to the color behind it, see [`Blending::Additive`].
    pub fn additive(mut self) -> Self {
        self.blending = Blending::Additive;
        self.transparent = true;
        self
    }

    /// Blends the surface also at opacity 1.
    pub fn transparent(mut self) -> Self {
        self.transparent = true;
        self
    }

    /// Draws both sides of each triangle.
    pub fn double_sided(self) -> Self {
        self.side(Side::Double)
    }

    /// Sets the side of the triangles that the renderer draws.
    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }

    /// Shades with face normals, for a faceted look.
    pub fn flat(mut self) -> Self {
        self.flat_shading = true;
        self
    }

    /// Sets the base color texture.
    pub fn map(mut self, texture: impl Into<Arc<Texture>>) -> Self {
        self.map = Some(texture.into());
        self
    }

    /// Sets the emissive texture.
    pub fn emissive_map(mut self, texture: impl Into<Arc<Texture>>) -> Self {
        self.emissive_map = Some(texture.into());
        self
    }

    /// Adds glowing grid lines, see [`Pattern::Grid`].
    pub fn grid(
        mut self,
        cell: f32,
        line_width: f32,
        color: impl Into<Rgb>,
        fade_distance: f32,
    ) -> Self {
        self.pattern = Some(Pattern::Grid {
            cell,
            line_width,
            color: color.into(),
            fade_distance,
        });
        self
    }

    /// Turns off the scene fog for this surface.
    pub fn no_fog(mut self) -> Self {
        self.fog = false;
        self
    }

    /// Whether the renderer draws the surface in the blended pass.
    pub(crate) fn is_blended(&self) -> bool {
        self.transparent || self.opacity < 1.0 || self.blending == Blending::Additive
    }
}
