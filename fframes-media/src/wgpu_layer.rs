//! Textures rendered with wgpu as images of a frame (feature `wgpu`).
//!
//! A rendering backend that can share its GPU opens its device as a wgpu device and
//! registers it with [`register_shared_gpu`]. Code that renders with wgpu gets the device
//! from [`shared_gpu`], renders into a texture, and puts the texture into the frame with
//! [`wgpu_layer`]. The backend draws the texture where the frame shows it. The pixels stay
//! on the GPU.
//!
//! ```rust,ignore
//! use fframes::wgpu;
//!
//! let Some(gpu) = fframes::shared_gpu() else {
//!     return render_on_the_cpu(); // the CPU backend, the editor
//! };
//! let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
//!     label: Some("my layer"),
//!     size: wgpu::Extent3d { width: 1920, height: 1080, depth_or_array_layers: 1 },
//!     mip_level_count: 1,
//!     sample_count: 1,
//!     dimension: wgpu::TextureDimension::D2,
//!     format: fframes::WGPU_LAYER_FORMAT,
//!     usage: fframes::WGPU_LAYER_USAGE | wgpu::TextureUsages::RENDER_ATTACHMENT,
//!     view_formats: &[],
//! });
//! let mut encoder = gpu.device().create_command_encoder(&Default::default());
//! // render passes into `texture`
//! fframes::finish_wgpu_layer(&mut encoder, &texture);
//! gpu.submit([encoder.finish()]);
//! let layer = fframes::wgpu_layer(&gpu, &texture);
//! svgr!(<image href={layer.href()} x="0" y="0" width="1920" height="1080" />)
//! ```
//!
//! Render into the texture again only after the image from [`wgpu_layer`] is dropped. At
//! that point the backend has submitted the frame that samples the texture, and the queue
//! orders the next render after it.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

use crate::ImageData;
use crate::markers::Markers;

/// The texture format of a layer: premultiplied, sRGB encoded RGBA, like every other image
/// of a frame.
pub const WGPU_LAYER_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The usages a layer texture needs in addition to the ones it is rendered with. The
/// backend samples it. Skia requires copies in both directions on a wrapped texture, and a
/// copy out is the fallback for backends without the shared GPU.
pub const WGPU_LAYER_USAGE: wgpu::TextureUsages = wgpu::TextureUsages::TEXTURE_BINDING
    .union(wgpu::TextureUsages::COPY_SRC)
    .union(wgpu::TextureUsages::COPY_DST);

/// The GPU of a rendering backend, opened as a wgpu device. Its queue is the queue the
/// backend draws frames with. The queue orders a layer before the frame that samples it.
#[derive(Clone)]
pub struct SharedGpu {
    inner: Arc<SharedGpuInner>,
}

