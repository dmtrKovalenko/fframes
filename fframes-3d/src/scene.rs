use std::ops::{Index, IndexMut};
use std::sync::Arc;

use glam::{Mat4, Quat, Vec3, Vec4};

use crate::math::{Aabb, euler};
use crate::{Blending, Environment, Geometry, Light, Material, Rgb};

/// The handle of a node in a [`Scene`].
///
/// An id stays valid in its scene and in all clones of the scene. Keep the ids of the
/// animated parts next to a template scene. Use them on the clone of each frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

impl NodeId {
    /// The position of the node in the node list of its scene.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// The content of a [`Node`].
#[derive(Debug, Clone)]
pub enum Object {
    /// A transform only, three.js `Group` / `Object3D`.
    Group,
    /// A geometry with a material, three.js `Mesh`.
    Mesh(Mesh),
    /// A light source.
    Light(Light),
    /// Glowing sprites, three.js `Points`.
    Particles(Particles),
}

/// A geometry drawn with a material, three.js `Mesh`.
#[derive(Debug, Clone)]
pub struct Mesh {
    /// The shape. All clones of a scene share it.
    pub geometry: Arc<Geometry>,
    pub material: Material,
    /// Whether the mesh casts shadows. Default `true`.
    pub cast_shadow: bool,
    /// Whether shadows fall on the mesh. Default `true`.
    pub receive_shadow: bool,
    /// The bones of a skinned mesh (glTF skins).
    pub skin: Option<Skin>,
    /// The weights of the morph targets of the geometry, in target order.
    pub morph_weights: Vec<f32>,
}

/// The joints of a skinned mesh.
///
/// A vertex moves by the world matrices of its joints times their inverse bind matrices.
/// The transform of the mesh node has no effect, as in glTF.
#[derive(Debug, Clone)]
pub struct Skin {
    pub joints: Vec<NodeId>,
    /// One matrix for each joint, from mesh space to the rest space of the joint.
    pub inverse_bind_matrices: Arc<Vec<Mat4>>,
}

/// Round glowing sprites and streaks, for sparks, embers and dust.
///
/// The renderer draws them after the opaque geometry, and the geometry hides them. The
/// positions are in the space of the node.
#[derive(Debug, Clone, Default)]
pub struct Particles {
    pub items: Vec<Particle>,
    pub blending: Blending,
}

/// One sprite of [`Particles`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Particle {
    pub position: Vec3,
    /// The offset from `position` to the end of the streak. Zero gives a round dot. For a
    /// spark, use `-velocity * shutter_time`.
    pub tail: Vec3,
    /// The radius in meters. The renderer draws at least about half a pixel.
    pub size: f32,
    /// The emitted light. Values above 1 bloom.
    pub color: Rgb,
    /// The opacity, `0..1`. Default 1.
    pub opacity: f32,
}

impl Particle {
    /// A round dot at `position` with the radius `size` in meters.
    pub fn new(position: Vec3, size: f32, color: impl Into<Rgb>) -> Self {
        Self {
            position,
            tail: Vec3::ZERO,
            size,
            color: color.into(),
            opacity: 1.0,
        }
    }

    /// Sets the offset to the end of the streak.
    pub fn tail(mut self, tail: Vec3) -> Self {
        self.tail = tail;
        self
    }

    /// Sets the opacity, `0..1`.
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
}

/// An object in a [`Scene`], three.js `Object3D`.
///
/// A node has a transform relative to its parent and an [`Object`] to draw. Build it with
/// chained methods:
///
/// ```rust
/// use fframes_3d::{Geometry, Material, Node, deg};
///
/// let wheel = Node::mesh(Geometry::cylinder(0.4, 0.4, 0.2, 32), Material::standard(0x222222))
///     .at(1.0, 0.4, 0.8)
///     .rotate_x(deg(90.0))
///     .named("wheel_fl");
/// ```
#[derive(Debug, Clone)]
pub struct Node {
    /// The name for [`Scene::find`]. glTF models keep the names from the file.
    pub name: Option<Arc<str>>,
    /// The position relative to the parent, in meters.
    pub position: Vec3,
    /// The rotation relative to the parent.
    pub rotation: Quat,
    /// The scale relative to the parent. Default 1.
    pub scale: Vec3,
    /// An invisible node hides its whole subtree. Default `true`.
    pub visible: bool,
    pub object: Object,
    pub(crate) parent: Option<NodeId>,
    pub(crate) children: Vec<NodeId>,
}

