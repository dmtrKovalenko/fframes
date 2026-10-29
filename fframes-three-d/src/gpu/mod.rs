//! The GPU renderer (feature `gpu`). It draws the same scenes, materials and lights as the
//! software rasterizer, with wgpu on Vulkan, Metal or DX12.
//!
//! The rendering backend can share its GPU (`fframes::shared_gpu`). The Skia Vulkan backend
//! does this with its `wgpu` feature. Then a frame renders on that device into a texture.
//! The backend samples this texture in place (`fframes::wgpu_layer`). The renderer reads
//! nothing back. The frame stays on the GPU through Skia to the encoder. Without a shared
//! GPU, frames render on a device of their own. Then the renderer reads the pixels back.
//!
//! The renderer uploads rigid meshes once and draws them with their world matrix. It deforms
//! skinned and morphed meshes on the CPU and streams their vertices every frame.
//!
//! The passes of one moment:
//! 1. the shadow maps,
//! 2. the reflection, when a material reflects the scene,
//! 3. a 4x multisampled geometry pass into an HDR target and a depth key: the background,
//!    the opaque meshes, the blended meshes from back to front, the particles,
//! 4. the depth of field,
//! 5. the motion blur accumulation.
//!
//! After the moments, the frame gets bloom and tone mapping. The last pass encodes
//! premultiplied sRGB pixels.

use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError, Weak};

use bytemuck::{Pod, Zeroable};
use fframes_media::wgpu_layer::{self, SharedGpu};
use glam::{Mat3, Mat4, Vec3, Vec4};
use wgpu::util::DeviceExt;

use crate::material::Shading;
use crate::render::prepare::{self, LightInfo, LightKind, Lights};
use crate::render::shadow::fit;
use crate::render::{Pixels, Renderer, ToneMapping};
use crate::scene::{Background, Fog, Object};
use crate::{Blending, Camera, Environment, Geometry, Material, Mesh, Scene, Side, Texture};

const MSAA: u32 = 4;
const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
/// The format of the depth key (1 / view distance) that the depth of field reads.
const KEY: wgpu::TextureFormat = wgpu::TextureFormat::R16Float;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const OUTPUT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const MAX_LIGHTS: usize = 16;
const MAX_SOFTBOXES: usize = 8;
const MAX_SHADOWS: usize = 2;
/// The alignment of uniform buffer offsets that all devices support.
const UNIFORM_ALIGNMENT: u64 = 256;
/// The distance between two draw uniforms in the draw buffer.
const DRAW_STRIDE: u64 = (size_of::<DrawUniform>() as u64).next_multiple_of(UNIFORM_ALIGNMENT);
/// The number of uniform slots in the post buffer. Slot 0 belongs to the passes of a moment.
const POST_SLOTS: u64 = 32;
/// The first post slot of the reflection levels. The frame passes use the slots below it.
const REFLECTION_SLOT: u64 = 24;
/// The size of a depth of field tile in pixels.
const DOF_TILE: u32 = 16;
const BLOOM_LEVELS: usize = 6;
/// The maximum number of frame contexts (render targets) in use at the same time.
const CONTEXTS: usize = 3;
/// Rigid meshes with this many indices or fewer that share a material become one batch.
/// A draw costs the same with six triangles or six thousand.
const BATCH_MAX_INDICES: u32 = 4096;

/// Adds the color and keeps the alpha of the target, for glows and additive particles.
const ADD_LIGHT: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
    alpha: wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::Zero,
        dst_factor: wgpu::BlendFactor::One,
        operation: wgpu::BlendOperation::Add,
    },
};

/// Maps clip space z to `w - z`: near is 1 and far is 0. This keeps the depth precise in 32
/// bit floats.
const REVERSED_Z: Mat4 = Mat4::from_cols(
    Vec4::X,
    Vec4::Y,
    Vec4::new(0.0, 0.0, -1.0, 0.0),
    Vec4::new(0.0, 0.0, 1.0, 1.0),
);

// The uniform layouts. Each one matches a struct in the WGSL files, which describe the fields.

/// `Light` in `scene.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuLight {
    color: [f32; 4],
    position: [f32; 4],
    direction: [f32; 4],
    params: [f32; 4],
}

/// `SoftBox` in `scene.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuSoftBox {
    direction: [f32; 4],
    color: [f32; 4],
}

/// `Shadow` in `scene.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuShadow {
    view_proj: [f32; 16],
    params: [f32; 4],
    params2: [f32; 4],
}

/// `Frame` in `scene.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameUniform {
    view_proj: [f32; 16],
    inv_view_proj: [f32; 16],
    view_proj_rel: [f32; 16],
    camera: [f32; 4],
    forward: [f32; 4],
    viewport: [f32; 4],
    ambient: [f32; 4],
    hemi_sky: [f32; 4],
    hemi_ground: [f32; 4],
    fog: [f32; 4],
    fog_params: [f32; 4],
    env: [f32; 4],
    env_sky: [f32; 4],
    env_horizon: [f32; 4],
    env_ground: [f32; 4],
    background: [f32; 4],
    background_color: [f32; 4],
    counts: [f32; 4],
    clip_plane: [f32; 4],
    lights: [GpuLight; MAX_LIGHTS],
    softboxes: [GpuSoftBox; MAX_SOFTBOXES],
    shadows: [GpuShadow; MAX_SHADOWS],
}

/// `Draw` in `scene.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DrawUniform {
    model: [f32; 16],
    normal_matrix: [f32; 16],
    color: [f32; 4],
    emissive: [f32; 4],
    pbr: [f32; 4],
    flags: [f32; 4],
    maps: [f32; 4],
    attrs: [f32; 4],
    /// The repeat and offset of each map: color, emissive, metal-rough, normal, occlusion.
    uv_transforms: [[f32; 4]; 5],
    detail: [f32; 4],
}

/// `Post` in `post.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct PostUniform {
    dof: [f32; 4],
    bloom: [f32; 4],
    encode: [f32; 4],
    texel: [f32; 4],
}

/// `VertexIn` in `scene.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    color: [f32; 4],
}

/// One copy of a mesh, `InstanceIn` in `scene.wgsl`. Entry 0 of the instance buffer is the
/// identity, which every mesh without copies uses.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct InstanceData {
    model: [f32; 16],
    /// The normal matrix, as three columns padded to four floats.
    normal: [f32; 12],
    color: [f32; 4],
}

impl InstanceData {
    fn new(model: Mat4, color: Vec4) -> Self {
        let n = Mat3::from_mat4(model).inverse().transpose();
        Self {
            model: model.to_cols_array(),
            normal: [n.x_axis, n.y_axis, n.z_axis]
                .map(|c| c.extend(0.0).to_array())
                .concat()
                .try_into()
                .unwrap_or([0.0; 12]),
            color: color.to_array(),
        }
    }
}

/// The per copy attributes of the geometry pipelines, at locations 4 to 11.
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![
    4 => Float32x4, 5 => Float32x4, 6 => Float32x4, 7 => Float32x4,
    8 => Float32x4, 9 => Float32x4, 10 => Float32x4, 11 => Float32x4,
];

/// The model matrix of a copy, for the shadow pipeline, at locations 1 to 4.
const SHADOW_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
    1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4,
];

fn instance_layout(attributes: &[wgpu::VertexAttribute]) -> wgpu::VertexBufferLayout<'_> {
    wgpu::VertexBufferLayout {
        array_stride: size_of::<InstanceData>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes,
    }
}

/// `ParticleIn` in `scene.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ParticleInstance {
    position: [f32; 4],
    color: [f32; 4],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Blend {
    Opaque,
    Normal,
    Additive,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PipelineKey {
    cull: Option<wgpu::Face>,
    blend: Blend,
    depth_write: bool,
    variant: Variant,
    /// Draws into the reflection: one sample, and nothing behind the plane of the reflector.
    reflection_pass: bool,
}

/// The pipeline constants of a material (the `override`s of `scene.wgsl`). One shader for
/// all materials is too large to run at full SIMD width.
///
/// The field order sets the draw order of the opaque meshes (see [`draw_order`]).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct Variant {
    shading: u8,
    environment: u8,
    clearcoat: bool,
    textured: bool,
    fog: bool,
    lights: u8,
    reflection: bool,
    normal_map: bool,
}

impl Variant {
    /// The variant of a material. The light count stays 0, because it is known only after
    /// all nodes.
    fn new(m: &Material, has_uv: bool, scene: &Scene) -> Self {
        Self {
            lights: 0,
            shading: match m.shading {
                Shading::Standard => 0,
                Shading::Unlit => 1,
                Shading::ShadowCatcher => 2,
                Shading::Normals => 3,
            },
            environment: match scene.environment {
                Environment::None => 0,
                Environment::Studio(_) => 1,
                Environment::Map { .. } => 2,
            },
            clearcoat: m.clearcoat > 0.0,
            textured: has_uv
                && (m.map.is_some()
                    || m.emissive_map.is_some()
                    || m.metal_rough_map.is_some()
                    || m.ao_map.is_some()),
            fog: m.fog && scene.fog.is_some(),
            reflection: m.reflection > 0.0 && scene.reflector.is_some(),
            normal_map: has_uv && m.normal_map.is_some(),
        }
    }

    fn constants(self, reflection_pass: bool) -> [(&'static str, f64); 9] {
        [
            ("LIGHTS", f64::from(self.lights)),
            ("REFLECTION", f64::from(self.reflection && !reflection_pass)),
            ("REFLECTION_PASS", f64::from(reflection_pass)),
            ("SHADING", f64::from(self.shading)),
            ("ENVIRONMENT", f64::from(self.environment)),
            ("CLEARCOAT", f64::from(self.clearcoat)),
            ("TEXTURED", f64::from(self.textured)),
            ("FOG", f64::from(self.fog)),
            ("NORMAL_MAP", f64::from(self.normal_map)),
        ]
    }
}

struct GpuMesh {
    geometry: Weak<Geometry>,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
}

struct GpuTexture {
    source: Weak<crate::texture::Levels>,
    view: wgpu::TextureView,
}

struct EnvMapTexture {
    source: Weak<crate::EnvironmentMap>,
    view: wgpu::TextureView,
    levels: u32,
}

/// The free frame contexts and the number of contexts made so far.
struct ContextPool {
    free: Vec<FrameContext>,
    created: usize,
}

pub(crate) struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    /// The queue lock of the backend, when the backend shares the device.
    queue_lock: Option<Arc<Mutex<()>>>,
    /// The [`SharedGpu::id`] of the device, when the backend shares it.
    shared_id: Option<u64>,
    scene_module: wgpu::ShaderModule,
    frame_layout: wgpu::BindGroupLayout,
    draw_layout: wgpu::BindGroupLayout,
    shadow_layout: wgpu::BindGroupLayout,
    shadow_draw_layout: wgpu::BindGroupLayout,
    post_layout: wgpu::BindGroupLayout,
    scene_pipeline_layout: wgpu::PipelineLayout,
    pipelines: Mutex<HashMap<PipelineKey, wgpu::RenderPipeline>>,
    shadow_pipeline: wgpu::RenderPipeline,
    background_pipeline: wgpu::RenderPipeline,
    /// The background pipeline of the reflection (one sample).
    background_1x: wgpu::RenderPipeline,
    particle_additive: wgpu::RenderPipeline,
    particle_normal: wgpu::RenderPipeline,
    post: PostPipelines,
    /// Make the mipmap levels of an image on the GPU, see `mip.wgsl`.
    mip_layout: wgpu::BindGroupLayout,
    mip_level0: wgpu::RenderPipeline,
    mip_down: wgpu::RenderPipeline,
    linear_repeat: wgpu::Sampler,
    linear_clamp: wgpu::Sampler,
    env_sampler: wgpu::Sampler,
    shadow_sampler: wgpu::Sampler,
    white: wgpu::TextureView,
    black_hdr: wgpu::TextureView,
    // The uploaded meshes, textures and panoramas, by the address of their `Arc`. Each entry
    // holds a weak reference, so no other value gets this address while the entry exists.
    meshes: Mutex<HashMap<usize, GpuMesh>>,
    textures: Mutex<HashMap<(usize, bool), GpuTexture>>,
    env_maps: Mutex<HashMap<usize, EnvMapTexture>>,
    contexts: Mutex<ContextPool>,
    /// Signals that a frame context is free again.
    available: Condvar,
    /// The textures that frames render into as layers of the backend's frames.
    layers: Mutex<Vec<LayerTarget>>,
}

