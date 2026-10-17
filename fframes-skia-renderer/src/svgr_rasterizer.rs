//! The trees of `FFramesContext::render_svgr` rendered with Skia, see [`SkiaSvgrRasterizer`].

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

use fframes::media::ImageData;
use fframes::usvgr;
use skia_safe::gpu::{self, DirectContext};
use skia_safe::{AlphaType, Canvas, ColorType, ImageInfo, surfaces};

use crate::render::RenderCache;
use crate::{QueueLock, SkiaBackend, SkiaCacheConfig, lock_queue};

/// Renders the svgr trees of `FFramesContext::render_svgr` with Skia, on the GPU or the
/// CPU of a backend. The threads of a render share it.
///
/// On a backend that shares its GPU with wgpu (the Vulkan backend with the `wgpu` feature)
/// the image is a texture on that GPU (`fframes::wgpu_layer`). Skia draws the tree into
/// it, and 3D materials and frames sample it in place. On every other backend the image
/// holds the pixels.
///
/// Each call takes a Skia context from a pool of at most `contexts` contexts. A call waits
/// while all of them are in use. A context is never held while user code runs, so a
/// waiting call always gets one.
pub struct SkiaSvgrRasterizer<'a, B: SkiaBackend> {
    backend: &'a B,
    cache_config: SkiaCacheConfig,
    pool: Mutex<Pool>,
    /// Signals that a context is free again.
    available: Condvar,
    #[cfg(feature = "wgpu")]
    targets: Mutex<Vec<Target>>,
}

#[derive(Default)]
struct Pool {
    free: Vec<Context>,
    /// The contexts that exist, free or in use.
    created: usize,
    max: usize,
}

/// A Skia context of the pool with its caches.
struct Context {
    gpu: Option<DirectContext>,
    queue_lock: Option<QueueLock>,
    cache: RenderCache,
}

// SAFETY: the pool gives a context to one thread at a time. A Skia context and the Skia
// objects of its cache are not bound to the thread that made them.
unsafe impl Send for Context {}

impl Drop for Context {
    fn drop(&mut self) {
        // what lives on the context goes first, and destroying a context waits for its queue
        self.cache = RenderCache::new();
        let _queue = lock_queue(self.queue_lock.as_ref());
        self.gpu = None;
    }
}

/// A texture that Skia draws an svgr tree into. It is free again when its image is
/// dropped. At that time the frames that sample it are submitted.
#[cfg(feature = "wgpu")]
struct Target {
    texture: fframes::wgpu::Texture,
    image: std::sync::Weak<usvgr::PreloadedImageData>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl<'a, B: SkiaBackend> SkiaSvgrRasterizer<'a, B> {
    /// `contexts` is the maximum number of Skia contexts that render trees at the same
    /// time (at least one).
    pub fn new(backend: &'a B, cache_config: SkiaCacheConfig, contexts: usize) -> Self {
        Self {
            backend,
            cache_config,
            pool: Mutex::new(Pool {
                max: contexts.max(1),
                ..Pool::default()
            }),
            available: Condvar::new(),
            #[cfg(feature = "wgpu")]
            targets: Mutex::new(Vec::new()),
        }
    }

    /// A free context, or a new one. Waits while all contexts are in use.
    fn take(&self) -> Option<Context> {
        let mut pool = lock(&self.pool);
        loop {
            if let Some(context) = pool.free.pop() {
                return Some(context);
            }
            if pool.created < pool.max {
                pool.created += 1;
                break;
            }
            pool = self
                .available
                .wait(pool)
                .unwrap_or_else(PoisonError::into_inner);
        }
        drop(pool);
        match self.backend.create_offscreen_context() {
            Ok((gpu, queue_lock)) => Some(Context {
                gpu,
                queue_lock,
                cache: RenderCache::with_config(self.cache_config).for_backend(self.backend),
            }),
            Err(e) => {
                eprintln!("fframes: render_svgr can not create a Skia context: {e}");
                lock(&self.pool).created -= 1;
                self.available.notify_one();
                None
            }
        }
    }

    fn give_back(&self, context: Context) {
        lock(&self.pool).free.push(context);
        self.available.notify_one();
    }

    /// Renders into a new surface and reads the pixels back.
    fn pixels(
        &self,
        context: &mut Context,
        tree: &usvgr::Tree,
        width: u32,
        height: u32,
    ) -> Option<ImageData<'static>> {
        let info = ImageInfo::new(
            (width as i32, height as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );
        let mut surface = match context.gpu.as_mut() {
            Some(gpu) => gpu::surfaces::render_target(
                gpu,
                gpu::Budgeted::Yes,
                &info,
                None,
                gpu::SurfaceOrigin::TopLeft,
                None,
                false,
                None,
            ),
            None => surfaces::raster(&info, None, None),
        }?;
        draw(surface.canvas(), tree, width, height, &mut context.cache);
        let mut pixels = vec![0; width as usize * height as usize * 4];
        // the readback submits the drawing to the queue and waits for it
        let read = {
            let _queue = lock_queue(context.queue_lock.as_ref());
            surface.read_pixels(&info, &mut pixels, width as usize * 4, (0, 0))
        };
        read.then(|| fframes::rendered_image(width, height, pixels))
    }
}

/// Clears the canvas and draws `tree` scaled to `width` x `height`.
fn draw(canvas: &Canvas, tree: &usvgr::Tree, width: u32, height: u32, cache: &mut RenderCache) {
    canvas.clear(skia_safe::Color::TRANSPARENT);
    canvas.save();
    crate::apply_fit(canvas, tree, width as i32, height as i32);
    crate::render::render_tree(tree, canvas, cache);
    canvas.restore();
}

impl<B: SkiaBackend> fframes::SvgrRasterizer for SkiaSvgrRasterizer<'_, B> {
    fn rasterize(&self, tree: &usvgr::Tree, width: u32, height: u32) -> Option<ImageData<'static>> {
        let mut context = self.take()?;
        #[cfg(feature = "wgpu")]
        let image = match self.shared_gpu() {
            Some(shared) if context.gpu.is_some() => {
                self.texture(&shared, &mut context, tree, width, height)
            }
            _ => self.pixels(&mut context, tree, width, height),
        };
        #[cfg(not(feature = "wgpu"))]
        let image = self.pixels(&mut context, tree, width, height);
        self.give_back(context);
        image
    }
}

