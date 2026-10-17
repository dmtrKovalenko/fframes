//! The CPU renderer: a multithreaded software rasterizer. It runs wherever fframes runs
//! (the CPU backend, the Skia backend, the wasm editor) and gives the same pixels on all
//! of them.

mod hybrid;
mod post;
pub(crate) mod prepare;
mod raster;
pub(crate) mod reflection;
mod shade;
pub(crate) mod shadow;

use std::sync::atomic::{AtomicU64, Ordering};

use glam::{Vec2, Vec4};
use rayon::prelude::*;

pub use hybrid::Hybrid;
pub use post::{Bloom, ToneMapping};

use crate::{Camera, Scene};
use prepare::{Viewport, World};
use raster::Blended;
use reflection::ReflectionMap;
use shade::ShadeContext;
use shadow::ShadowMap;

/// Renders [`Scene`]s into images for `svgr!`.
///
/// Make it once, for example in the constructor of the video. It holds only settings.
/// Then call [`Renderer::render`] in `render_frame`. With the `wgpu` feature of fframes it
/// renders on the GPU, see [`Renderer::gpu`].
///
/// ```rust,ignore
/// let renderer = Renderer::new(1920, 1080).bloom(Bloom::default());
///
/// // in render_frame
/// let image = renderer.render(&scene, &camera);
/// fframes::svgr!(<image href={image.href()} x="0" y="0" width="1920" height="1080" />)
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Renderer {
    /// The width of the image in pixels.
    pub width: u32,
    /// The height of the image in pixels.
    pub height: u32,
    /// Whether to use 4x multisampling. Without it, edges are jagged, but frames render
    /// about two times faster.
    pub antialias: bool,
    /// The multiplier of the light before tone mapping.
    pub exposure: f32,
    /// How HDR colors map to the display.
    pub tone_mapping: ToneMapping,
    /// The glow around bright pixels, `None` for no glow.
    pub bloom: Option<Bloom>,
    /// Render on the GPU when the `wgpu` feature of fframes is on, see [`Renderer::gpu`].
    pub gpu: bool,
    /// Render some frames on the CPU while the GPU is busy, see [`Renderer::hybrid`].
    pub hybrid: Option<Hybrid>,
}

/// Premultiplied sRGB RGBA8 pixels from [`Renderer::render_pixels`].
///
/// Use [`Pixels::get`] to read a pixel and [`Pixels::to_straight_rgba`] to save a PNG.
#[derive(Debug, Clone)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    /// Premultiplied alpha, row major.
    pub rgba: Vec<u8>,
}

impl Pixels {
    /// The pixel at `x`, `y`, with straight (not premultiplied) alpha.
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        straight(self.rgba.as_chunks::<4>().0[(y * self.width + x) as usize])
    }

    /// All pixels as straight alpha RGBA8, for example to save a PNG.
    pub fn to_straight_rgba(&self) -> Vec<u8> {
        self.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| straight(*p))
            .collect()
    }

    /// The image for an `<image href>` in `svgr!`.
    pub fn into_image(self) -> fframes_media::ImageData<'static> {
        use fframes_media::{ImageData, ImageMetadata};

        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        // Each image gets a new id, so a cache with the id as key never gives an old frame.
        let id = format!("fframes-3d:{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
        let (width, height) = (self.width, self.height);
        let metadata = ImageMetadata { width, height };

        #[cfg(not(target_arch = "wasm32"))]
        return ImageData::new_from_raw_data(
            std::sync::Arc::new(usvgr::PreloadedImageData {
                data: std::borrow::Cow::Owned(self.rgba),
                width,
                height,
                id: id.clone(),
            }),
            id,
            metadata,
        );

        // The editor shows images from URLs, so the pixels go into a PNG data URL.
        #[cfg(target_arch = "wasm32")]
        {
            use base64::Engine;

            let mut png_bytes = Vec::new();
            let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_compression(png::Compression::Fast);
            if let Ok(mut writer) = encoder.write_header() {
                let _ = writer.write_image_data(&self.to_straight_rgba());
            }
            let url = format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&png_bytes)
            );
            ImageData::new_from_web_source(
                fframes_media::OwnedSharedString::new_owned(url.as_str()),
                url,
                id,
                metadata,
            )
        }
    }
}