/// A texture that a frame renders into for `fframes::wgpu_layer`. It is free again when
/// the image of the frame is dropped. At that time the backend has submitted the frame that
/// samples it.
struct LayerTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    image: Weak<usvgr::PreloadedImageData>,
    reserved: bool,
}

struct PostPipelines {
    tile: wgpu::RenderPipeline,
    near: wgpu::RenderPipeline,
    half: wgpu::RenderPipeline,
    dof: wgpu::RenderPipeline,
    accumulate: wgpu::RenderPipeline,
    bright: wgpu::RenderPipeline,
    down: wgpu::RenderPipeline,
    /// `down` that keeps the coverage, for the reflection levels.
    down_rgba: wgpu::RenderPipeline,
    up: wgpu::RenderPipeline,
    encode: wgpu::RenderPipeline,
}

/// The texture that holds the finished HDR frame. A single moment needs no accumulation, so
/// the frame uses its target as it is.
#[derive(Clone, Copy)]
enum Accumulated {
    Accum,
    Hdr,
    Dof,
}

/// What the meshes of one batch share: the draw uniform without the matrices, the textures,
/// the variant, the side, the mirroring and the shadow casting.
type BatchKey = (Vec<u8>, Variant, u8, bool, bool);

/// A mesh of the frame, ready to draw.
struct DrawItem<'s> {
    material: &'s Material,
    geometry: Arc<Geometry>,
    uniform: DrawUniform,
    /// `None` for vertices in the stream buffer.
    vertices: Option<wgpu::Buffer>,
    vertex_offset: u64,
    /// `None` for indices in the stream index buffer (a batch).
    indices: Option<wgpu::Buffer>,
    index_offset: u64,
    index_count: u32,
    /// The copies in the instance buffer: entry 0 alone for a mesh without copies.
    first_instance: u32,
    instance_count: u32,
    /// Part of a batch. The batch draws it.
    batched: bool,
    cast_shadow: bool,
    mirrored: bool,
    depth: f32,
    bounds: crate::Aabb,
    variant: Variant,
}

impl DrawItem<'_> {
    /// Items of one rank use the same pipeline.
    fn pipeline_rank(&self) -> (Variant, u8, bool) {
        let cull = match cull_face(self.material.side, self.mirrored) {
            None => 0,
            Some(wgpu::Face::Back) => 1,
            Some(wgpu::Face::Front) => 2,
        };
        (self.variant, cull, self.material.depth_write)
    }

    /// Binds the vertices and indices of the item and draws them.
    fn draw(&self, pass: &mut wgpu::RenderPass<'_>, cx: &FrameContext) {
        // wgpu cannot bind an empty index buffer.
        if self.index_count == 0 {
            return;
        }
        let vertices = self.vertices.as_ref().unwrap_or(&cx.stream_buffer);
        let indices = self.indices.as_ref().unwrap_or(&cx.stream_index_buffer);
        pass.set_vertex_buffer(0, vertices.slice(self.vertex_offset..));
        pass.set_vertex_buffer(1, cx.instance_buffer.slice(..));
        pass.set_index_buffer(
            indices.slice(self.index_offset..),
            wgpu::IndexFormat::Uint32,
        );
        let instances = self.first_instance..self.first_instance + self.instance_count;
        pass.draw_indexed(0..self.index_count, 0, instances);
    }
}

/// The render targets and buffers of one frame in progress.
struct FrameContext {
    width: u32,
    height: u32,
    msaa_color: wgpu::TextureView,
    msaa_key: wgpu::TextureView,
    depth: wgpu::TextureView,
    hdr: wgpu::TextureView,
    key: wgpu::TextureView,
    tiles: wgpu::TextureView,
    /// The largest blur around each tile.
    near: wgpu::TextureView,
    /// The frame and its keys at half resolution, for the outer samples of the depth of field.
    half_hdr: wgpu::TextureView,
    half_key: wgpu::TextureView,
    dof: wgpu::TextureView,
    accum: wgpu::TextureView,
    /// The bloom levels with their sizes, each half the size of the one before.
    bloom: Vec<(wgpu::TextureView, u32, u32)>,
    output: wgpu::Texture,
    output_view: wgpu::TextureView,
    readback: wgpu::Buffer,
    padded_row: u32,
    shadow_size: u32,
    shadow_array: wgpu::TextureView,
    shadow_layers: Vec<wgpu::TextureView>,
    /// The average of the moments. Bloom and tone mapping read it.
    result: Accumulated,
    /// The scene as the reflector mirrors it, when the scene has a reflector.
    reflection: Option<ReflectionTargets>,
    frame_buffer: wgpu::Buffer,
    shadow_pass_buffer: wgpu::Buffer,
    post_buffer: wgpu::Buffer,
    draw_buffer: wgpu::Buffer,
    /// The vertices of the deformed meshes and of the batches.
    stream_buffer: wgpu::Buffer,
    /// The copies of the meshes, see [`InstanceData`].
    instance_buffer: wgpu::Buffer,
    /// The indices of the batches.
    stream_index_buffer: wgpu::Buffer,
    particle_buffer: wgpu::Buffer,
}

static OWN_GPU: OnceLock<Option<Arc<Gpu>>> = OnceLock::new();

/// The renderer on a device of its own. It is made on first use. `None` when no adapter is
/// usable.
fn own_gpu() -> Option<Arc<Gpu>> {
    OWN_GPU
        .get_or_init(|| match pollster::block_on(Gpu::new()) {
            Ok(gpu) => Some(Arc::new(gpu)),
            Err(e) => {
                eprintln!("fframes_three_d: no GPU available ({e}), rendering on the CPU");
                None
            }
        })
        .clone()
}

/// The renderer on the GPU of the backend, when the backend shares it.
fn shared_gpu() -> Option<(SharedGpu, Arc<Gpu>)> {
    let shared = wgpu_layer::shared_gpu()?;
    let gpu = shared.state(|shared| {
        Gpu::with_device(
            shared.device().clone(),
            shared.queue().clone(),
            Some((shared.queue_lock(), shared.id())),
        )
    });
    Some((shared, gpu))
}

/// The renderer for frames that are read back as pixels. It uses the GPU of the backend when
/// the backend shares it, so that there is only one device. Otherwise it uses its own device.
pub(crate) fn gpu() -> Option<Arc<Gpu>> {
    shared_gpu().map(|(_, gpu)| gpu).or_else(own_gpu)
}

/// Renders the moments (one, or more for motion blur) as a layer of the backend's frame.
/// `None` when the backend does not share its GPU.
pub(crate) fn layer(
    renderer: &Renderer,
    moments: &[(&Scene, &Camera)],
) -> Option<fframes_media::ImageData<'static>> {
    let (shared, gpu) = shared_gpu()?;
    Some(gpu.render_layer(renderer, moments, &shared))
}