impl Node {
    /// A node at the origin with no rotation and a scale of 1.
    pub fn new(object: Object) -> Self {
        Self {
            name: None,
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            visible: true,
            object,
            parent: None,
            children: Vec::new(),
        }
    }

    /// An empty transform that groups and pivots its children, three.js `Group`.
    pub fn group() -> Self {
        Self::new(Object::Group)
    }

    /// A mesh that casts and receives shadows, three.js `Mesh`.
    pub fn mesh(geometry: impl Into<Arc<Geometry>>, material: Material) -> Self {
        Self::new(Object::Mesh(Mesh {
            geometry: geometry.into(),
            material,
            cast_shadow: true,
            receive_shadow: true,
            skin: None,
            morph_weights: Vec::new(),
        }))
    }

    /// A light source, see [`Light`].
    pub fn light(light: Light) -> Self {
        Self::new(Object::Light(light))
    }

    /// Glowing sprites, three.js `Points`. See [`Particles`].
    pub fn particles(items: Vec<Particle>, blending: Blending) -> Self {
        Self::new(Object::Particles(Particles { items, blending }))
    }

    /// Sets the name for [`Scene::find`].
    pub fn named(mut self, name: &str) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the position in meters, three.js `position.set(x, y, z)`.
    pub fn at(self, x: f32, y: f32, z: f32) -> Self {
        self.with_position(Vec3::new(x, y, z))
    }

    /// Sets the position in meters.
    pub fn with_position(mut self, position: Vec3) -> Self {
        self.position = position;
        self
    }

    /// Sets the rotation.
    pub fn with_rotation(mut self, rotation: Quat) -> Self {
        self.rotation = rotation;
        self
    }

    /// Sets the rotation from Euler angles in radians, in XYZ order as in three.js.
    pub fn euler(self, x: f32, y: f32, z: f32) -> Self {
        self.with_rotation(euler(x, y, z))
    }

    /// Turns the node by `angle` radians around its local X axis.
    pub fn rotate_x(mut self, angle: f32) -> Self {
        self.rotation *= Quat::from_rotation_x(angle);
        self
    }

    /// Turns the node by `angle` radians around its local Y axis.
    pub fn rotate_y(mut self, angle: f32) -> Self {
        self.rotation *= Quat::from_rotation_y(angle);
        self
    }

    /// Turns the node by `angle` radians around its local Z axis.
    pub fn rotate_z(mut self, angle: f32) -> Self {
        self.rotation *= Quat::from_rotation_z(angle);
        self
    }

    /// Sets the same scale on all axes.
    pub fn scaled(self, scale: f32) -> Self {
        self.scaled3(scale, scale, scale)
    }

    /// Sets the scale of each axis.
    pub fn scaled3(mut self, x: f32, y: f32, z: f32) -> Self {
        self.scale = Vec3::new(x, y, z);
        self
    }

    /// Hides the node and its subtree.
    pub fn hidden(mut self) -> Self {
        self.visible = false;
        self
    }

    /// Sets whether a mesh casts and receives shadows. Other nodes ignore it.
    pub fn shadows(mut self, cast: bool, receive: bool) -> Self {
        if let Some(mesh) = self.mesh_mut() {
            mesh.cast_shadow = cast;
            mesh.receive_shadow = receive;
        }
        self
    }

