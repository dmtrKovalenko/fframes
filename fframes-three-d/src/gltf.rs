//! glTF 2.0 models, the format that three.js loads with `GLTFLoader`.
//!
//! The loader reads `.glb` files and `.gltf` files with embedded buffers. It reads meshes,
//! PBR materials and textures, node hierarchies, skins, morph targets, animations and
//! punctual lights.
//!
//! Not supported: cameras, the texture coordinates of sets other than 0, sampler wrap modes
//! (textures repeat), `KHR_texture_transform` and the other material extensions. A
//! `MASK` material blends instead of cutting at `alphaCutoff`.

use std::collections::HashMap;
use std::sync::Arc;

use base64::Engine;
use glam::{Mat4, Quat, Vec2, Vec3, Vec4};
use gltf::animation::util::ReadOutputs;

use crate::animation::{AnimationClip, Channel, Interpolation, Keyframes};
use crate::geometry::{MorphTarget, SkinWeights};
use crate::math::Aabb;
use crate::scene::Skin;
use crate::{Geometry, Light, Material, Node, NodeId, Rgb, Scene, Shading, Side, Texture};

/// The reason why a model does not load.
#[derive(Debug)]
pub enum ModelError {
    /// The file is not valid glTF.
    Gltf(gltf::Error),
    /// A buffer or an image refers to a file that is not available, or its data URI does
    /// not decode.
    MissingFile(String),
    /// A texture image does not decode.
    Image(image::ImageError),
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelError::Gltf(e) => write!(f, "invalid glTF: {e}"),
            ModelError::MissingFile(uri) => {
                write!(f, "glTF refers to a file that was not provided: {uri}")
            }
            ModelError::Image(e) => write!(f, "cannot decode a glTF texture: {e}"),
        }
    }
}

impl std::error::Error for ModelError {}

impl From<gltf::Error> for ModelError {
    fn from(e: gltf::Error) -> Self {
        ModelError::Gltf(e)
    }
}

impl From<image::ImageError> for ModelError {
    fn from(e: image::ImageError) -> Self {
        ModelError::Image(e)
    }
}

/// A glTF model: its nodes as a [`Scene`] and its animations.
///
/// Load the model once, and add it to the scene of each frame with [`Model::add_to`].
///
/// ```rust,ignore
/// let robot = Model::from_gltf(include_bytes!("RobotExpressive.glb"))?;
/// let dance = robot.animation("Dance").unwrap();
///
/// // in render_frame
/// let mut scene = self.stage.clone();
/// let instance = self.robot.add_to(&mut scene, None);
/// scene[instance.root].scale = Vec3::splat(0.5);
/// instance.play(&mut scene, dance, frame.seconds());
/// ```
#[derive(Debug, Clone)]
pub struct Model {
    pub scene: Scene,
    pub animations: Vec<AnimationClip>,
}

/// A model in a scene. Model node `NodeId(i)` is `NodeId(i + offset)` in the scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelInstance {
    /// The group that holds the model. Move, turn and scale the whole model with it.
    pub root: NodeId,
    /// The difference between the scene ids and the model ids.
    pub offset: u32,
    /// The number of model nodes.
    pub len: u32,
}

impl ModelInstance {
    /// The scene id of a model node.
    pub fn node(&self, model_node: NodeId) -> NodeId {
        NodeId(model_node.0 + self.offset)
    }

    /// The scene id of the first node of this instance with this name.
    pub fn find(&self, scene: &Scene, name: &str) -> Option<NodeId> {
        (self.offset..self.offset + self.len)
            .map(NodeId)
            .find(|id| scene.get(*id).and_then(|n| n.name.as_deref()) == Some(name))
    }

    /// Plays `clip` at `time` seconds, looped.
    pub fn play(&self, scene: &mut Scene, clip: &AnimationClip, time: f32) {
        self.blend(scene, clip, time, 1.0);
    }