/// Locks `mutex`. The data stays usable after a panic on another thread that held the lock.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Converts a finite `f32` to the bits of an `f16`, clamped to the `f16` range. This is
/// enough for HDR panoramas.
fn to_f16(value: f32) -> u16 {
    let value = value.clamp(-65504.0, 65504.0);
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    let mantissa = bits & 0x007f_ffff;
    if exponent <= 0 {
        if exponent < -10 {
            return sign;
        }
        let m = (mantissa | 0x0080_0000) >> (1 - exponent);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    if exponent >= 31 {
        return sign | 0x7bff;
    }

    sign | ((exponent as u16) << 10) | ((mantissa + 0x1000) >> 13) as u16
}

impl Gpu {
    async fn new() -> Result<Gpu, String> {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::PRIMARY;
        let instance = wgpu::Instance::new(descriptor);
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|e| e.to_string())?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("fframes_three_d"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        Ok(Self::with_device(device, queue, None))
    }

    /// The renderer on `device`. When the backend shares the device, `shared` is its queue
    /// lock and its [`SharedGpu::id`]. The renderer holds the lock while it submits.
    fn with_device(
        device: wgpu::Device,
        queue: wgpu::Queue,
        shared: Option<(Arc<Mutex<()>>, u64)>,
    ) -> Gpu {
        let (queue_lock, shared_id) = shared.unzip();
        let scene_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene"),
            source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
        });
        let shadow_module = device.create_shader_module(wgpu::include_wgsl!("shadow.wgsl"));
        let post_module = device.create_shader_module(wgpu::include_wgsl!("post.wgsl"));

        let uniform = |binding: u32, dynamic: bool, visibility: wgpu::ShaderStages| {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: dynamic,
                    min_binding_size: None,
                },
                count: None,
            }
        };
        let texture = |binding: u32,
                       sample_type: wgpu::TextureSampleType,
                       view_dimension: wgpu::TextureViewDimension| {
            wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type,
                    view_dimension,
                    multisampled: false,
                },
                count: None,
            }
        };
        let float_2d = |binding: u32| {
            texture(
                binding,
                wgpu::TextureSampleType::Float { filterable: true },
                wgpu::TextureViewDimension::D2,
            )
        };
        let sampler = |binding: u32, kind: wgpu::SamplerBindingType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(kind),
            count: None,
        };
        let filtering = wgpu::SamplerBindingType::Filtering;
        let bind_group_layout = |label: &str, entries: &[wgpu::BindGroupLayoutEntry]| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
                entries,
            })
        };
        let frame_layout = bind_group_layout(
            "frame",
            &[
                uniform(0, false, wgpu::ShaderStages::VERTEX_FRAGMENT),
                texture(
                    1,
                    wgpu::TextureSampleType::Depth,
                    wgpu::TextureViewDimension::D2Array,
                ),
                sampler(2, wgpu::SamplerBindingType::Comparison),
                float_2d(3),
                sampler(4, filtering),
                float_2d(5),
                sampler(6, filtering),
            ],
        );
        let draw_layout = bind_group_layout(
            "draw",
            &[
                uniform(0, true, wgpu::ShaderStages::VERTEX_FRAGMENT),
                float_2d(1),
                float_2d(2),
                float_2d(3),
                sampler(4, filtering),
                float_2d(5),
                float_2d(6),
            ],
        );
        let shadow_layout = bind_group_layout(
            "shadow pass",
            &[uniform(0, false, wgpu::ShaderStages::VERTEX)],
        );
        let shadow_draw_layout = bind_group_layout(
            "shadow draw",
            &[uniform(0, true, wgpu::ShaderStages::VERTEX)],
        );
        let post_layout = bind_group_layout(
            "post",
            &[
                uniform(0, false, wgpu::ShaderStages::FRAGMENT),
                float_2d(1),
                sampler(2, filtering),
                float_2d(3),
                float_2d(4),
                float_2d(5),
                float_2d(6),
            ],
        );

        let pipeline_layout = |label: &str, groups: &[Option<&wgpu::BindGroupLayout>]| {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(label),
                bind_group_layouts: groups,
                ..Default::default()
            })
        };
        let scene_pipeline_layout =
            pipeline_layout("scene", &[Some(&frame_layout), Some(&draw_layout)]);
        let shadow_pipeline_layout =
            pipeline_layout("shadow", &[Some(&shadow_layout), Some(&shadow_draw_layout)]);
        let frame_only_layout = pipeline_layout("frame only", &[Some(&frame_layout)]);
        let post_pipeline_layout = pipeline_layout("post", &[Some(&post_layout)]);

        let shadow_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shadow"),
            layout: Some(&shadow_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shadow_module,
                entry_point: Some("vs_shadow"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                    }),
                    Some(instance_layout(&SHADOW_INSTANCE_ATTRIBUTES)),
                ],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(depth_test(true, wgpu::CompareFunction::Less)),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shadow_module,
                entry_point: Some("fs_shadow"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[],
            }),
            multiview_mask: None,
            cache: None,
        });

        let msaa = wgpu::MultisampleState {
            count: MSAA,
            ..Default::default()
        };
        let background = |multisample: wgpu::MultisampleState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("background"),
                layout: Some(&frame_only_layout),
                vertex: wgpu::VertexState {
                    module: &scene_module,
                    entry_point: Some("vs_fullscreen"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(depth_test(false, wgpu::CompareFunction::Always)),
                multisample,
                fragment: Some(wgpu::FragmentState {
                    module: &scene_module,
                    entry_point: Some("fs_background"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &scene_targets(None),
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let particle = |entry: &str, blend: wgpu::BlendState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&frame_only_layout),
                vertex: wgpu::VertexState {
                    module: &scene_module,
                    entry_point: Some("vs_particle"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<ParticleInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4],
                    })],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(depth_test(false, wgpu::CompareFunction::Greater)),
                multisample: msaa,
                fragment: Some(wgpu::FragmentState {
                    module: &scene_module,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &scene_targets(Some(blend)),
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let post_pipeline = |entry: &str, targets: &[Option<wgpu::ColorTargetState>]| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&post_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &post_module,
                    entry_point: Some("vs_fullscreen"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &post_module,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let added_hdr = [Some(wgpu::ColorTargetState {
            blend: Some(wgpu::BlendState::ADDITIVE),
            ..HDR.into()
        })];
        let post = PostPipelines {
            tile: post_pipeline("fs_tile", &[Some(KEY.into())]),
            near: post_pipeline("fs_near", &[Some(KEY.into())]),
            half: post_pipeline("fs_half", &[Some(HDR.into()), Some(KEY.into())]),
            dof: post_pipeline("fs_dof", &[Some(HDR.into())]),
            accumulate: post_pipeline("fs_accumulate", &added_hdr),
            bright: post_pipeline("fs_bright", &[Some(HDR.into())]),
            down: post_pipeline("fs_down", &[Some(HDR.into())]),
            down_rgba: post_pipeline("fs_down_rgba", &[Some(HDR.into())]),
            up: post_pipeline("fs_up", &added_hdr),
            encode: post_pipeline("fs_encode", &[Some(OUTPUT.into())]),
        };

        let mip_module = device.create_shader_module(wgpu::include_wgsl!("mip.wgsl"));
        let mip_layout = bind_group_layout("mip", &[float_2d(0)]);
        let mip_pipeline_layout = pipeline_layout("mip", &[Some(&mip_layout)]);
        let mip_pipeline = |entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&mip_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &mip_module,
                    entry_point: Some("vs_fullscreen"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &mip_module,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::TextureFormat::Rgba8UnormSrgb.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let (mip_level0, mip_down) = (mip_pipeline("fs_level0"), mip_pipeline("fs_down"));

        let linear = |address_mode_u, address_mode_v, anisotropy_clamp| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                address_mode_u,
                address_mode_v,
                address_mode_w: address_mode_u,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                anisotropy_clamp,
                ..Default::default()
            })
        };
        let (repeat, clamp) = (wgpu::AddressMode::Repeat, wgpu::AddressMode::ClampToEdge);
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let texel = |format: wgpu::TextureFormat, data: &[u8]| {
            device
                .create_texture_with_data(
                    &queue,
                    &texture_desc(
                        "single texel",
                        1,
                        1,
                        format,
                        wgpu::TextureUsages::TEXTURE_BINDING,
                    ),
                    wgpu::util::TextureDataOrder::LayerMajor,
                    data,
                )
                .create_view(&wgpu::TextureViewDescriptor::default())
        };

        Gpu {
            white: texel(wgpu::TextureFormat::Rgba8Unorm, &[255; 4]),
            black_hdr: texel(HDR, &[0; 8]),
            linear_repeat: linear(repeat, repeat, 8),
            linear_clamp: linear(clamp, clamp, 1),
            env_sampler: linear(repeat, clamp, 1),
            shadow_sampler,
            shadow_pipeline,
            background_pipeline: background(msaa),
            background_1x: background(wgpu::MultisampleState::default()),
            particle_additive: particle("fs_particle_additive", ADD_LIGHT),
            particle_normal: particle(
                "fs_particle_normal",
                wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING,
            ),
            post,
            mip_layout,
            mip_level0,
            mip_down,
            device,
            queue,
            queue_lock,
            shared_id,
            scene_module,
            frame_layout,
            draw_layout,
            shadow_layout,
            shadow_draw_layout,
            post_layout,
            scene_pipeline_layout,
            pipelines: Mutex::new(HashMap::new()),
            meshes: Mutex::new(HashMap::new()),
            textures: Mutex::new(HashMap::new()),
            env_maps: Mutex::new(HashMap::new()),
            contexts: Mutex::new(ContextPool {
                free: Vec::new(),
                created: 0,
            }),
            available: Condvar::new(),
            layers: Mutex::new(Vec::new()),
        }
    }

    /// Submits `commands`. Holds the queue lock of the backend when the queue is shared.
    fn submit(&self, commands: wgpu::CommandBuffer) -> wgpu::SubmissionIndex {
        let _queue = self.queue_lock.as_deref().map(lock);
        self.queue.submit([commands])
    }

    /// The geometry pipeline for `key`, made on first use.
    fn pipeline(&self, key: PipelineKey) -> wgpu::RenderPipeline {
        let mut pipelines = lock(&self.pipelines);
        if let Some(pipeline) = pipelines.get(&key) {
            return pipeline.clone();
        }
        let blend = match key.blend {
            Blend::Opaque => None,
            Blend::Normal => Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            Blend::Additive => Some(ADD_LIGHT),
        };
        let constants = key.variant.constants(key.reflection_pass);
        let pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("geometry"),
                layout: Some(&self.scene_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &self.scene_module,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[
                        Some(wgpu::VertexBufferLayout {
                            array_stride: size_of::<Vertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4],
                        }),
                        Some(instance_layout(&INSTANCE_ATTRIBUTES)),
                    ],
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: key.cull,
                    ..Default::default()
                },
                depth_stencil: Some(depth_test(key.depth_write, wgpu::CompareFunction::Greater)),
                multisample: wgpu::MultisampleState {
                    count: if key.reflection_pass { 1 } else { MSAA },
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &self.scene_module,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions {
                        constants: &constants,
                        ..Default::default()
                    },
                    targets: &scene_targets(blend),
                }),
                multiview_mask: None,
                cache: None,
            });
        pipelines.insert(key, pipeline.clone());
        pipeline
    }

    /// The vertex buffer, the index buffer and the index count of a rigid mesh. The mesh is
    /// uploaded on first use.
    fn mesh(&self, geometry: &Arc<Geometry>) -> (wgpu::Buffer, wgpu::Buffer, u32) {
        let key = Arc::as_ptr(geometry) as usize;
        let mut meshes = lock(&self.meshes);
        if let Some(m) = meshes.get(&key) {
            return (m.vertices.clone(), m.indices.clone(), m.index_count);
        }
        if meshes.len() > 64 {
            meshes.retain(|_, m| m.geometry.strong_count() > 0);
        }
        let indices: Vec<u32> = geometry
            .indices
            .iter()
            .copied()
            .filter(|&i| (i as usize) < geometry.positions.len())
            .collect();
        let mesh = meshes.entry(key).or_insert(GpuMesh {
            geometry: Arc::downgrade(geometry),
            vertices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("mesh vertices"),
                    contents: bytemuck::cast_slice(&vertices_of(geometry, None)),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
            indices: self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("mesh indices"),
                    contents: bytemuck::cast_slice(&indices),
                    usage: wgpu::BufferUsages::INDEX,
                }),
            index_count: indices.len() as u32,
        });
        (
            mesh.vertices.clone(),
            mesh.indices.clone(),
            mesh.index_count,
        )
    }

    /// The view of a material texture with its mipmaps, made on first use. Textures that
    /// share their levels share the view.
    fn texture(&self, texture: &Arc<Texture>) -> wgpu::TextureView {
        let levels = texture.shared_levels();
        let key = (Arc::as_ptr(levels) as usize, texture.srgb);
        let mut textures = lock(&self.textures);
        if let Some(t) = textures.get(&key) {
            return t.view.clone();
        }
        textures.retain(|_, t| t.source.strong_count() > 0);
        let on_this_gpu =
            (levels.layer()).filter(|layer| texture.srgb && Some(layer.gpu_id()) == self.shared_id);
        let view = match on_this_gpu {
            Some(layer) => self.mipmaps(layer.texture()),
            None => self.upload(texture),
        };
        let entry = textures.entry(key).or_insert(GpuTexture {
            source: Arc::downgrade(levels),
            view,
        });
        entry.view.clone()
    }

    /// Uploads the mipmap levels that the texture made on the CPU.
    fn upload(&self, texture: &Texture) -> wgpu::TextureView {
        let format = if texture.srgb {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let gpu_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            mip_level_count: texture.levels().count() as u32,
            ..texture_desc(
                "material texture",
                texture.width,
                texture.height,
                format,
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            )
        });
        for (level, (width, height, texels)) in texture.levels().enumerate() {
            self.write_level(
                &gpu_texture,
                level as u32,
                width,
                height,
                bytemuck::cast_slice(texels),
            );
        }
        gpu_texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    /// A color texture with the mipmap levels of `image` (premultiplied sRGB on this
    /// device), made on the GPU like the CPU makes them. The image does not leave the GPU.
    fn mipmaps(&self, image: &wgpu::Texture) -> wgpu::TextureView {
        let (width, height) = (image.width(), image.height());
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            mip_level_count: width.max(height).ilog2() + 1,
            ..texture_desc(
                "material texture",
                width,
                height,
                wgpu::TextureFormat::Rgba8UnormSrgb,
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            )
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let mut source = image.create_view(&wgpu::TextureViewDescriptor::default());
        for level in 0..texture.mip_level_count() {
            let target = texture.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: level,
                mip_level_count: Some(1),
                ..Default::default()
            });
            let group = self.bind_group(
                "mip",
                &self.mip_layout,
                &[wgpu::BindingResource::TextureView(&source)],
            );
            let pipeline = if level == 0 {
                &self.mip_level0
            } else {
                &self.mip_down
            };
            fullscreen(&mut encoder, pipeline, &[&group], &[&target], None);
            source = target;
        }
        self.submit(encoder.finish());
        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    /// The view and the level count of an environment panorama, uploaded on first use.
    fn env_map(&self, map: &Arc<crate::EnvironmentMap>) -> (wgpu::TextureView, u32) {
        let key = Arc::as_ptr(map) as usize;
        let mut maps = lock(&self.env_maps);
        if let Some(t) = maps.get(&key) {
            return (t.view.clone(), t.levels);
        }
        maps.retain(|_, t| t.source.strong_count() > 0);
        let levels: Vec<_> = map.levels().collect();
        let (width, height, _) = levels[0];
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            mip_level_count: levels.len() as u32,
            ..texture_desc(
                "environment",
                width as u32,
                height as u32,
                HDR,
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            )
        });
        for (level, (width, height, texels)) in levels.iter().enumerate() {
            let half: Vec<u16> = texels
                .iter()
                .flat_map(|c| [to_f16(c.x), to_f16(c.y), to_f16(c.z), to_f16(1.0)])
                .collect();
            self.write_level(
                &texture,
                level as u32,
                *width as u32,
                *height as u32,
                bytemuck::cast_slice(&half),
            );
        }
        let entry = maps.entry(key).or_insert(EnvMapTexture {
            source: Arc::downgrade(map),
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            levels: levels.len() as u32,
        });
        (entry.view.clone(), entry.levels)
    }

    /// Uploads one mipmap level. `data` holds the rows without padding.
    fn write_level(
        &self,
        texture: &wgpu::Texture,
        level: u32,
        width: u32,
        height: u32,
        data: &[u8],
    ) {
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                mip_level: level,
                ..texture.as_image_copy()
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(data.len() as u32 / height),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Writes `data` to the start of `buffer`. When the data does not fit, the buffer is
    /// replaced by a larger one (the next power of two).
    fn write_growing(&self, buffer: &mut wgpu::Buffer, label: &str, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        if data.len() as u64 > buffer.size() {
            *buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: (data.len() as u64).next_power_of_two(),
                usage: buffer.usage(),
                mapped_at_creation: false,
            });
        }
        self.queue.write_buffer(buffer, 0, data);
    }

    /// A bind group with `resources` at bindings 0, 1, 2 and so on.
    fn bind_group(
        &self,
        label: &str,
        layout: &wgpu::BindGroupLayout,
        resources: &[wgpu::BindingResource<'_>],
    ) -> wgpu::BindGroup {
        let entries: Vec<wgpu::BindGroupEntry> = resources
            .iter()
            .enumerate()
            .map(|(binding, resource)| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: resource.clone(),
            })
            .collect();
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout,
            entries: &entries,
        })
    }

    /// The bind group of a mesh: its draw uniform at a dynamic offset and its textures.
    fn draw_group(&self, cx: &FrameContext, material: &Material) -> wgpu::BindGroup {
        let view = |t: &Option<Arc<Texture>>| {
            t.as_ref()
                .map_or_else(|| self.white.clone(), |t| self.texture(t))
        };
        let map = view(&material.map);
        let emissive = view(&material.emissive_map);
        let metal_rough = view(&material.metal_rough_map);
        let normal = view(&material.normal_map);
        let occlusion = view(&material.ao_map);
        self.bind_group(
            "draw",
            &self.draw_layout,
            &[
                buffer_range(&cx.draw_buffer, 0, size_of::<DrawUniform>()),
                wgpu::BindingResource::TextureView(&map),
                wgpu::BindingResource::TextureView(&emissive),
                wgpu::BindingResource::TextureView(&metal_rough),
                wgpu::BindingResource::Sampler(&self.linear_repeat),
                wgpu::BindingResource::TextureView(&normal),
                wgpu::BindingResource::TextureView(&occlusion),
            ],
        )
    }

    /// A bind group of `post.wgsl` with the uniform in `slot` of the post buffer. The
    /// textures are `src`, `depth_key`, `tiles`, `aux_tex` and `half_key` in this order.
    fn post_group(
        &self,
        cx: &FrameContext,
        slot: u64,
        [src, key, tiles, aux, half_key]: [&wgpu::TextureView; 5],
    ) -> wgpu::BindGroup {
        self.bind_group(
            "post",
            &self.post_layout,
            &[
                buffer_range(
                    &cx.post_buffer,
                    slot * UNIFORM_ALIGNMENT,
                    size_of::<PostUniform>(),
                ),
                wgpu::BindingResource::TextureView(src),
                wgpu::BindingResource::Sampler(&self.linear_clamp),
                wgpu::BindingResource::TextureView(key),
                wgpu::BindingResource::TextureView(tiles),
                wgpu::BindingResource::TextureView(aux),
                wgpu::BindingResource::TextureView(half_key),
            ],
        )
    }

    /// Takes a free frame context of this size from the pool, or makes one. Waits while all
    /// contexts are in use. The shadow maps get exactly `shadow_size` texels, as on the CPU:
    /// a context that kept larger maps from an earlier shot would shade the frame with
    /// another texel size and normal offset than a fresh one. `None` keeps the maps.
    fn take_context(&self, width: u32, height: u32, shadow_size: Option<u32>) -> FrameContext {
        let mut pool = lock(&self.contexts);
        loop {
            let fits = |c: &FrameContext| c.width == width && c.height == height;
            // A context with maps of this size first, so that shots do not reallocate them.
            let found = (pool.free.iter())
                .position(|c| fits(c) && shadow_size.is_none_or(|s| c.shadow_size == s))
                .or_else(|| pool.free.iter().position(fits));
            if let Some(i) = found {
                let mut context = pool.free.swap_remove(i);
                drop(pool);
                if let Some(size) = shadow_size.filter(|&s| s != context.shadow_size) {
                    context.resize_shadows(&self.device, size);
                }
                return context;
            }
            if pool.created < CONTEXTS {
                pool.created += 1;
                break;
            }
            // Drops a free context of another size to make room for a new one.
            if pool.free.pop().is_some() {
                break;
            }
            pool = self
                .available
                .wait(pool)
                .unwrap_or_else(PoisonError::into_inner);
        }
        drop(pool);
        FrameContext::new(&self.device, width, height, shadow_size.unwrap_or(1024))
    }

    /// Puts a frame context back into the pool.
    fn give_back(&self, context: FrameContext) {
        lock(&self.contexts).free.push(context);
        self.available.notify_one();
    }

    /// Renders the moments (one, or more for motion blur) and returns the pixels.
    pub(crate) fn render(&self, renderer: &Renderer, moments: &[(&Scene, &Camera)]) -> Pixels {
        let context = self.render_moments(renderer, moments);
        let pixels = self.finish(&context, renderer);
        self.give_back(context);
        pixels
    }

    /// Renders the moments into a texture that the backend samples in place, see
    /// `fframes::wgpu_layer`. Does not wait for the GPU.
    fn render_layer(
        &self,
        renderer: &Renderer,
        moments: &[(&Scene, &Camera)],
        shared: &SharedGpu,
    ) -> fframes_media::ImageData<'static> {
        let context = self.render_moments(renderer, moments);
        let (slot, texture, view) = self.take_layer(context.width, context.height);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.post_process(&context, renderer, &mut encoder, &view);
        wgpu_layer::finish_wgpu_layer(&mut encoder, &texture);
        self.submit(encoder.finish());
        self.give_back(context);
        let image = wgpu_layer::wgpu_layer(shared, &texture);
        let mut layers = lock(&self.layers);
        layers[slot].image = Arc::downgrade(&image.href());
        layers[slot].reserved = false;
        drop(layers);
        // Releases the resources of finished submissions. This does not wait for the GPU.
        let _ = self.device.poll(wgpu::PollType::Poll);
        image
    }

    /// A free layer texture of this size, or a new one. A layer is free when its frame is done.
    fn take_layer(&self, width: u32, height: u32) -> (usize, wgpu::Texture, wgpu::TextureView) {
        let mut layers = lock(&self.layers);
        if let Some(slot) = layers.iter().position(|l| {
            !l.reserved
                && l.image.strong_count() == 0
                && l.texture.width() == width
                && l.texture.height() == height
        }) {
            let layer = &mut layers[slot];
            layer.reserved = true;
            return (slot, layer.texture.clone(), layer.view.clone());
        }
        let texture = self.device.create_texture(&texture_desc(
            "fframes_three_d layer",
            width,
            height,
            wgpu_layer::WGPU_LAYER_FORMAT,
            wgpu_layer::WGPU_LAYER_USAGE | wgpu::TextureUsages::RENDER_ATTACHMENT,
        ));
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        layers.push(LayerTarget {
            texture: texture.clone(),
            view: view.clone(),
            image: Weak::new(),
            reserved: true,
        });
        (layers.len() - 1, texture, view)
    }

    /// Renders the moments into a frame context from the pool and averages them.
    fn render_moments(&self, renderer: &Renderer, moments: &[(&Scene, &Camera)]) -> FrameContext {
        let shadow_size = moments
            .iter()
            .flat_map(|(scene, _)| {
                scene.nodes().filter_map(|(_, n)| match &n.object {
                    Object::Light(crate::Light::Directional(l)) => {
                        l.shadow.as_ref().map(|s| s.map_size.clamp(64, 8192))
                    }
                    _ => None,
                })
            })
            .max();
        let mut context = self.take_context(renderer.width, renderer.height, shadow_size);
        let weight = 1.0 / moments.len().max(1) as f32;
        for (i, (scene, camera)) in moments.iter().enumerate() {
            self.render_moment(&mut context, scene, camera, weight, i, moments.len() == 1);
        }
        context
    }

    /// Renders the moment with index `moment` and adds it with `weight` to the average of
    /// the frame. Moment 0 starts a new average. `single` tells that the moment is the
    /// whole frame.
    fn render_moment(
        &self,
        cx: &mut FrameContext,
        scene: &Scene,
        camera: &Camera,
        weight: f32,
        moment: usize,
        single: bool,
    ) {
        let (width, height) = (cx.width, cx.height);
        let worlds = scene.world_matrices();
        let visible = scene.visibilities();
        let view = camera.view_matrix();

        // The lights, the meshes and the particles of the scene.
        let mut lights = Lights {
            direct: Vec::new(),
            ambient: Vec3::ZERO,
            hemisphere: None,
            shadow_casters: Vec::new(),
        };
        let mut items: Vec<DrawItem> = Vec::new();
        let mut streamed: Vec<Vertex> = Vec::new();
        let mut instances = vec![InstanceData::new(Mat4::IDENTITY, Vec4::ONE)];
        let mut additive: Vec<ParticleInstance> = Vec::new();
        let mut normal: Vec<ParticleInstance> = Vec::new();
        for (id, node) in scene.nodes() {
            if !visible[id.index()] {
                continue;
            }
            let world = worlds[id.index()];
            match &node.object {
                Object::Group => {}
                Object::Light(light) => prepare::push_light(&mut lights, light, &world),
                Object::Particles(particles) => {
                    let scale = world.x_axis.truncate().length();
                    let target = if particles.blending == Blending::Additive {
                        &mut additive
                    } else {
                        &mut normal
                    };
                    for p in &particles.items {
                        if p.opacity <= 0.0 {
                            continue;
                        }
                        target.push(ParticleInstance {
                            position: world
                                .transform_point3(p.position)
                                .extend(p.size * scale)
                                .to_array(),
                            color: (p.color.0 * p.opacity).extend(p.opacity).to_array(),
                        });
                    }
                }
                Object::Mesh(mesh) => {
                    let g = &mesh.geometry;
                    if g.indices.is_empty() || g.positions.is_empty() {
                        continue;
                    }
                    let (vertices, indices, index_count) = self.mesh(g);
                    // A skinned or morphed mesh streams its vertices in world space.
                    let (model, normal_matrix, vertices, vertex_offset, bounds) =
                        match prepare::deformed(mesh, &world, &worlds) {
                            Some((positions, normals)) => {
                                let offset = (streamed.len() * size_of::<Vertex>()) as u64;
                                let bounds = crate::Aabb::from_points(positions.iter().copied());
                                streamed.extend(vertices_of(g, Some((&positions, &normals))));
                                (Mat4::IDENTITY, Mat4::IDENTITY, None, offset, bounds)
                            }
                            None => (
                                world,
                                Mat4::from_mat3(Mat3::from_mat4(world).inverse().transpose()),
                                Some(vertices),
                                0,
                                g.bounds().transformed(&world),
                            ),
                        };
                    // The copies of a rigid mesh go into the instance buffer.
                    let (first_instance, instance_count, bounds) = match &mesh.instances {
                        Some(copies) if vertices.is_some() => {
                            let first = instances.len() as u32;
                            instances.extend(
                                (copies.transforms.iter().enumerate())
                                    .map(|(i, t)| InstanceData::new(*t, copies.color(i))),
                            );
                            (first, copies.transforms.len() as u32, mesh.bounds(&world))
                        }
                        _ => (0, 1, bounds),
                    };
                    let material = &mesh.material;
                    items.push(DrawItem {
                        material,
                        geometry: Arc::clone(g),
                        uniform: draw_uniform(mesh, model, normal_matrix),
                        vertices,
                        vertex_offset,
                        indices: Some(indices),
                        index_offset: 0,
                        index_count,
                        first_instance,
                        instance_count,
                        batched: false,
                        cast_shadow: mesh.cast_shadow
                            && !material.is_blended()
                            && material.shading != Shading::ShadowCatcher,
                        mirrored: mesh.skin.is_none() && world.determinant() < 0.0,
                        depth: -view.transform_point3(bounds.center()).z,
                        bounds,
                        variant: Variant::new(material, !g.uvs.is_empty(), scene),
                    });
                }
            }
        }
        let light_count = lights.direct.len().min(MAX_LIGHTS);
        for item in &mut items {
            item.variant.lights = light_count as u8;
        }
        let stream_indices = batch(&mut items, &mut streamed);
        let order = draw_order(&items);

        // The uploads: the streamed vertices, the draw uniforms and the particles. The
        // particle buffer holds the additive particles first.
        self.write_growing(
            &mut cx.stream_buffer,
            "streamed vertices",
            bytemuck::cast_slice(&streamed),
        );
        self.write_growing(
            &mut cx.stream_index_buffer,
            "streamed indices",
            bytemuck::cast_slice(&stream_indices),
        );
        self.write_growing(
            &mut cx.instance_buffer,
            "instances",
            bytemuck::cast_slice(&instances),
        );
        let mut draws = vec![0u8; items.len().max(1) * DRAW_STRIDE as usize];
        for (chunk, item) in draws.chunks_mut(DRAW_STRIDE as usize).zip(&items) {
            chunk[..size_of::<DrawUniform>()].copy_from_slice(bytemuck::bytes_of(&item.uniform));
        }
        self.write_growing(&mut cx.draw_buffer, "draws", &draws);
        let additive_count = additive.len() as u32;
        let particles = [additive, normal].concat();
        let normal_count = particles.len() as u32 - additive_count;
        self.write_growing(
            &mut cx.particle_buffer,
            "particles",
            bytemuck::cast_slice(&particles),
        );

        // The frame uniform.
        let projection = REVERSED_Z * camera.projection_matrix(width as f32 / height as f32);
        let view_proj = projection * view;
        let focal = match camera.ortho_height {
            Some(h) => height as f32 / h,
            None => height as f32 / (2.0 * (camera.fov.to_radians() * 0.5).tan()),
        };
        let mut frame = FrameUniform::zeroed();
        frame.view_proj = view_proj.to_cols_array();
        frame.inv_view_proj = view_proj.inverse().to_cols_array();
        frame.view_proj_rel = (projection * relative(view)).to_cols_array();
        frame.camera = camera
            .position
            .extend(f32::from(camera.ortho_height.is_some()))
            .to_array();
        frame.forward = camera.forward().extend(0.0).to_array();
        frame.viewport = [width as f32, height as f32, focal, camera.near];
        frame.ambient = lights
            .ambient
            .extend(f32::from(lights.hemisphere.is_some()))
            .to_array();
        if let Some((sky, ground)) = lights.hemisphere {
            frame.hemi_sky = sky.extend(0.0).to_array();
            frame.hemi_ground = ground.extend(0.0).to_array();
        }
        let env = self.set_environment(&mut frame, scene);
        let shadows = fit_shadows(&lights, &items, cx.shadow_size);
        for (uniform, shadow) in frame.shadows.iter_mut().zip(&shadows) {
            *uniform = shadow.unwrap_or_else(GpuShadow::zeroed);
        }
        for (uniform, light) in frame.lights.iter_mut().zip(&lights.direct) {
            *uniform = gpu_light(light, &shadows);
        }

        // The targets and the frame uniform of the reflection.
        let reflector = scene
            .reflector
            .filter(|_| items.iter().any(|it| it.material.reflection > 0.0));
        if let Some(reflector) = reflector {
            let size = |pixels: u32| (pixels as f32 * reflector.scale).ceil().max(1.0) as u32;
            cx.prepare_reflection(&self.device, size(width), size(height));
        }
        let reflection = reflector.and(cx.reflection.as_ref());
        frame.counts = [
            light_count as f32,
            env.as_ref().map_or(0.0, |(_, levels)| *levels as f32),
            reflection.map_or(0.0, |r| r.levels as f32),
            0.0,
        ];
        if let (Some(reflector), Some(targets)) = (reflector, reflection) {
            let (rw, rh) = (targets.width as f32, targets.height as f32);
            let mirror = reflector.matrix();
            let view_proj = projection * view * mirror;
            let mut mirrored = frame;
            mirrored.view_proj = view_proj.to_cols_array();
            mirrored.inv_view_proj = view_proj.inverse().to_cols_array();
            mirrored.view_proj_rel = (projection * relative(view * mirror)).to_cols_array();
            mirrored.camera = mirror
                .transform_point3(camera.position)
                .extend(frame.camera[3])
                .to_array();
            mirrored.forward = mirror
                .transform_vector3(camera.forward())
                .extend(0.0)
                .to_array();
            mirrored.viewport = [rw, rh, focal * rh / height as f32, camera.near];
            mirrored.clip_plane = reflector.plane().to_array();
            self.queue
                .write_buffer(&targets.frame_buffer, 0, bytemuck::bytes_of(&mirrored));
        }
        self.queue
            .write_buffer(&cx.frame_buffer, 0, bytemuck::bytes_of(&frame));

        // The bind groups.
        let env_view = env.map_or_else(|| self.black_hdr.clone(), |(view, _)| view);
        let frame_group = |buffer: &wgpu::Buffer, reflection: &wgpu::TextureView| {
            self.bind_group(
                "frame",
                &self.frame_layout,
                &[
                    buffer.as_entire_binding(),
                    wgpu::BindingResource::TextureView(&cx.shadow_array),
                    wgpu::BindingResource::Sampler(&self.shadow_sampler),
                    wgpu::BindingResource::TextureView(&env_view),
                    wgpu::BindingResource::Sampler(&self.env_sampler),
                    wgpu::BindingResource::TextureView(reflection),
                    wgpu::BindingResource::Sampler(&self.linear_clamp),
                ],
            )
        };
        let draw_groups: Vec<wgpu::BindGroup> = items
            .iter()
            .map(|item| self.draw_group(cx, item.material))
            .collect();
        let draw_meshes = |pass: &mut wgpu::RenderPass<'_>, reflection_pass: bool| {
            self.draw_items(pass, &items, &order, &draw_groups, cx, reflection_pass);
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.draw_shadow_maps(&mut encoder, cx, &items, &shadows);

        let clear = match &scene.background {
            Background::Color(c) => wgpu::Color {
                r: f64::from(c.0.x),
                g: f64::from(c.0.y),
                b: f64::from(c.0.z),
                a: 1.0,
            },
            _ => wgpu::Color::TRANSPARENT,
        };
        let has_background = matches!(scene.background, Background::Environment { .. });

        // The reflection pass: the scene without the reflectors, seen in the mirror.
        if let Some(targets) = reflection {
            let group = frame_group(&targets.frame_buffer, &self.black_hdr);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("reflection"),
                color_attachments: &[
                    Some(color_attachment(
                        &targets.level_views[0],
                        None,
                        wgpu::LoadOp::Clear(clear),
                        wgpu::StoreOp::Store,
                    )),
                    Some(color_attachment(
                        &targets.key,
                        None,
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        wgpu::StoreOp::Discard,
                    )),
                ],
                depth_stencil_attachment: Some(depth_attachment(
                    &targets.depth,
                    0.0,
                    wgpu::StoreOp::Discard,
                )),
                ..Default::default()
            });
            pass.set_bind_group(0, &group, &[]);
            if has_background {
                pass.set_pipeline(&self.background_1x);
                pass.draw(0..3, 0..1);
            }
            draw_meshes(&mut pass, true);
            drop(pass);
            self.blur_reflection(&mut encoder, cx, targets);
        }

        // The geometry pass.
        let group = frame_group(
            &cx.frame_buffer,
            reflection.map_or(&self.black_hdr, |r| &r.view),
        );
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("geometry"),
                color_attachments: &[
                    Some(color_attachment(
                        &cx.msaa_color,
                        Some(&cx.hdr),
                        wgpu::LoadOp::Clear(clear),
                        wgpu::StoreOp::Discard,
                    )),
                    Some(color_attachment(
                        &cx.msaa_key,
                        Some(&cx.key),
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        wgpu::StoreOp::Discard,
                    )),
                ],
                depth_stencil_attachment: Some(depth_attachment(
                    &cx.depth,
                    0.0,
                    wgpu::StoreOp::Discard,
                )),
                ..Default::default()
            });
            pass.set_bind_group(0, &group, &[]);
            if has_background {
                pass.set_pipeline(&self.background_pipeline);
                pass.draw(0..3, 0..1);
            }
            draw_meshes(&mut pass, false);
            if additive_count > 0 {
                pass.set_pipeline(&self.particle_additive);
                pass.set_vertex_buffer(0, cx.particle_buffer.slice(..));
                pass.draw(0..6, 0..additive_count);
            }
            if normal_count > 0 {
                pass.set_pipeline(&self.particle_normal);
                let offset = u64::from(additive_count) * size_of::<ParticleInstance>() as u64;
                pass.set_vertex_buffer(0, cx.particle_buffer.slice(offset..));
                pass.draw(0..6, 0..normal_count);
            }
        }

        // The depth of field, then the share of the moment in the average.
        let focus = camera
            .focus
            .filter(|f| camera.ortho_height.is_none() && f.blur > 0.0);
        let mut post = PostUniform {
            encode: [0.0, 0.0, 0.0, weight],
            ..Default::default()
        };
        if let Some(f) = focus {
            let blur = f.blur * height as f32 / 1080.0;
            post.dof = [f.distance, blur, (blur * 1.6).min(40.0), DOF_TILE as f32];
        }
        self.queue
            .write_buffer(&cx.post_buffer, 0, bytemuck::bytes_of(&post));
        let (source, result) = if focus.is_some() {
            self.depth_of_field(&mut encoder, cx);
            (&cx.dof, Accumulated::Dof)
        } else {
            (&cx.hdr, Accumulated::Hdr)
        };
        if single {
            // One moment is the average already.
            cx.result = result;
        } else {
            let black = &self.black_hdr;
            let group = self.post_group(cx, 0, [source, &cx.key, &cx.tiles, black, black]);
            let clear = (moment == 0).then_some(wgpu::Color::TRANSPARENT);
            fullscreen(
                &mut encoder,
                &self.post.accumulate,
                &[&group],
                &[&cx.accum],
                clear,
            );
            cx.result = Accumulated::Accum;
        }
        self.submit(encoder.finish());
    }

    /// Writes the fog, the environment and the background of the scene into the frame
    /// uniform. Returns the view and the level count of the environment panorama, when the
    /// environment has one.
    fn set_environment(
        &self,
        frame: &mut FrameUniform,
        scene: &Scene,
    ) -> Option<(wgpu::TextureView, u32)> {
        match scene.fog {
            Some(Fog::Linear { color, near, far }) => {
                frame.fog = color.0.extend(1.0).to_array();
                frame.fog_params = [near, near + (far - near).max(1e-6), 0.0, 0.0];
            }
            Some(Fog::Exp2 { color, density }) => {
                frame.fog = color.0.extend(2.0).to_array();
                frame.fog_params = [0.0, 0.0, density, 0.0];
            }
            None => {}
        }
        match &scene.background {
            Background::Transparent => {}
            Background::Color(c) => {
                frame.background = [1.0, 0.0, 0.0, 0.0];
                frame.background_color = c.0.extend(1.0).to_array();
            }
            Background::Environment { blur } => frame.background = [2.0, *blur, 0.0, 0.0],
        }
        match &scene.environment {
            Environment::None => None,
            Environment::Studio(studio) => {
                // The softboxes give the diffuse light. The reflections come from the studio
                // baked into a panorama.
                let count = studio.softboxes.len().min(MAX_SOFTBOXES);
                frame.env = [1.0, studio.intensity, studio.rotation, count as f32];
                frame.env_sky = studio.sky.0.extend(0.0).to_array();
                frame.env_horizon = studio.horizon.0.extend(0.0).to_array();
                frame.env_ground = studio.ground.0.extend(0.0).to_array();
                for (uniform, b) in frame.softboxes.iter_mut().zip(&studio.softboxes) {
                    let solid_angle = 4.0 * b.half_size.x.sin() * b.half_size.y.sin();
                    *uniform = GpuSoftBox {
                        direction: b.direction.extend(solid_angle).to_array(),
                        color: b.color.0.extend(0.0).to_array(),
                    };
                }
                Some(self.env_map(&studio.baked()))
            }
            Environment::Map {
                map,
                intensity,
                rotation,
            } => {
                frame.env = [2.0, *intensity, *rotation, 0.0];
                Some(self.env_map(map))
            }
        }
    }

    /// Renders the depth of the shadow casters into the shadow maps.
    fn draw_shadow_maps(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        cx: &FrameContext,
        items: &[DrawItem],
        shadows: &[Option<GpuShadow>],
    ) {
        let matrix = size_of::<[f32; 16]>();
        let draw_group = self.bind_group(
            "shadow draws",
            &self.shadow_draw_layout,
            &[buffer_range(&cx.draw_buffer, 0, matrix)],
        );
        for (i, shadow) in shadows.iter().enumerate() {
            let Some(shadow) = shadow else { continue };
            let offset = i as u64 * UNIFORM_ALIGNMENT;
            self.queue.write_buffer(
                &cx.shadow_pass_buffer,
                offset,
                bytemuck::cast_slice(&shadow.view_proj),
            );
            let pass_group = self.bind_group(
                "shadow pass",
                &self.shadow_layout,
                &[buffer_range(&cx.shadow_pass_buffer, offset, matrix)],
            );
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                depth_stencil_attachment: Some(depth_attachment(
                    &cx.shadow_layers[i],
                    1.0,
                    wgpu::StoreOp::Store,
                )),
                ..Default::default()
            });
            pass.set_pipeline(&self.shadow_pipeline);
            pass.set_bind_group(0, &pass_group, &[]);
            for (slot, item) in items.iter().enumerate() {
                if item.cast_shadow && !item.batched {
                    pass.set_bind_group(1, &draw_group, &[(slot as u64 * DRAW_STRIDE) as u32]);
                    item.draw(&mut pass, cx);
                }
            }
        }
    }

    /// Fills the smaller levels of the reflection. Each level is a box filtered half of the
    /// level before it.
    fn blur_reflection(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        cx: &FrameContext,
        targets: &ReflectionTargets,
    ) {
        let black = &self.black_hdr;
        for level in 1..targets.levels as usize {
            let source_width = (targets.width >> (level - 1)).max(1);
            let source_height = (targets.height >> (level - 1)).max(1);
            let slot = REFLECTION_SLOT + level as u64;
            let uniform = PostUniform {
                texel: [
                    1.0 / source_width as f32,
                    1.0 / source_height as f32,
                    0.0,
                    0.0,
                ],
                ..Default::default()
            };
            self.queue.write_buffer(
                &cx.post_buffer,
                slot * UNIFORM_ALIGNMENT,
                bytemuck::bytes_of(&uniform),
            );
            let source = &targets.level_views[level - 1];
            let group = self.post_group(cx, slot, [source, black, black, black, black]);
            fullscreen(
                encoder,
                &self.post.down_rgba,
                &[&group],
                &[&targets.level_views[level]],
                Some(wgpu::Color::TRANSPARENT),
            );
        }
    }

    /// The depth of field of the HDR frame into `cx.dof`.
    fn depth_of_field(&self, encoder: &mut wgpu::CommandEncoder, cx: &FrameContext) {
        let black = &self.black_hdr;
        // The tile and half passes write the textures that the later passes read. So they
        // bind placeholders for them.
        let group = self.post_group(cx, 0, [&cx.hdr, &cx.key, black, black, black]);
        fullscreen(encoder, &self.post.tile, &[&group], &[&cx.tiles], None);
        fullscreen(
            encoder,
            &self.post.half,
            &[&group],
            &[&cx.half_hdr, &cx.half_key],
            None,
        );
        let group = self.post_group(cx, 0, [&cx.hdr, &cx.key, &cx.tiles, black, black]);
        fullscreen(encoder, &self.post.near, &[&group], &[&cx.near], None);
        let group = self.post_group(
            cx,
            0,
            [&cx.hdr, &cx.key, &cx.near, &cx.half_hdr, &cx.half_key],
        );
        fullscreen(encoder, &self.post.dof, &[&group], &[&cx.dof], None);
    }

    /// Draws the meshes in `order`. The reflection pass skips the reflectors and culls the
    /// other faces, because the mirror turns the triangles around.
    fn draw_items(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        items: &[DrawItem],
        order: &[usize],
        draw_groups: &[wgpu::BindGroup],
        cx: &FrameContext,
        reflection_pass: bool,
    ) {
        for &slot in order {
            let item = &items[slot];
            let m = item.material;
            if reflection_pass && m.reflection > 0.0 {
                continue;
            }
            let blend = if !m.is_blended() {
                Blend::Opaque
            } else if m.blending == Blending::Additive {
                Blend::Additive
            } else {
                Blend::Normal
            };
            pass.set_pipeline(&self.pipeline(PipelineKey {
                cull: cull_face(m.side, item.mirrored != reflection_pass),
                blend,
                depth_write: blend == Blend::Opaque || (blend == Blend::Normal && m.depth_write),
                variant: item.variant,
                reflection_pass,
            }));
            pass.set_bind_group(1, &draw_groups[slot], &[(slot as u64 * DRAW_STRIDE) as u32]);
            item.draw(pass, cx);
        }
    }

    /// Bloom, tone mapping and the encoding of the average of the moments into `target`
    /// (premultiplied sRGB RGBA8).
    fn post_process(
        &self,
        cx: &FrameContext,
        renderer: &Renderer,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
    ) {
        let bloom = renderer.bloom.filter(|b| b.strength > 0.0);
        let black = &self.black_hdr;
        let texel = |width: u32, height: u32| [1.0 / width as f32, 1.0 / height as f32, 0.0, 0.0];
        // The uniforms of the passes, from slot 1 of the post buffer.
        let mut slots: Vec<PostUniform> = Vec::new();
        let mut pass = |uniform: PostUniform,
                        pipeline: &wgpu::RenderPipeline,
                        source: &wgpu::TextureView,
                        aux: &wgpu::TextureView,
                        output: &wgpu::TextureView| {
            slots.push(uniform);
            let group = self.post_group(
                cx,
                slots.len() as u64,
                [source, &cx.key, &cx.tiles, aux, black],
            );
            fullscreen(encoder, pipeline, &[&group], &[output], None);
        };
        if let Some(b) = bloom {
            let threshold = b.threshold.max(0.0);
            pass(
                PostUniform {
                    bloom: [threshold, threshold * 0.5 + 1e-4, 0.0, b.strength],
                    texel: texel(cx.width, cx.height),
                    ..Default::default()
                },
                &self.post.bright,
                cx.result(),
                black,
                &cx.bloom[0].0,
            );
            for i in 1..cx.bloom.len() {
                let (source, width, height) = &cx.bloom[i - 1];
                pass(
                    PostUniform {
                        texel: texel(*width, *height),
                        ..Default::default()
                    },
                    &self.post.down,
                    source,
                    black,
                    &cx.bloom[i].0,
                );
            }
            let factors: [f32; BLOOM_LEVELS] = [1.0, 0.8, 0.6, 0.4, 0.2, 0.1];
            for i in (0..cx.bloom.len() - 1).rev() {
                let (source, width, height) = &cx.bloom[i + 1];
                let f = factors[i + 1];
                pass(
                    PostUniform {
                        bloom: [0.0, 0.0, (f + (1.2 - f - f) * b.radius).max(0.0), 0.0],
                        texel: texel(*width, *height),
                        ..Default::default()
                    },
                    &self.post.up,
                    source,
                    black,
                    &cx.bloom[i].0,
                );
            }
        }
        let tone = match renderer.tone_mapping {
            ToneMapping::Linear => 0.0,
            ToneMapping::Aces => 1.0,
            ToneMapping::Neutral => 2.0,
        };
        pass(
            PostUniform {
                bloom: [0.0, 0.0, 0.0, bloom.map_or(0.0, |b| b.strength)],
                encode: [renderer.exposure, tone, f32::from(bloom.is_some()), 1.0],
                ..Default::default()
            },
            &self.post.encode,
            cx.result(),
            if bloom.is_some() {
                &cx.bloom[0].0
            } else {
                black
            },
            target,
        );

        let stride = UNIFORM_ALIGNMENT as usize;
        let mut data = vec![0u8; slots.len() * stride];
        for (chunk, uniform) in data.chunks_mut(stride).zip(&slots) {
            chunk[..size_of::<PostUniform>()].copy_from_slice(bytemuck::bytes_of(uniform));
        }
        self.queue
            .write_buffer(&cx.post_buffer, UNIFORM_ALIGNMENT, &data);
    }

    /// Post processes the frame into the output texture of the context and reads it back.
    fn finish(&self, cx: &FrameContext, renderer: &Renderer) -> Pixels {
        let (width, height) = (cx.width, cx.height);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.post_process(cx, renderer, &mut encoder, &cx.output_view);
        encoder.copy_texture_to_buffer(
            cx.output.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &cx.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(cx.padded_row),
                    rows_per_image: Some(height),
                },
            },
            cx.output.size(),
        );
        let submission = self.submit(encoder.finish());
        let slice = cx.readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        let _ = self.device.poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        });
        let row = (width * 4) as usize;
        let mut rgba = vec![0u8; row * height as usize];
        if let Ok(Ok(())) = receiver.recv()
            && let Ok(mapped) = slice.get_mapped_range()
        {
            for (dst, src) in rgba
                .chunks_exact_mut(row)
                .zip(mapped.chunks(cx.padded_row as usize))
            {
                dst.copy_from_slice(&src[..row]);
            }
            drop(mapped);
            cx.readback.unmap();
        }
        Pixels {
            width,
            height,
            rgba,
        }
    }
}