    /// The transform relative to the parent.
    pub fn local_matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }

    /// The parent node. `None` for a node at the root.
    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    /// The child nodes, in the order they were added.
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    /// The mesh of a mesh node.
    pub fn as_mesh(&self) -> Option<&Mesh> {
        match &self.object {
            Object::Mesh(mesh) => Some(mesh),
            _ => None,
        }
    }

    /// The mesh of a mesh node, mutable.
    pub fn mesh_mut(&mut self) -> Option<&mut Mesh> {
        match &mut self.object {
            Object::Mesh(mesh) => Some(mesh),
            _ => None,
        }
    }

    /// The material of a mesh node, mutable.
    pub fn material_mut(&mut self) -> Option<&mut Material> {
        self.mesh_mut().map(|mesh| &mut mesh.material)
    }

    /// The light of a light node, mutable.
    pub fn light_mut(&mut self) -> Option<&mut Light> {
        match &mut self.object {
            Object::Light(light) => Some(light),
            _ => None,
        }
    }
}

/// What the renderer draws where there is no geometry.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Background {
    /// Alpha 0, the default. The SVG behind the `<image>` shows through. Glows and bloom
    /// stay visible on it.
    #[default]
    Transparent,
    /// A solid color.
    Color(Rgb),
    /// The environment, blurred as a surface of roughness `blur` (`0..1`) reflects it.
    Environment { blur: f32 },
    /// A vertical gradient from the top to the bottom of the frame.
    Gradient { top: Rgb, bottom: Rgb },
}

/// Distance fog, three.js `Fog` and `FogExp2`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fog {
    /// No fog closer than `near` meters, full fog beyond `far` meters, three.js `Fog`.
    Linear { color: Rgb, near: f32, far: f32 },
    /// Fog that grows as `1 - exp(-(density * distance)²)`, three.js `FogExp2`.
    Exp2 { color: Rgb, density: f32 },
}

impl Fog {
    /// The amount of fog at `distance` meters from the camera, `0..1`.
    pub(crate) fn factor(&self, distance: f32) -> f32 {
        match *self {
            Fog::Linear { near, far, .. } => ((distance - near) / (far - near)).clamp(0.0, 1.0),
            Fog::Exp2 { density, .. } => 1.0 - (-(density * distance).powi(2)).exp(),
        }
    }

    pub(crate) fn color(&self) -> Rgb {
        match *self {
            Fog::Linear { color, .. } | Fog::Exp2 { color, .. } => color,
        }
    }
}

/// A scene graph, three.js `Scene`.
///
/// Build a template scene once and keep the [`NodeId`]s of the parts that you animate. For
/// each frame, clone the template, set the transforms and render the clone. The clones
/// share the geometry, so a clone is cheap.
///
/// ```rust
/// use fframes_3d::*;
///
/// let mut scene = Scene::new().environment(Environment::studio());
/// let cube = scene.add(Node::mesh(Geometry::rounded_box(1.0, 1.0, 1.0, 0.1, 3), Material::metal(0xd0d4dc, 0.25)));
/// scene.add(Node::light(Light::sun(Rgb::WHITE, 3.0, vec3(3.0, 6.0, 4.0))));
///
/// // in render_frame
/// let t = 0.5;
/// let mut frame_scene = scene.clone();
/// frame_scene[cube].rotation = euler(t, t * 1.3, 0.0);
/// let camera = Camera::perspective(40.0).at(0.0, 1.5, 4.0).look_at(Vec3::ZERO);
/// let image = Renderer::new(640, 360).render(&frame_scene, &camera);
/// ```
#[derive(Debug, Clone, Default)]
pub struct Scene {
    /// All nodes. A parent always comes before its children.
    nodes: Vec<Node>,
    /// Default [`Background::Transparent`].
    pub background: Background,
    /// The light around the scene. Default [`Environment::None`].
    pub environment: Environment,
    /// Default `None`.
    pub fog: Option<Fog>,
    /// The plane that materials with [`Material::reflection`] mirror. Default `None`.
    pub reflector: Option<Reflector>,
}