    /// Blends `clip` at `time` seconds, looped, over the current pose. `weight` is `0..1`.
    pub fn blend(&self, scene: &mut Scene, clip: &AnimationClip, time: f32, weight: f32) {
        clip.apply_looped(scene, self.offset, time, weight);
    }
}

impl Model {
    /// Loads a `.glb` file, or a `.gltf` file with its buffers and images embedded as data
    /// URIs.
    pub fn from_gltf(bytes: &[u8]) -> Result<Self, ModelError> {
        Self::from_gltf_with(bytes, |_| None)
    }

    /// Loads a glTF file and reads external buffers and images with `resolve`. `resolve`
    /// gets each `uri` relative to the `.gltf` file.
    pub fn from_gltf_with(
        bytes: &[u8],
        resolve: impl Fn(&str) -> Option<Vec<u8>>,
    ) -> Result<Self, ModelError> {
        let gltf = gltf::Gltf::from_slice(bytes)?;
        let load_uri = |uri: &str| {
            let missing = || ModelError::MissingFile(uri.to_owned());
            match uri.strip_prefix("data:") {
                Some(data) => {
                    let base64 = data.split_once(";base64,").map_or("", |(_, d)| d);
                    base64::engine::general_purpose::STANDARD
                        .decode(base64)
                        .map_err(|_| missing())
                }
                None => resolve(uri).ok_or_else(missing),
            }
        };

        let buffers = gltf
            .buffers()
            .map(|buffer| match buffer.source() {
                gltf::buffer::Source::Bin => Ok(gltf.blob.clone().unwrap_or_default()),
                gltf::buffer::Source::Uri(uri) => load_uri(uri),
            })
            .collect::<Result<Vec<_>, _>>()?;

        let decode = |bytes: &[u8]| -> Result<_, ModelError> {
            Ok(Arc::new(image::load_from_memory(bytes)?.to_rgba8()))
        };
        let images = gltf
            .images()
            .map(|image| match image.source() {
                gltf::image::Source::View { view, .. } => buffers[view.buffer().index()]
                    .get(view.offset()..view.offset() + view.length())
                    .map(decode)
                    .transpose(),
                // An image file that is not available leaves its textures out.
                gltf::image::Source::Uri { uri, .. } => {
                    load_uri(uri).ok().map(|bytes| decode(&bytes)).transpose()
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut loader = Loader {
            buffers: &buffers,
            images: &images,
            textures: HashMap::new(),
            scene: Scene::new(),
            node_ids: HashMap::new(),
            mesh_nodes: Vec::new(),
        };
        let roots: Vec<gltf::Node> =
            if let Some(scene) = gltf.default_scene().or_else(|| gltf.scenes().next()) {
                scene.nodes().collect()
            } else {
                // without scenes, the nodes that are nobody's child
                let children: std::collections::HashSet<usize> = gltf
                    .nodes()
                    .flat_map(|n| n.children().map(|c| c.index()))
                    .collect();
                gltf.nodes()
                    .filter(|n| !children.contains(&n.index()))
                    .collect()
            };
        for root in roots {
            loader.add_node(&root, None);
        }
        loader.add_skins();
        let animations = gltf.animations().map(|a| loader.animation(&a)).collect();
        Ok(Model {
            scene: loader.scene,
            animations,
        })
    }

    /// The animation clip with this name.
    pub fn animation(&self, name: &str) -> Option<&AnimationClip> {
        self.animations.iter().find(|a| a.name == name)
    }

    /// Adds the model below `parent`, or at the root for `None`. A new group node holds the
    /// model and is the root of the instance.
    pub fn add_to(&self, scene: &mut Scene, parent: Option<NodeId>) -> ModelInstance {
        let root = scene.insert(parent, Node::group());
        let offset = scene.append(Some(root), &self.scene);
        ModelInstance {
            root,
            offset,
            len: self.scene.len() as u32,
        }
    }

    /// The bounds of the model in its rest pose.
    pub fn bounds(&self) -> Aabb {
        self.scene
            .nodes()
            .filter(|(_, node)| node.parent().is_none())
            .fold(Aabb::EMPTY, |aabb, (id, _)| {
                aabb.union(self.scene.bounds(id))
            })
    }
}

/// Builds the [`Scene`] of a model from the glTF nodes.
struct Loader<'a, 'g> {
    buffers: &'a [Vec<u8>],
    images: &'a [Option<Arc<image::RgbaImage>>],
    /// The textures by image index and color space (`true` for sRGB).
    textures: HashMap<(usize, bool), Arc<Texture>>,
    scene: Scene,
    /// The scene ids of the glTF nodes, by glTF node index.
    node_ids: HashMap<usize, NodeId>,
    /// The glTF nodes with meshes, for [`Loader::add_skins`].
    mesh_nodes: Vec<(NodeId, gltf::Node<'g>)>,
}

impl<'g> Loader<'_, 'g> {
    /// Adds a glTF node and its subtree below `parent`. As in three.js, a node with one
    /// mesh primitive is the mesh. A node with several primitives is a group with one mesh
    /// child for each. A light is a child node.
    fn add_node(&mut self, gltf_node: &gltf::Node<'g>, parent: Option<NodeId>) {
        let (t, r, s) = gltf_node.transform().decomposed();
        let meshes: Vec<Node> = gltf_node.mesh().map_or_else(Vec::new, |mesh| {
            // the node overrides the default weights of the mesh
            let weights = gltf_node.weights().or(mesh.weights()).unwrap_or_default();
            mesh.primitives()
                .filter(|p| p.mode() == gltf::mesh::Mode::Triangles)
                .map(|primitive| {
                    let mut node = self.mesh_node(&primitive);
                    if let Some(mesh) = node.mesh_mut() {
                        mesh.morph_weights = weights.to_vec();
                    }
                    node
                })
                .collect()
        });
        let (base, meshes) = match <[Node; 1]>::try_from(meshes) {
            Ok([mesh]) => (mesh, Vec::new()),
            Err(meshes) => (Node::group(), meshes),
        };
        let node = Node {
            name: gltf_node.name().map(Into::into),
            position: Vec3::from(t),
            rotation: Quat::from_array(r).normalize(),
            scale: Vec3::from(s),
            ..base
        };
        let id = self.scene.insert(parent, node);
        self.node_ids.insert(gltf_node.index(), id);

        if let Some(light) = gltf_node.light() {
            self.scene.add_to(id, Node::light(convert_light(&light)));
        }
        for mesh in meshes {
            self.scene.add_to(id, mesh);
        }
        if gltf_node.mesh().is_some() {
            self.mesh_nodes.push((id, gltf_node.clone()));
        }

        for child in gltf_node.children() {
            self.add_node(&child, Some(id));
        }
    }

    /// Gives the skinned meshes their skins. It runs after all nodes have ids, because a
    /// skin refers to its joints by id.
    fn add_skins(&mut self) {
        let buffers = self.buffers;
        for (id, gltf_node) in std::mem::take(&mut self.mesh_nodes) {
            let Some(skin) = gltf_node.skin() else {
                continue;
            };
            let reader = skin.reader(|b| buffers.get(b.index()).map(Vec::as_slice));
            let inverse_bind_matrices = reader
                .read_inverse_bind_matrices()
                .map(|m| m.map(|m| Mat4::from_cols_array_2d(&m)).collect())
                .unwrap_or_default();
            let joints = skin
                .joints()
                .map(|j| self.node_ids.get(&j.index()).copied().unwrap_or(NodeId(0)))
                .collect();
            let skin = Skin {
                joints,
                inverse_bind_matrices: Arc::new(inverse_bind_matrices),
            };
            for target in self.scene.descendants(id) {
                if let Some(mesh) = self.scene[target].mesh_mut()
                    && mesh.geometry.skin.is_some()
                {
                    mesh.skin = Some(skin.clone());
                }
            }
        }
    }

    /// Reads an animation. It leaves out the channels of nodes outside the scene.
    fn animation(&self, animation: &gltf::Animation) -> AnimationClip {
        let buffers = self.buffers;
        let mut channels = Vec::new();
        let mut duration: f32 = 0.0;
        for channel in animation.channels() {
            let Some(&node) = self.node_ids.get(&channel.target().node().index()) else {
                continue;
            };
            let reader = channel.reader(|b| buffers.get(b.index()).map(Vec::as_slice));
            let Some(times) = reader.read_inputs().map(Iterator::collect::<Vec<f32>>) else {
                continue;
            };
            duration = duration.max(times.last().copied().unwrap_or(0.0));
            let Some(outputs) = reader.read_outputs() else {
                continue;
            };
            let interpolation = match channel.sampler().interpolation() {
                gltf::animation::Interpolation::Step => Interpolation::Step,
                gltf::animation::Interpolation::Linear => Interpolation::Linear,
                gltf::animation::Interpolation::CubicSpline => Interpolation::CubicSpline,
            };
            let keyframes = match outputs {
                ReadOutputs::Translations(t) => Keyframes::Translation(t.map(Vec3::from).collect()),
                ReadOutputs::Scales(s) => Keyframes::Scale(s.map(Vec3::from).collect()),
                ReadOutputs::Rotations(r) => {
                    Keyframes::Rotation(r.into_f32().map(Quat::from_array).collect())
                }
                ReadOutputs::MorphTargetWeights(w) => {
                    let values: Vec<f32> = w.into_f32().collect();
                    let per_keyframe = values.len().checked_div(times.len()).unwrap_or(0);
                    // A cubic spline stores an in tangent, a value and an out tangent.
                    let targets = match interpolation {
                        Interpolation::CubicSpline => per_keyframe / 3,
                        _ => per_keyframe,
                    };
                    Keyframes::Weights { targets, values }
                }
            };
            channels.push(Channel {
                node,
                interpolation,
                times,
                keyframes,
            });
        }
        AnimationClip {
            name: animation.name().unwrap_or_default().to_owned(),
            duration,
            channels,
        }
    }

    /// A mesh node of a primitive. A primitive without normals is flat shaded, as the
    /// glTF specification asks.
    fn mesh_node(&mut self, primitive: &gltf::Primitive) -> Node {
        let geometry = self.geometry(primitive);
        let mut material = self.material(&primitive.material());
        material.flat_shading |= primitive.get(&gltf::Semantic::Normals).is_none();
        Node::mesh(geometry, material)
    }

    fn geometry(&self, primitive: &gltf::Primitive) -> Geometry {
        let reader = primitive.reader(|b| self.buffers.get(b.index()).map(Vec::as_slice));
        let positions = vec3s(reader.read_positions());
        let indices: Vec<u32> = reader.read_indices().map_or_else(
            || (0..positions.len() as u32).collect(),
            |i| i.into_u32().collect(),
        );
        let normals = vec3s(reader.read_normals());
        let uvs: Vec<Vec2> = reader
            .read_tex_coords(0)
            .map(|uv| uv.into_f32().map(Vec2::from).collect())
            .unwrap_or_default();
        let mut geometry = Geometry::new(positions, normals, uvs, indices);
        if let Some(colors) = reader.read_colors(0) {
            geometry.colors = colors.into_rgba_f32().map(Vec4::from).collect();
        }
        if let (Some(joints), Some(weights)) = (reader.read_joints(0), reader.read_weights(0)) {
            // The weights of each vertex add up to 1.
            let normalize = |w: [f32; 4]| {
                let sum: f32 = w.iter().sum();
                if sum > 0.0 {
                    w.map(|x| x / sum)
                } else {
                    [1.0, 0.0, 0.0, 0.0]
                }
            };
            geometry.skin = Some(SkinWeights {
                joints: joints.into_u16().collect(),
                weights: weights.into_f32().map(normalize).collect(),
            });
        }
        geometry
            .morph_targets
            .extend(
                reader
                    .read_morph_targets()
                    .map(|(positions, normals, _)| MorphTarget {
                        name: None,
                        positions: vec3s(positions),
                        normals: vec3s(normals),
                    }),
            );
        geometry
    }

    /// The texture of a glTF image in a color space. Each pair decodes once.
    fn texture(&mut self, texture: &gltf::Texture, srgb: bool) -> Option<Arc<Texture>> {
        let index = texture.source().index();
        if let Some(texture) = self.textures.get(&(index, srgb)) {
            return Some(Arc::clone(texture));
        }
        let image = self.images.get(index)?.as_ref()?;
        let texture = Arc::new(Texture::from_rgba8(
            image.width(),
            image.height(),
            image.as_raw(),
            srgb,
        ));
        self.textures.insert((index, srgb), Arc::clone(&texture));
        Some(texture)
    }

    fn material(&mut self, material: &gltf::Material) -> Material {
        use gltf::material::AlphaMode;

        let pbr = material.pbr_metallic_roughness();
        let [r, g, b, a] = pbr.base_color_factor();
        let [er, eg, eb] = material.emissive_factor();
        let alpha_mode = material.alpha_mode();
        let normal = material.normal_texture();
        Material {
            shading: if material.unlit() {
                Shading::Unlit
            } else {
                Shading::Standard
            },
            color: Rgb::linear(r, g, b),
            // an OPAQUE material ignores alpha
            opacity: if alpha_mode == AlphaMode::Opaque {
                1.0
            } else {
                a
            },
            metalness: pbr.metallic_factor(),
            roughness: pbr.roughness_factor(),
            emissive: Rgb::linear(er, eg, eb),
            emissive_intensity: material.emissive_strength().unwrap_or(1.0),
            side: if material.double_sided() {
                Side::Double
            } else {
                Side::Front
            },
            transparent: alpha_mode != AlphaMode::Opaque,
            // the cut out parts of a MASK material hide what is behind them
            depth_write: alpha_mode == AlphaMode::Mask,
            map: pbr
                .base_color_texture()
                .and_then(|info| self.texture(&info.texture(), true)),
            metal_rough_map: pbr
                .metallic_roughness_texture()
                .and_then(|info| self.texture(&info.texture(), false)),
            emissive_map: material
                .emissive_texture()
                .and_then(|info| self.texture(&info.texture(), true)),
            normal_scale: normal
                .as_ref()
                .map_or(1.0, gltf::material::NormalTexture::scale),
            normal_map: normal.and_then(|info| self.texture(&info.texture(), false)),
            ao_map: material
                .occlusion_texture()
                .and_then(|info| self.texture(&info.texture(), false)),
            ..Default::default()
        }
    }
}

/// Reads a glTF vector attribute. A missing attribute gives no values.
fn vec3s(values: Option<impl Iterator<Item = [f32; 3]>>) -> Vec<Vec3> {
    values
        .map(|v| v.map(Vec3::from).collect())
        .unwrap_or_default()
}

/// Converts a `KHR_lights_punctual` light. glTF lights shine down -Z of their node.
fn convert_light(light: &gltf::khr_lights_punctual::Light) -> Light {
    let color = Rgb::from(light.color());
    let intensity = light.intensity();
    let light_range = light.range();
    match light.kind() {
        gltf::khr_lights_punctual::Kind::Directional => {
            Light::directional(color, intensity, Vec3::Z)
        }
        gltf::khr_lights_punctual::Kind::Point => {
            Light::point(color, intensity, light.range().unwrap_or(0.0))
        }
        gltf::khr_lights_punctual::Kind::Spot {
            outer_cone_angle,
            inner_cone_angle,
        } => {
            let penumbra = if outer_cone_angle > 0.0 {
                1.0 - inner_cone_angle / outer_cone_angle
            } else {
                0.0
            };
            let mut spot = Light::spot(color, intensity, outer_cone_angle, penumbra);
            if let Light::Spot(light) = &mut spot {
                light.distance = light_range.unwrap_or(0.0);
            }
            spot.pointing(Vec3::NEG_Z)
        }
    }
}