/// `view` for positions relative to the camera, which `view` moves to the origin: the
/// matrix without its translation. `relative(view) * (p - camera)` is `view * p`. Without
/// the large translation, a small part far from the origin keeps its precision.
fn relative(view: Mat4) -> Mat4 {
    Mat4::from_cols(view.x_axis, view.y_axis, view.z_axis, Vec4::W)
}

/// A depth test against a `DEPTH` target.
fn depth_test(write: bool, compare: wgpu::CompareFunction) -> wgpu::DepthStencilState {
    wgpu::DepthStencilState {
        format: DEPTH,
        depth_write_enabled: Some(write),
        depth_compare: Some(compare),
        stencil: wgpu::StencilState::default(),
        bias: wgpu::DepthBiasState::default(),
    }
}

/// The color targets of the scene pipelines: the HDR color with `blend`, and the depth key.
/// Only the pipelines without blending write the key.
fn scene_targets(blend: Option<wgpu::BlendState>) -> [Option<wgpu::ColorTargetState>; 2] {
    let key_writes = if blend.is_none() {
        wgpu::ColorWrites::ALL
    } else {
        wgpu::ColorWrites::empty()
    };
    [
        Some(wgpu::ColorTargetState {
            blend,
            ..HDR.into()
        }),
        Some(wgpu::ColorTargetState {
            write_mask: key_writes,
            ..KEY.into()
        }),
    ]
}