/// A plane that mirrors the scene, three.js `Reflector`.
///
/// Use it for a floor, a table top or a mirror wall. Surfaces on the plane with
/// [`Material::reflection`] show the mirrored scene, blurred by their roughness. A scene
/// has one reflector at most.
///
/// ```rust
/// use fframes_3d::*;
///
/// let mut scene = Scene::new().reflector(Reflector::floor());
/// scene.add(Node::mesh(Geometry::floor(20.0, 20.0), Material::standard(0x303236).roughness(0.3).reflection(0.8)));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reflector {
    /// A point on the plane, in meters.
    pub point: Vec3,
    /// The unit normal of the plane, on the side of the viewer.
    pub normal: Vec3,
    /// The resolution of the reflection relative to the frame, `0.1..=1`. Default 0.5.
    /// Roughness blurs most reflections, so only mirror finishes need more.
    pub scale: f32,
}

impl Reflector {
    /// A plane through `point` with the normal `normal`. A zero normal becomes +Y.
    pub fn new(point: Vec3, normal: Vec3) -> Self {
        Self {
            point,
            normal: normal.normalize_or(Vec3::Y),
            scale: 0.5,
        }
    }

    /// The ground plane `y = 0`, facing up.
    pub fn floor() -> Self {
        Self::new(Vec3::ZERO, Vec3::Y)
    }

    /// Sets the resolution relative to the frame, clamped to `0.1..=1`.
    pub fn scale(mut self, scale: f32) -> Self {
        self.scale = scale.clamp(0.1, 1.0);
        self
    }

    /// The plane as `(normal, offset)`. `dot(normal, p) + offset` is the distance of `p` in
    /// front of the plane.
    pub(crate) fn plane(&self) -> Vec4 {
        self.normal.extend(-self.normal.dot(self.point))
    }

    /// The transform that mirrors the world in the plane.
    pub(crate) fn matrix(&self) -> Mat4 {
        let n = self.normal;
        let mirror = |v: Vec3| v - 2.0 * n * n.dot(v);
        Mat4::from_cols(
            mirror(Vec3::X).extend(0.0),
            mirror(Vec3::Y).extend(0.0),
            mirror(Vec3::Z).extend(0.0),
            (2.0 * n * n.dot(self.point)).extend(1.0),
        )
    }
}

impl Scene {
    /// An empty scene with a transparent background and no environment light.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the background.
    pub fn background(mut self, background: Background) -> Self {
        self.background = background;
        self
    }

    /// Sets the environment light.
    pub fn environment(mut self, environment: Environment) -> Self {
        self.environment = environment;
        self
    }

    /// Sets the plane that the scene reflects in, see [`Reflector`].
    pub fn reflector(mut self, reflector: Reflector) -> Self {
        self.reflector = Some(reflector);
        self
    }

    /// Sets the fog.
    pub fn fog(mut self, fog: Fog) -> Self {
        self.fog = Some(fog);
        self
    }

    /// Adds a node at the root and returns its id.
    pub fn add(&mut self, node: Node) -> NodeId {
        self.insert(None, node)
    }

    /// Adds a node as a child of `parent` and returns its id. The transform of the node is
    /// relative to the parent.
    pub fn add_to(&mut self, parent: NodeId, node: Node) -> NodeId {
        self.insert(Some(parent), node)
    }