struct SharedGpuInner {
    id: u64,
    device: wgpu::Device,
    queue: wgpu::Queue,
    queue_lock: Arc<Mutex<()>>,
    state: Mutex<HashMap<TypeId, Arc<dyn Any + Send + Sync>>>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
/// The ids of the pixels read back from layers.
static NEXT_PIXELS: AtomicU64 = AtomicU64::new(0);
static ACTIVE: Mutex<Weak<SharedGpuInner>> = Mutex::new(Weak::new());
static LAYERS: Markers<WgpuLayer> = Markers::new("fframes-wgpu:");

impl SharedGpu {
    /// `queue` is the queue the backend submits frames to. The backend holds `queue_lock`
    /// while it submits to that queue.
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, queue_lock: Arc<Mutex<()>>) -> Self {
        Self {
            inner: Arc::new(SharedGpuInner {
                id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
                device,
                queue,
                queue_lock,
                state: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Unique for each device, for caches keyed by device.
    pub fn id(&self) -> u64 {
        self.inner.id
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.inner.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.inner.queue
    }

    /// Submits with the backend's queue lock held. Submit all layer work here. A plain
    /// `queue.submit` races the backend's own submissions.
    pub fn submit<I: IntoIterator<Item = wgpu::CommandBuffer>>(
        &self,
        command_buffers: I,
    ) -> wgpu::SubmissionIndex {
        let _queue = self
            .inner
            .queue_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        self.inner.queue.submit(command_buffers)
    }

    /// The backend's queue lock, for code that keeps the device but not the `SharedGpu`.
    pub fn queue_lock(&self) -> Arc<Mutex<()>> {
        Arc::clone(&self.inner.queue_lock)
    }

    /// State of code that renders on this device: pipelines, caches, texture pools. `init`
    /// makes it on first use. It is dropped with the device, so it must not hold a
    /// `SharedGpu`; keep the `wgpu::Device` and `wgpu::Queue` instead.
    pub fn state<T: Any + Send + Sync>(&self, init: impl FnOnce(&SharedGpu) -> T) -> Arc<T> {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let entry = state
            .entry(TypeId::of::<T>())
            .or_insert_with(|| Arc::new(init(self)));
        Arc::clone(entry)
            .downcast::<T>()
            .unwrap_or_else(|_| unreachable!("state is keyed by its type"))
    }
}

impl std::fmt::Debug for SharedGpu {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedGpu")
            .field("id", &self.inner.id)
            .finish_non_exhaustive()
    }
}

/// Makes `gpu` the device that [`shared_gpu`] returns, as long as the backend keeps it.
/// Called by backends.
pub fn register_shared_gpu(gpu: &SharedGpu) {
    *ACTIVE.lock().unwrap_or_else(PoisonError::into_inner) = Arc::downgrade(&gpu.inner);
}

/// The GPU of the backend that renders the video, when the backend shares it (the Skia
/// Vulkan backend with its `wgpu` feature). `None` with every other backend: the CPU
/// backend, Skia without the feature, the editor. Render into pixels then.
///
/// It is the GPU of the Skia Vulkan context that was made last and is still alive. The
/// tiny-skia CPU backend draws no layers, so do not keep such a context alive while it
/// renders.
pub fn shared_gpu() -> Option<SharedGpu> {
    let inner = ACTIVE
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .upgrade()?;
    Some(SharedGpu { inner })
}

/// Records the transition of `texture` to the sampled state. A layer must be in this state
/// at the end of the last submission that renders it.
pub fn finish_wgpu_layer(encoder: &mut wgpu::CommandEncoder, texture: &wgpu::Texture) {
    encoder.transition_resources(
        std::iter::empty(),
        std::iter::once(wgpu::TextureTransition {
            texture,
            selector: None,
            state: wgpu::TextureUses::RESOURCE,
        }),
    );
}

/// A texture rendered on the [`SharedGpu`] as an image of the frame. Use it as
/// `<image href={layer.href()} .../>` like any other image.
///
/// The texture must have [`WGPU_LAYER_FORMAT`] and [`WGPU_LAYER_USAGE`]. Finish it with
/// [`finish_wgpu_layer`] and submit it with [`SharedGpu::submit`] before the frame is
/// returned. Render into it again only after the returned image is dropped.
pub fn wgpu_layer(gpu: &SharedGpu, texture: &wgpu::Texture) -> ImageData<'static> {
    let (width, height) = (texture.width(), texture.height());
    if texture.format() != WGPU_LAYER_FORMAT || !texture.usage().contains(WGPU_LAYER_USAGE) {
        log::warn!(
            "fframes: a wgpu layer needs a {WGPU_LAYER_FORMAT:?} texture with {WGPU_LAYER_USAGE:?}, got {:?} with {:?}",
            texture.format(),
            texture.usage()
        );
        return LAYERS.empty(width, height);
    }
    LAYERS.create(
        WgpuLayer {
            gpu: gpu.clone(),
            texture: texture.clone(),
            pixels: OnceLock::new(),
        },
        width,
        height,
    )
}

/// The layer behind `image`, if [`wgpu_layer`] made it. Rendering backends call this for
/// every raster image.
pub fn resolve_wgpu_layer(image: &Arc<usvgr::PreloadedImageData>) -> Option<Arc<WgpuLayer>> {
    LAYERS.resolve(image)
}

/// Drops every layer rendered on `gpu`, so that no texture outlives the device, and stops
/// [`shared_gpu`] from returning it. The backend calls this when its contexts are done.
pub fn release_wgpu_layers(gpu: &SharedGpu) {
    LAYERS.retain(|layer| layer.gpu.id() != gpu.id());
    let mut active = ACTIVE.lock().unwrap_or_else(PoisonError::into_inner);
    if std::ptr::eq(active.as_ptr(), Arc::as_ptr(&gpu.inner)) {
        *active = Weak::new();
    }
}

/// A texture of a frame, see [`wgpu_layer`].
pub struct WgpuLayer {
    gpu: SharedGpu,
    texture: wgpu::Texture,
    pixels: OnceLock<Option<Arc<usvgr::PreloadedImageData>>>,
}

impl WgpuLayer {
    /// The [`SharedGpu::id`] of the device the texture lives on. A backend samples the
    /// texture only on that device. On other devices it uses [`Self::pixels`].
    pub fn gpu_id(&self) -> u64 {
        self.gpu.id()
    }

    pub fn texture(&self) -> &wgpu::Texture {
        &self.texture
    }

    /// The texture read back as premultiplied RGBA pixels, for a backend that cannot sample
    /// it: another device, or a picture recording. This waits for the GPU.
    pub fn pixels(&self) -> Option<Arc<usvgr::PreloadedImageData>> {
        self.pixels.get_or_init(|| self.read_back()).clone()
    }

    fn read_back(&self) -> Option<Arc<usvgr::PreloadedImageData>> {
        let (width, height) = (self.texture.width(), self.texture.height());
        let row = width * 4;
        let padded = row.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let device = self.gpu.device();
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fframes wgpu layer readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            self.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            self.texture.size(),
        );
        finish_wgpu_layer(&mut encoder, &self.texture);
        let submission = self.gpu.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: None,
            })
            .ok()?;
        receiver.recv().ok()?.ok()?;
        let mapped = slice.get_mapped_range().ok()?;
        let rgba: Vec<u8> = mapped
            .chunks(padded as usize)
            .flat_map(|line| &line[..row as usize])
            .copied()
            .collect();
        drop(mapped);
        buffer.unmap();
        Some(Arc::new(usvgr::PreloadedImageData {
            data: std::borrow::Cow::Owned(rgba),
            width,
            height,
            id: format!(
                "fframes-wgpu-pixels:{}",
                NEXT_PIXELS.fetch_add(1, Ordering::Relaxed)
            ),
        }))
    }
}

impl std::fmt::Debug for WgpuLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WgpuLayer")
            .field("gpu", &self.gpu.id())
            .field("width", &self.texture.width())
            .field("height", &self.texture.height())
            .finish_non_exhaustive()
    }
}