/// A 2D texture with one layer, one mipmap level and one sample.
fn texture_desc(
    label: &str,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::TextureDescriptor<'_> {
    wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    }
}

/// `size` bytes of `buffer` from `offset`, for a uniform binding.
fn buffer_range(buffer: &wgpu::Buffer, offset: u64, size: usize) -> wgpu::BindingResource<'_> {
    wgpu::BindingResource::Buffer(wgpu::BufferBinding {
        buffer,
        offset,
        size: wgpu::BufferSize::new(size as u64),
    })
}

fn color_attachment<'a>(
    view: &'a wgpu::TextureView,
    resolve_target: Option<&'a wgpu::TextureView>,
    load: wgpu::LoadOp<wgpu::Color>,
    store: wgpu::StoreOp,
) -> wgpu::RenderPassColorAttachment<'a> {
    wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target,
        ops: wgpu::Operations { load, store },
    }
}

fn depth_attachment(
    view: &wgpu::TextureView,
    clear: f32,
    store: wgpu::StoreOp,
) -> wgpu::RenderPassDepthStencilAttachment<'_> {
    wgpu::RenderPassDepthStencilAttachment {
        view,
        depth_ops: Some(wgpu::Operations {
            load: wgpu::LoadOp::Clear(clear),
            store,
        }),
        stencil_ops: None,
    }
}

