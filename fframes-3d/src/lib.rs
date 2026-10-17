//! 3D scenes for fframes, with an API after three.js.
//!
//! A [`Scene`] holds [`Node`]s in a hierarchy: meshes, lights, particles and groups. A
//! [`Camera`] looks at the scene. A [`Renderer`] makes an image of both for an `<image>`
//! element in the `svgr!` tree of a frame. Regular SVG can go below and above it.
//!
//! The renderer has 4x multisampling and physically based materials: metal and roughness,
//! clearcoat and specular antialiasing. A procedural studio or an HDR panorama lights the
//! scene. The renderer draws soft shadows, planar reflections ([`Reflector`]), transparency,
//! additive glows and particles. It also has fog, depth of field, motion blur, bloom and
//! ACES tone mapping.
//!
//! The default renderer is a multithreaded software rasterizer. It runs on all backends:
//! CPU, Skia and the wasm editor. The `gpu` feature renders with wgpu on Vulkan, Metal or
//! DX12, see [`Renderer::gpu`]. The two paths use the same shading. On average, a GPU frame
//! differs from the CPU frame by less than one 8-bit step. Without a usable GPU, the CPU
//! renders.
//!
//! A backend can share its GPU, see [`fframes::shared_gpu`]. The Skia Vulkan backend does
//! this with its `wgpu` feature. [`Renderer::render`] then renders on that device into a
//! texture that Skia draws. The 3D image stays on the GPU until the encoder.
//!
//! Units are meters and radians. Colors as hex numbers or `&str` are sRGB.
//!
//! # Example
//!
//! ```rust,ignore
//! use fframes::{FFramesContext, Frame, Svgr, Video, svgr};
//! use fframes_3d::*;
//!
//! struct Spin {
//!     scene: Scene,
//!     cube: NodeId,
//!     renderer: Renderer,
//! }
//!
//! impl Spin {
//!     fn new() -> Self {
//!         let mut scene = Scene::new().environment(Environment::studio());
//!         let cube = scene.add(Node::mesh(
//!             Geometry::rounded_box(1.0, 1.0, 1.0, 0.08, 4),
//!             Material::metal(0xd4d8e0, 0.25),
//!         ).at(0.0, 0.5, 0.0));
//!         scene.add(Node::mesh(Geometry::floor(20.0, 20.0), Material::shadow_catcher(0.6)));
//!         scene.add(Node::light(Light::sun(Rgb::WHITE, 3.0, vec3(2.0, 5.0, 3.0))));
//!         Self { scene, cube, renderer: Renderer::new(1920, 1080).bloom(Bloom::default()) }
//!     }
//! }
//!
//! // in render_frame
//! let t = frame.seconds();
//! let mut scene = self.scene.clone();
//! scene[self.cube].rotation = euler(0.0, t, 0.0);
//! let camera = Camera::perspective(35.0).orbit(vec3(0.0, 0.5, 0.0), 5.0, 30.0, 15.0);
//! let image = self.renderer.render(&scene, &camera);
//! svgr!(<svg ...><image href={image.href()} x="0" y="0" width="1920" height="1080" /></svg>)
//! ```
//!
//! # Coming from three.js
//!
//! | three.js | fframes-3d |
//! | --- | --- |
//! | `new Mesh(new BoxGeometry(1, 2, 3), new MeshStandardMaterial({ color: 0xff0000 }))` | `Node::mesh(Geometry::cuboid(1.0, 2.0, 3.0), Material::standard(0xff0000))` |
//! | `mesh.position.set(x, y, z)`, `mesh.rotation.set(x, y, z)` | `.at(x, y, z)`, `.euler(x, y, z)` or `scene[id].position = ...` |
//! | `new Group()`, `group.add(mesh)` | [`Node::group`], `scene.add_to(group, node)` |
//! | `metalness`, `roughness`, `emissive`, `emissiveIntensity`, `clearcoat`, `map`, `normalMap`, `normalScale`, `aoMap` | the same names on [`Material`] |
//! | `MeshBasicMaterial`, `ShadowMaterial` | [`Material::unlit`], [`Material::shadow_catcher`] |
//! | `AmbientLight`, `HemisphereLight`, `DirectionalLight`, `PointLight`, `SpotLight` | [`Light`] |
//! | `light.castShadow = true` | [`Light::sun`] or [`Light::with_shadow`] |
//! | `scene.environment = pmrem(RoomEnvironment)` | [`Environment::studio`] |
//! | `RGBELoader` panorama | [`Environment::map`] with [`EnvironmentMap::decode`] |
//! | `scene.fog = new Fog(color, near, far)` | [`Scene::fog`] with [`Fog::Linear`] |
//! | `Reflector` | [`Scene::reflector`] with [`Material::reflection`] |
//! | `camera.position.set(..); camera.lookAt(..)` | `Camera::perspective(fov).at(..).look_at(..)` |
//! | `vector.project(camera)` | [`Camera::project`] |
//! | `GLTFLoader` + `AnimationMixer` | [`Model::from_gltf`] + [`ModelInstance::play`] |
//! | `STLLoader`, `toCreasedNormals` | [`Geometry::from_stl`], [`Geometry::smooth`] |
//! | `VideoTexture` | [`Texture::from_image`] of a decoded video frame, for each frame |
//! | `UnrealBloomPass`, `ACESFilmicToneMapping` | [`Bloom`], [`ToneMapping`] |
//! | `Points` | [`Node::particles`] |
//! | `InstancedMesh`, `instanceColor` | [`Node::instanced`] with [`Instances`] |

// Colors are hex numbers as in three.js: `0xff8800`.
#![allow(clippy::unreadable_literal)]
// Rasterization and mesh generation index several parallel arrays in one loop.
#![allow(clippy::needless_range_loop, clippy::items_after_statements)]

mod animation;
mod camera;
mod color;
mod environment;
mod geometry;
mod gltf;
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
mod gpu;
mod light;
mod material;
pub mod math;
mod render;
mod scene;
mod texture;

pub use animation::{AnimationClip, Channel, Interpolation, Keyframes};
pub use camera::{Camera, Focus};
pub use color::Rgb;
pub use environment::{Environment, EnvironmentMap, SoftBox, Studio};
pub use geometry::{
    Geometry, Glyph, GlyphShapes, MorphTarget, Shape, SkinWeights, StlError, TextError,
};
pub use gltf::{Model, ModelError, ModelInstance};
pub use light::{DirectionalLight, Light, PointLight, Shadow, SpotLight};
pub use material::{Blending, Material, Shading, Side};
pub use math::{Aabb, Mat3, Mat4, Quat, Vec2, Vec3, Vec4, deg, euler, euler_deg, vec2, vec3, vec4};
pub use render::{Bloom, Pixels, Renderer, ToneMapping};
pub use scene::{
    Background, Fog, Instances, Mesh, Node, NodeId, Object, Particle, Particles, Reflector, Scene,
    Skin,
};
pub use texture::Texture;