#[cfg(feature = "wgpu")]
impl<B: SkiaBackend> SkiaSvgrRasterizer<'_, B> {
    /// The GPU that the contexts of the backend draw on, when it is shared with wgpu.
    fn shared_gpu(&self) -> Option<fframes::SharedGpu> {
        let id = self.backend.shared_gpu_id()?;
        fframes::shared_gpu().filter(|gpu| gpu.id() == id)
    }

    /// A free texture of this size with a new image of it, or a new texture.
    fn target(
        &self,
        shared: &fframes::SharedGpu,
        width: u32,
        height: u32,
    ) -> (fframes::wgpu::Texture, ImageData<'static>) {
        use fframes::wgpu;

        let mut targets = lock(&self.targets);
        let free = |t: &Target| t.image.strong_count() == 0;
        let found = targets
            .iter()
            .position(|t| free(t) && t.texture.width() == width && t.texture.height() == height);
        let slot = found.unwrap_or_else(|| {
            // free textures of other sizes are not used again soon
            targets.retain(|t| !free(t));
            targets.push(Target {
                texture: shared.device().create_texture(&wgpu::TextureDescriptor {
                    label: Some("fframes svgr image"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: fframes::WGPU_LAYER_FORMAT,
                    usage: fframes::WGPU_LAYER_USAGE | wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                }),
                image: std::sync::Weak::new(),
            });
            targets.len() - 1
        });
        let texture = targets[slot].texture.clone();
        let image = fframes::wgpu_layer(shared, &texture);
        targets[slot].image = std::sync::Arc::downgrade(&image.href());
        (texture, image)
    }

    /// Draws `tree` into a texture on the shared GPU. The pixels stay on the GPU.
    ///
    /// wgpu tracks the state of the texture, so wgpu makes every transition: a clear pass
    /// leaves the texture as a color target, Skia draws and leaves it in that layout, and
    /// [`fframes::finish_wgpu_layer`] makes it sampled. Each step is a submission to the
    /// one queue of the shared GPU, so the queue orders them.
    fn texture(
        &self,
        shared: &fframes::SharedGpu,
        context: &mut Context,
        tree: &usvgr::Tree,
        width: u32,
        height: u32,
    ) -> Option<ImageData<'static>> {
        use fframes::wgpu;
        use skia_safe::gpu::vk::ImageLayout;

        let (texture, image) = self.target(shared, width, height);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = shared
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fframes svgr image clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        shared.submit([encoder.finish()]);

        let gpu_context = context.gpu.as_mut()?;
        let backend_texture = crate::render::wgpu_layer::backend_texture(
            &texture,
            ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        )?;
        let mut surface = gpu::surfaces::wrap_backend_texture(
            gpu_context,
            &backend_texture,
            gpu::SurfaceOrigin::TopLeft,
            None,
            ColorType::RGBA8888,
            None,
            None,
        )?;
        draw(surface.canvas(), tree, width, height, &mut context.cache);
        let state = gpu::vk::mutable_texture_states::new_vulkan(
            ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            ash::vk::QUEUE_FAMILY_IGNORED,
        );
        {
            let _queue = lock_queue(context.queue_lock.as_ref());
            gpu_context.flush_surface_with_texture_state(
                &mut surface,
                &gpu::FlushInfo::default(),
                Some(&state),
            );
            gpu_context.submit(gpu::SyncCpu::No);
        }
        drop(surface);

        let mut encoder = shared
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        fframes::finish_wgpu_layer(&mut encoder, &texture);
        shared.submit([encoder.finish()]);
        Some(image)
    }
}