/// Draws one triangle that covers `targets`. With `clear` the pass clears the targets first,
/// otherwise it draws over them.
fn fullscreen(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    groups: &[&wgpu::BindGroup],
    targets: &[&wgpu::TextureView],
    clear: Option<wgpu::Color>,
) {
    let load = clear.map_or(wgpu::LoadOp::Load, wgpu::LoadOp::Clear);
    let attachments: Vec<_> = targets
        .iter()
        .map(|view| Some(color_attachment(view, None, load, wgpu::StoreOp::Store)))
        .collect();
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("post"),
        color_attachments: &attachments,
        ..Default::default()
    });
    pass.set_pipeline(pipeline);
    for (index, group) in groups.iter().enumerate() {
        pass.set_bind_group(index as u32, *group, &[]);
    }
    pass.draw(0..3, 0..1);
}

/// The face to cull for a material side. A mirror turns the triangles around.
fn cull_face(side: Side, mirrored: bool) -> Option<wgpu::Face> {
    match (side, mirrored) {
        (Side::Double, _) => None,
        (Side::Front, false) | (Side::Back, true) => Some(wgpu::Face::Back),
        (Side::Back, false) | (Side::Front, true) => Some(wgpu::Face::Front),
    }
}

/// The interleaved vertices of a geometry. `deformed` replaces the positions and normals.
fn vertices_of(g: &Geometry, deformed: Option<(&[Vec3], &[Vec3])>) -> Vec<Vertex> {
    (0..g.positions.len())
        .map(|i| {
            let (p, n) = match deformed {
                Some((positions, normals)) => (positions[i], normals[i]),
                None => (g.positions[i], g.normals.get(i).copied().unwrap_or(Vec3::Y)),
            };
            Vertex {
                position: p.to_array(),
                normal: n.to_array(),
                uv: g.uvs.get(i).copied().unwrap_or_default().to_array(),
                color: g.colors.get(i).copied().unwrap_or(Vec4::ONE).to_array(),
            }
        })
        .collect()
}