/// A premultiplied pixel with straight alpha.
fn straight([r, g, b, a]: [u8; 4]) -> [u8; 4] {
    if a == 0 {
        return [0, 0, 0, 0];
    }
    let un = |c: u8| ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
    [un(r), un(g), un(b), a]
}

impl Renderer {
    /// A renderer for `width` x `height` images with antialiasing, ACES tone mapping and no
    /// bloom. It renders on the GPU with the `wgpu` feature of fframes. In the wasm editor the
    /// size is halved, so the previews stay interactive.
    pub fn new(width: u32, height: u32) -> Self {
        #[cfg(target_arch = "wasm32")]
        let (width, height) = ((width / 2).max(1), (height / 2).max(1));
        Self {
            width: width.max(1),
            height: height.max(1),
            antialias: true,
            exposure: 1.0,
            tone_mapping: ToneMapping::Aces,
            bloom: None,
            gpu: cfg!(feature = "gpu"),
            hybrid: None,
        }
    }

    /// Renders on the GPU (Vulkan, Metal or DX12 through wgpu). This is the default with
    /// the `wgpu` feature of fframes. Without the feature, or without a GPU that works, the
    /// CPU renders. The shading is the same, and the pixels differ only by rounding.
    pub fn gpu(mut self) -> Self {
        self.gpu = true;
        self
    }

    /// Renders on the CPU, also with the `wgpu` feature.
    pub fn cpu(mut self) -> Self {
        self.gpu = false;
        self
    }

    /// Lets the CPU rasterizer render frames of this renderer while other threads keep the
    /// GPU busy, see [`Hybrid`]. Off by default.
    ///
    /// Turn it on for the shots whose CPU frames look like their GPU frames (compare them
    /// with [`Renderer::cpu`]), and only for renders: which frames render on the CPU depends
    /// on the timing.
    pub fn hybrid(mut self, hybrid: Hybrid) -> Self {
        self.hybrid = Some(hybrid);
        self
    }

    /// Whether frames render on the GPU: [`Renderer::gpu`] is set and a GPU was found.
    pub fn uses_gpu(&self) -> bool {
        #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
        return self.gpu && crate::gpu::gpu().is_some();
        #[cfg(not(all(feature = "gpu", not(target_arch = "wasm32"))))]
        return false;
    }

    /// Adds a glow around bright pixels, see [`Bloom`].
    pub fn bloom(mut self, bloom: Bloom) -> Self {
        self.bloom = Some(bloom);
        self
    }

    /// Multiplies the light before tone mapping. Values above 1 make the image brighter.
    pub fn exposure(mut self, exposure: f32) -> Self {
        self.exposure = exposure;
        self
    }

    /// Sets how HDR colors map to the display, see [`ToneMapping`].
    pub fn tone_mapping(mut self, tone_mapping: ToneMapping) -> Self {
        self.tone_mapping = tone_mapping;
        self
    }

    /// Turns 4x multisampling on or off. Without it, edges are jagged, but frames render
    /// about two times faster.
    pub fn antialias(mut self, antialias: bool) -> Self {
        self.antialias = antialias;
        self
    }

    /// Renders at `scale` times the size, for example for drafts. Put the image in the SVG
    /// at the full size, and it scales up.
    pub fn scaled(mut self, scale: f32) -> Self {
        self.width = ((self.width as f32 * scale).round() as u32).max(1);
        self.height = ((self.height as f32 * scale).round() as u32).max(1);
        self
    }