    /// Adds a node below `parent`, or at the root for `None`, and returns its id.
    pub(crate) fn insert(&mut self, parent: Option<NodeId>, mut node: Node) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        node.parent = parent;
        node.children.clear();
        if let Some(parent) = parent {
            self.nodes[parent.index()].children.push(id);
        }
        self.nodes.push(node);
        id
    }

    /// Copies all nodes of `other` with their hierarchy below `parent`, or to the root for
    /// `None`. Returns the id offset: node `NodeId(i)` of `other` becomes
    /// `NodeId(i + offset)`. The copies of skins point to the copied joints.
    /// [`Model::add_to`](crate::Model::add_to) uses it.
    pub fn append(&mut self, parent: Option<NodeId>, other: &Scene) -> u32 {
        let offset = self.nodes.len() as u32;
        let shift = |id: &mut NodeId| id.0 += offset;
        for node in &other.nodes {
            let mut node = node.clone();
            node.children.iter_mut().for_each(shift);
            if let Some(skin) = node.mesh_mut().and_then(|mesh| mesh.skin.as_mut()) {
                skin.joints.iter_mut().for_each(shift);
            }
            if let Some(own) = &mut node.parent {
                shift(own);
            } else if let Some(parent) = parent {
                node.parent = Some(parent);
                let id = NodeId(self.nodes.len() as u32);
                self.nodes[parent.index()].children.push(id);
            }
            self.nodes.push(node);
        }
        offset
    }

    /// The number of nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the scene has no nodes.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// All nodes with their ids. A parent always comes before its children.
    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        (0..).map(NodeId).zip(&self.nodes)
    }

    /// The node with this id. `None` when the id is out of range.
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.index())
    }

    /// The node with this id, mutable. `None` when the id is out of range.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id.index())
    }

    /// The id of the first node with this name.
    pub fn find(&self, name: &str) -> Option<NodeId> {
        self.nodes()
            .find(|(_, n)| n.name.as_deref() == Some(name))
            .map(|(id, _)| id)
    }

    /// The first node with this name, mutable.
    pub fn find_mut(&mut self, name: &str) -> Option<&mut Node> {
        self.nodes
            .iter_mut()
            .find(|n| n.name.as_deref() == Some(name))
    }

    /// `id` and all nodes below it, breadth first.
    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = vec![id];
        let mut i = 0;
        while i < out.len() {
            out.extend_from_slice(&self.nodes[out[i].index()].children);
            i += 1;
        }
        out
    }

    /// The transform from the space of the node to world space.
    pub fn world_matrix(&self, id: NodeId) -> Mat4 {
        let node = &self.nodes[id.index()];
        match node.parent {
            Some(parent) => self.world_matrix(parent) * node.local_matrix(),
            None => node.local_matrix(),
        }
    }

    /// The world position of the origin of the node. Give it to
    /// [`Camera::project`](crate::Camera::project) to put SVG on the node.
    pub fn world_position(&self, id: NodeId) -> Vec3 {
        self.world_matrix(id).w_axis.truncate()
    }

    /// Converts a point from the space of the node to world space.
    pub fn local_to_world(&self, id: NodeId, local: Vec3) -> Vec3 {
        self.world_matrix(id).transform_point3(local)
    }

    /// The world matrices of all nodes, in node order.
    pub fn world_matrices(&self) -> Vec<Mat4> {
        let mut out: Vec<Mat4> = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let local = node.local_matrix();
            let world = match node.parent {
                Some(parent) => out[parent.index()] * local,
                None => local,
            };
            out.push(world);
        }
        out
    }

    /// For each node, whether the node and all its ancestors are visible.
    pub fn visibilities(&self) -> Vec<bool> {
        let mut out: Vec<bool> = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let parent_visible = node.parent.is_none_or(|p| out[p.index()]);
            out.push(parent_visible && node.visible);
        }
        out
    }

    /// The world bounds of the visible meshes in the subtree of `id`. Skins do not change
    /// the result.
    pub fn bounds(&self, id: NodeId) -> Aabb {
        let worlds = self.world_matrices();
        let visible = self.visibilities();
        let mut aabb = Aabb::EMPTY;
        for n in self.descendants(id) {
            if visible[n.index()]
                && let Some(mesh) = self[n].as_mesh()
            {
                aabb = aabb.union(mesh.geometry.bounds().transformed(&worlds[n.index()]));
            }
        }
        aabb
    }
}

impl Index<NodeId> for Scene {
    type Output = Node;

    fn index(&self, id: NodeId) -> &Node {
        &self.nodes[id.index()]
    }
}

impl IndexMut<NodeId> for Scene {
    fn index_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.index()]
    }
}