fn draw_uniform(mesh: &Mesh, model: Mat4, normal_matrix: Mat4) -> DrawUniform {
    let m = &mesh.material;
    let uv_transforms = [
        &m.map,
        &m.emissive_map,
        &m.metal_rough_map,
        &m.normal_map,
        &m.ao_map,
    ]
    .map(|t| {
        t.as_ref().map_or([1.0, 1.0, 0.0, 0.0], |t| {
            [t.repeat.x, t.repeat.y, t.offset.x, t.offset.y]
        })
    });
    DrawUniform {
        model: model.to_cols_array(),
        normal_matrix: normal_matrix.to_cols_array(),
        color: m.color.0.extend(m.opacity).to_array(),
        emissive: (m.emissive.0 * m.emissive_intensity)
            .extend(m.env_intensity)
            .to_array(),
        pbr: [m.metalness, m.roughness, m.clearcoat, m.clearcoat_roughness],
        flags: [
            f32::from(m.flat_shading),
            f32::from(m.blending == Blending::Additive),
            f32::from(m.fog),
            f32::from(m.is_blended()),
        ],
        maps: [
            f32::from(m.map.is_some()),
            f32::from(m.emissive_map.is_some()),
            f32::from(m.metal_rough_map.is_some()),
            f32::from(mesh.receive_shadow),
        ],
        attrs: [
            f32::from(!mesh.geometry.uvs.is_empty()),
            f32::from(
                !mesh.geometry.colors.is_empty()
                    || mesh
                        .instances
                        .as_ref()
                        .is_some_and(|i| !i.colors.is_empty()),
            ),
            m.reflection,
            0.0,
        ],
        uv_transforms,
        detail: [
            f32::from(m.normal_map.is_some()),
            m.normal_scale,
            f32::from(m.ao_map.is_some()),
            0.0,
        ],
    }
}

/// A light in the layout of `scene.wgsl`. `shadows` holds the fitted shadow maps.
fn gpu_light(light: &LightInfo, shadows: &[Option<GpuShadow>]) -> GpuLight {
    match light.kind {
        LightKind::Directional { direction, shadow } => {
            let index = shadow
                .filter(|&i| shadows.get(i).is_some_and(Option::is_some))
                .map_or(-1.0, |i| i as f32);
            GpuLight {
                color: light.color.extend(0.0).to_array(),
                position: [0.0; 4],
                direction: direction.extend(0.0).to_array(),
                params: [0.0, 0.0, index, 0.0],
            }
        }
        LightKind::Point {
            position,
            distance,
            decay,
        } => GpuLight {
            color: light.color.extend(1.0).to_array(),
            position: position.extend(distance).to_array(),
            direction: [0.0, 0.0, 0.0, decay],
            params: [0.0, 0.0, -1.0, 0.0],
        },
        LightKind::Spot {
            position,
            direction,
            distance,
            decay,
            cos_outer,
            cos_inner,
        } => GpuLight {
            color: light.color.extend(2.0).to_array(),
            position: position.extend(distance).to_array(),
            direction: direction.extend(decay).to_array(),
            params: [cos_outer, cos_inner, -1.0, 0.0],
        },
    }
}

/// Fits a shadow map of `map_size` texels to each shadow casting light (two at most). `None`
/// for a light with nothing to shade.
fn fit_shadows(lights: &Lights, items: &[DrawItem], map_size: u32) -> Vec<Option<GpuShadow>> {
    lights
        .shadow_casters
        .iter()
        .take(MAX_SHADOWS)
        .map(|(direction, settings)| {
            let bounds = settings.bounds.unwrap_or_else(|| {
                items
                    .iter()
                    .filter(|it| it.cast_shadow)
                    .fold(crate::Aabb::EMPTY, |acc, it| acc.union(it.bounds))
            });
            if bounds.is_empty() {
                return None;
            }
            let f = fit(*direction, &bounds, map_size as usize);
            Some(GpuShadow {
                view_proj: f.view_proj.to_cols_array(),
                params: [
                    f.texel_world,
                    f.depth_per_texel,
                    settings.softness.max(0.0),
                    settings.strength.clamp(0.0, 1.0),
                ],
                params2: [settings.bias, map_size as f32, 0.0, 0.0],
            })
        })
        .collect()
}