    /// Renders a frame into an image for `<image href={image.href()} .../>`.
    ///
    /// With [`Renderer::gpu`] and a backend that shares its GPU (the Skia Vulkan backend
    /// with its `wgpu` feature), the image is a texture on that GPU. The backend draws it
    /// there, so the frame does not go back to the CPU. In all other cases the image holds
    /// pixels.
    pub fn render(&self, scene: &Scene, camera: &Camera) -> fframes_media::ImageData<'static> {
        self.image(&[(scene, camera)])
    }

    /// Renders a frame into pixels.
    pub fn render_pixels(&self, scene: &Scene, camera: &Camera) -> Pixels {
        self.pixels(&[(scene, camera)])
    }

    /// Renders a frame with motion blur. It renders `samples` moments in the shutter time
    /// and averages them before bloom and tone mapping.
    ///
    /// `moment(offset)` makes the scene and the camera at `offset`, in `-0.5..0.5`. The
    /// shutter is centered on the frame. Map the offset to time, for example
    /// `t + offset / fps * 0.5` for a half open shutter.
    ///
    /// ```rust,ignore
    /// let image = renderer.render_motion_blur(4, |offset| self.build(t + offset / 60.0));
    /// ```
    pub fn render_motion_blur(
        &self,
        samples: usize,
        moment: impl FnMut(f32) -> (Scene, Camera),
    ) -> fframes_media::ImageData<'static> {
        let moments: Vec<(Scene, Camera)> = shutter_offsets(samples).map(moment).collect();
        let moments: Vec<(&Scene, &Camera)> = moments.iter().map(|(s, c)| (s, c)).collect();
        self.image(&moments)
    }

    /// [`Renderer::render_motion_blur`] into pixels.
    pub fn render_motion_blur_pixels(
        &self,
        samples: usize,
        moment: impl FnMut(f32) -> (Scene, Camera),
    ) -> Pixels {
        let moments: Vec<(Scene, Camera)> = shutter_offsets(samples).map(moment).collect();
        let moments: Vec<(&Scene, &Camera)> = moments.iter().map(|(s, c)| (s, c)).collect();
        self.pixels(&moments)
    }

    /// The average of the moments as an image. It stays on the GPU when the backend shares
    /// its GPU.
    fn image(&self, moments: &[(&Scene, &Camera)]) -> fframes_media::ImageData<'static> {
        #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
        if self.gpu {
            if let Some(turn) = self.cpu_turn() {
                return turn.install(|| self.cpu_pixels(moments)).into_image();
            }
            if let Some(image) = crate::gpu::layer(self, moments) {
                return image;
            }
        }
        self.gpu_or_cpu_pixels(moments).into_image()
    }

    /// The average of the moments as pixels.
    fn pixels(&self, moments: &[(&Scene, &Camera)]) -> Pixels {
        #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
        if self.gpu
            && let Some(turn) = self.cpu_turn()
        {
            return turn.install(|| self.cpu_pixels(moments));
        }
        self.gpu_or_cpu_pixels(moments)
    }

    /// The average of the moments as pixels of the GPU, or of the CPU without a GPU.
    fn gpu_or_cpu_pixels(&self, moments: &[(&Scene, &Camera)]) -> Pixels {
        #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
        if self.gpu
            && let Some(gpu) = crate::gpu::gpu()
        {
            return gpu.render(self, moments);
        }
        self.cpu_pixels(moments)
    }

    /// The frame of a CPU run of [`Renderer::hybrid`], `None` for the GPU.
    #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
    fn cpu_turn(&self) -> Option<hybrid::CpuTurn> {
        let gpu = crate::gpu::gpu()?;
        hybrid::cpu_turn(self.hybrid.as_ref()?, &gpu)
    }

    /// The average of the moments as pixels of the CPU rasterizer. The moments render at
    /// the same time.
    fn cpu_pixels(&self, moments: &[(&Scene, &Camera)]) -> Pixels {
        let mut hdr = moments
            .par_iter()
            .map(|(scene, camera)| self.render_hdr(scene, camera))
            .collect::<Vec<_>>()
            .into_iter()
            .reduce(|mut sum, hdr| {
                sum.iter_mut().zip(&hdr).for_each(|(a, b)| *a += *b);
                sum
            })
            .unwrap_or_default();
        if moments.len() > 1 {
            let scale = 1.0 / moments.len() as f32;
            for p in &mut hdr {
                *p *= scale;
            }
        }
        let (width, height) = (self.width as usize, self.height as usize);
        if let Some(bloom) = &self.bloom {
            post::bloom(&mut hdr, width, height, bloom);
        }
        Pixels {
            width: self.width,
            height: self.height,
            rgba: post::encode(&hdr, width, height, self.exposure, self.tone_mapping),
        }
    }

    /// Premultiplied linear HDR pixels, before bloom and tone mapping.
    fn render_hdr(&self, scene: &Scene, camera: &Camera) -> Vec<Vec4> {
        let (width, height) = (self.width as usize, self.height as usize);
        let (mut world, lights) = prepare::build_world(scene);
        let shadows: Vec<Option<ShadowMap>> = lights
            .shadow_casters
            .iter()
            .map(|(direction, settings)| ShadowMap::build(*direction, settings, &world))
            .collect();
        let baked = scene.environment.baked();
        let ortho = camera.ortho_height.is_some();
        let view_proj = camera.view_projection(width as f32 / height as f32);
        // The shading of the frame. The reflection changes only the camera fields.
        let frame = ShadeContext {
            lights: &lights,
            shadows: &shadows,
            environment: &scene.environment,
            baked: baked.as_deref(),
            fog: scene.fog,
            background: &scene.background,
            camera_position: camera.position,
            ortho_forward: ortho.then(|| camera.forward()),
            reflection: None,
            viewport: Vec2::new(width as f32, height as f32),
        };

        // The reflection renders first, because the surfaces that show it sample it.
        let reflection = scene
            .reflector
            .filter(|_| world.draws.iter().any(|d| d.material.reflection > 0.0))
            .map(|reflector| {
                let size = (frame.viewport * reflector.scale).ceil().max(Vec2::ONE);
                let mirror = reflector.matrix();
                let shade = ShadeContext {
                    camera_position: mirror.transform_point3(camera.position),
                    ortho_forward: frame.ortho_forward.map(|f| mirror.transform_vector3(f)),
                    viewport: size,
                    ..frame
                };
                let viewport = Viewport {
                    size,
                    view_proj: view_proj * mirror,
                    ortho,
                    mirrored: true,
                    clip_plane: Some(reflector.plane()),
                };
                let (hdr, _) = render_view(&mut world, &shade, &viewport, camera, false);
                ReflectionMap::new(size.x as usize, size.y as usize, hdr)
            });

        let shade = ShadeContext {
            reflection: reflection.as_ref(),
            ..frame
        };
        let viewport = Viewport {
            size: frame.viewport,
            view_proj,
            ortho,
            mirrored: false,
            clip_plane: None,
        };
        let (mut hdr, depth) = render_view(&mut world, &shade, &viewport, camera, self.antialias);
        if let Some(focus) = camera.focus
            && !ortho
            && focus.blur > 0.0
        {
            // `blur` is in pixels of a 1080 pixel tall frame
            let scale = height as f32 / 1080.0;
            post::depth_of_field(
                &mut hdr,
                &depth,
                width,
                height,
                focus.distance,
                focus.blur * scale,
            );
        }
        hdr
    }
}

/// The moments of a shutter that is centered on the frame, in `-0.5..0.5`.
fn shutter_offsets(samples: usize) -> impl Iterator<Item = f32> {
    let samples = samples.max(1);
    (0..samples).map(move |i| (i as f32 + 0.5) / samples as f32 - 0.5)
}

/// Renders one view into premultiplied linear HDR. Also returns the depth key of the
/// nearest opaque surface of each pixel.
///
/// The mirrored view of a reflection leaves out the particles and the surfaces that show
/// the reflection.
fn render_view(
    world: &mut World,
    shade: &ShadeContext,
    viewport: &Viewport,
    camera: &Camera,
    multisample: bool,
) -> (Vec<Vec4>, Vec<f32>) {
    let reflection = viewport.mirrored;
    let (opaque, blended_tris) = prepare::setup_triangles(world, viewport, |d| {
        !reflection || d.material.reflection <= 0.0
    });
    let sprites = if reflection {
        Vec::new()
    } else {
        prepare::setup_sprites(&world.sprites, camera, viewport)
    };
    let mut blended: Vec<Blended> = blended_tris
        .iter()
        .map(Blended::Tri)
        .chain(sprites.iter().map(Blended::Sprite))
        .collect();
    blended.sort_by(|a, b| b.depth().total_cmp(&a.depth()));
    raster::rasterize(&raster::Frame {
        width: viewport.size.x as usize,
        height: viewport.size.y as usize,
        streams: &world.streams,
        draws: &world.draws,
        opaque: &opaque,
        blended: &blended,
        shade,
        inverse_view_proj: viewport.view_proj.inverse(),
        multisample,
    })
}