/// Merges small rigid opaque meshes that look the same into batches. Their vertices move to
/// the space of the batch and into `streamed`. The space of a batch is the world space moved
/// to the position of its first mesh, so that the vertices keep their precision far from the
/// origin. Returns the indices of the batches.
fn batch(items: &mut Vec<DrawItem>, streamed: &mut Vec<Vertex>) -> Vec<u32> {
    let texture_id = |t: &Option<Arc<Texture>>| t.as_ref().map_or(0, |t| Arc::as_ptr(t) as usize);
    let mut groups: HashMap<BatchKey, Vec<usize>> = HashMap::new();
    for (i, item) in items.iter().enumerate() {
        if item.vertices.is_none()
            || item.first_instance != 0
            || item.material.is_blended()
            || item.index_count > BATCH_MAX_INDICES
        {
            continue;
        }
        // The draw uniform without the matrices, and the textures, which it does not hold.
        let mut look =
            bytemuck::bytes_of(&item.uniform)[std::mem::offset_of!(DrawUniform, color)..].to_vec();
        for texture in [
            &item.material.map,
            &item.material.emissive_map,
            &item.material.metal_rough_map,
            &item.material.normal_map,
            &item.material.ao_map,
        ] {
            look.extend_from_slice(&texture_id(texture).to_ne_bytes());
        }
        let key = (
            look,
            item.variant,
            item.material.side as u8,
            item.mirrored,
            item.cast_shadow,
        );
        groups.entry(key).or_default().push(i);
    }
    let mut groups: Vec<Vec<usize>> = groups.into_values().filter(|g| g.len() > 1).collect();
    // The order of a HashMap changes from run to run. The sort keeps the draw order the same.
    groups.sort_by_key(|g| g[0]);

    let mut indices: Vec<u32> = Vec::new();
    for members in groups {
        let vertex_offset = (streamed.len() * size_of::<Vertex>()) as u64;
        let index_start = indices.len();
        let mut bounds = crate::Aabb::EMPTY;
        let mut depth = f32::INFINITY;
        let mut base = 0u32;
        let origin = Mat4::from_cols_array(&items[members[0]].uniform.model)
            .w_axis
            .truncate();
        for &i in &members {
            let item = &mut items[i];
            let g = &item.geometry;
            let model = Mat4::from_cols_array(&item.uniform.model);
            let normal_matrix = Mat4::from_cols_array(&item.uniform.normal_matrix);
            let (linear, offset) = (Mat3::from_mat4(model), model.w_axis.truncate() - origin);
            let positions: Vec<Vec3> = g.positions.iter().map(|p| linear * *p + offset).collect();
            let normals: Vec<Vec3> = (0..g.positions.len())
                .map(|k| {
                    normal_matrix.transform_vector3(g.normals.get(k).copied().unwrap_or(Vec3::Y))
                })
                .collect();
            streamed.extend(vertices_of(g, Some((&positions, &normals))));
            indices.extend(
                g.indices
                    .iter()
                    .copied()
                    .filter(|&k| (k as usize) < g.positions.len())
                    .map(|k| k + base),
            );
            base += g.positions.len() as u32;
            bounds = bounds.union(item.bounds);
            depth = depth.min(item.depth);
            item.batched = true;
        }
        let first = &items[members[0]];
        let mut uniform = first.uniform;
        uniform.model = Mat4::from_translation(origin).to_cols_array();
        uniform.normal_matrix = Mat4::IDENTITY.to_cols_array();
        let batch = DrawItem {
            material: first.material,
            geometry: Arc::clone(&first.geometry),
            uniform,
            vertices: None,
            vertex_offset,
            indices: None,
            index_offset: (index_start * size_of::<u32>()) as u64,
            index_count: (indices.len() - index_start) as u32,
            first_instance: 0,
            instance_count: 1,
            batched: false,
            cast_shadow: first.cast_shadow,
            mirrored: first.mirrored,
            depth,
            bounds,
            variant: first.variant,
        };
        items.push(batch);
    }
    indices
}

/// The order in which the items are drawn. The opaque items come first, grouped by pipeline
/// and front to back within a group, because every pipeline switch stalls the GPU. The
/// blended items follow from back to front.
fn draw_order(items: &[DrawItem]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..items.len()).filter(|&i| !items[i].batched).collect();
    order.sort_by(|&a, &b| {
        let (ia, ib) = (&items[a], &items[b]);
        match (ia.material.is_blended(), ib.material.is_blended()) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            (false, false) => ia
                .pipeline_rank()
                .cmp(&ib.pipeline_rank())
                .then(ia.depth.total_cmp(&ib.depth)),
            (true, true) => ib.depth.total_cmp(&ia.depth),
        }
    });
    order
}

impl FrameContext {
    fn new(device: &wgpu::Device, width: u32, height: u32, shadow_size: u32) -> FrameContext {
        let view = |descriptor: &wgpu::TextureDescriptor| {
            device
                .create_texture(descriptor)
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let multisampled = |label: &str, format: wgpu::TextureFormat| {
            view(&wgpu::TextureDescriptor {
                sample_count: MSAA,
                ..texture_desc(
                    label,
                    width.max(1),
                    height.max(1),
                    format,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                )
            })
        };
        let sampled = |label: &str, format: wgpu::TextureFormat, w: u32, h: u32| {
            view(&texture_desc(
                label,
                w.max(1),
                h.max(1),
                format,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            ))
        };
        let buffer = |label: &str, size: u64, usage: wgpu::BufferUsages| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };

        let mut bloom = Vec::new();
        let (mut w, mut h) = ((width / 2).max(1), (height / 2).max(1));
        bloom.push((sampled("bloom", HDR, w, h), w, h));
        while bloom.len() < BLOOM_LEVELS && w > 8 && h > 8 {
            w = (w / 2).max(1);
            h = (h / 2).max(1);
            bloom.push((sampled("bloom", HDR, w, h), w, h));
        }
        let output = device.create_texture(&texture_desc(
            "output",
            width,
            height,
            OUTPUT,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        ));
        let padded_row = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let (tiles_w, tiles_h) = (width.div_ceil(DOF_TILE), height.div_ceil(DOF_TILE));
        let (half_w, half_h) = (width.div_ceil(2), height.div_ceil(2));
        let (shadow_array, shadow_layers) = shadow_targets(device, shadow_size);
        let uniform = wgpu::BufferUsages::UNIFORM;
        FrameContext {
            width,
            height,
            msaa_color: multisampled("msaa color", HDR),
            msaa_key: multisampled("msaa key", KEY),
            depth: multisampled("depth", DEPTH),
            hdr: sampled("hdr", HDR, width, height),
            key: sampled("key", KEY, width, height),
            tiles: sampled("dof tiles", KEY, tiles_w, tiles_h),
            near: sampled("dof near", KEY, tiles_w, tiles_h),
            half_hdr: sampled("half", HDR, half_w, half_h),
            half_key: sampled("half key", KEY, half_w, half_h),
            dof: sampled("dof", HDR, width, height),
            accum: sampled("accumulation", HDR, width, height),
            bloom,
            output_view: output.create_view(&wgpu::TextureViewDescriptor::default()),
            output,
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("readback"),
                size: u64::from(padded_row) * u64::from(height),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            padded_row,
            shadow_size,
            shadow_array,
            shadow_layers,
            result: Accumulated::Accum,
            reflection: None,
            frame_buffer: buffer("frame", size_of::<FrameUniform>() as u64, uniform),
            shadow_pass_buffer: buffer(
                "shadow pass",
                UNIFORM_ALIGNMENT * MAX_SHADOWS as u64,
                uniform,
            ),
            post_buffer: buffer("post", UNIFORM_ALIGNMENT * POST_SLOTS, uniform),
            draw_buffer: buffer("draws", DRAW_STRIDE * 256, uniform),
            stream_buffer: buffer("streamed vertices", 1 << 20, wgpu::BufferUsages::VERTEX),
            instance_buffer: buffer("instances", 1 << 16, wgpu::BufferUsages::VERTEX),
            stream_index_buffer: buffer("streamed indices", 1 << 18, wgpu::BufferUsages::INDEX),
            particle_buffer: buffer("particles", 1 << 16, wgpu::BufferUsages::VERTEX),
        }
    }

    /// Makes the reflection targets at this size, unless they exist already.
    fn prepare_reflection(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self
            .reflection
            .as_ref()
            .is_none_or(|r| r.width != width || r.height != height)
        {
            self.reflection = Some(ReflectionTargets::new(device, width, height));
        }
    }

    /// The finished HDR frame, see [`Accumulated`].
    fn result(&self) -> &wgpu::TextureView {
        match self.result {
            Accumulated::Accum => &self.accum,
            Accumulated::Hdr => &self.hdr,
            Accumulated::Dof => &self.dof,
        }
    }

    fn resize_shadows(&mut self, device: &wgpu::Device, size: u32) {
        (self.shadow_array, self.shadow_layers) = shadow_targets(device, size);
        self.shadow_size = size;
    }
}

/// The targets of the reflection: an HDR image with blurred levels, its depth and key
/// targets, and the frame uniform of the mirrored view.
struct ReflectionTargets {
    width: u32,
    height: u32,
    levels: u32,
    /// All levels, for sampling.
    view: wgpu::TextureView,
    /// One view per level, for rendering.
    level_views: Vec<wgpu::TextureView>,
    key: wgpu::TextureView,
    depth: wgpu::TextureView,
    frame_buffer: wgpu::Buffer,
}

impl ReflectionTargets {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let levels = crate::render::reflection::level_count(width as usize, height as usize) as u32;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            mip_level_count: levels,
            ..texture_desc(
                "reflection",
                width,
                height,
                HDR,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            )
        });
        let level_views = (0..levels)
            .map(|level| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    base_mip_level: level,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let attachment = |label: &str, format: wgpu::TextureFormat| {
            device
                .create_texture(&texture_desc(
                    label,
                    width,
                    height,
                    format,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                ))
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        Self {
            width,
            height,
            levels,
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            level_views,
            key: attachment("reflection key", KEY),
            depth: attachment("reflection depth", DEPTH),
            frame_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("reflection frame"),
                size: size_of::<FrameUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        }
    }
}

/// The shadow maps as one array view for sampling and one view per layer for rendering.
fn shadow_targets(device: &wgpu::Device, size: u32) -> (wgpu::TextureView, Vec<wgpu::TextureView>) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: MAX_SHADOWS as u32,
        },
        ..texture_desc(
            "shadow maps",
            size,
            size,
            DEPTH,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        )
    });
    let array = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let layers = (0..MAX_SHADOWS as u32)
        .map(|layer| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    (array, layers)
}
