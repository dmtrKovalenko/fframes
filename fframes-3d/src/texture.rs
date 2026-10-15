use std::sync::{Arc, OnceLock};

use glam::{Vec2, Vec4};

use crate::Rgb;
use crate::color::{linear_to_srgb, srgb_to_linear};

/// An RGBA8 image that materials sample. The image repeats outside `0..1`. Sampling is
/// trilinear between mipmap levels that match the pixel footprint, so fine detail (floor
/// tiles, text) does not shimmer. Clones share the image.
#[derive(Clone)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    levels: Arc<Levels>,
    /// The image the texture comes from. While it lives, the backend does not draw into its
    /// texture on the GPU again, see [`Levels::layer`].
    #[cfg(not(target_arch = "wasm32"))]
    _image: Option<Arc<fframes::usvgr::PreloadedImageData>>,
    /// True for color data. The texture keeps it as sRGB and converts it to linear when it
    /// samples it. False for linear data such as metalness and roughness.
    pub srgb: bool,
    /// UV scale applied before sampling: `uv * repeat + offset`.
    pub repeat: Vec2,
    /// UV offset applied before sampling, after `repeat`.
    pub offset: Vec2,
}

/// The mipmap levels of a texture. Level 0 is the image. Each next level has half the size,
/// down to one texel.
pub(crate) struct Levels {
    cpu: OnceLock<Vec<Level>>,
    /// The width and the height of level 0.
    size: (u32, u32),
    /// The texture on the GPU that the image is (feature `gpu`): an svgr image or a 3D
    /// render that stays on the GPU. The GPU renderer makes the levels from it on the GPU.
    /// The CPU levels are its pixels, read back on first use. It is weak: the layer and its
    /// device go with the image and with the backend.
    #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
    layer: Option<std::sync::Weak<fframes::WgpuLayer>>,
}

impl Levels {
    fn cpu(levels: Vec<Level>) -> Arc<Self> {
        Arc::new(Self {
            size: (levels[0].width, levels[0].height),
            cpu: OnceLock::from(levels),
            #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
            layer: None,
        })
    }

    /// The texture on the GPU that the levels come from.
    #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
    pub(crate) fn layer(&self) -> Option<Arc<fframes::WgpuLayer>> {
        self.layer.as_ref()?.upgrade()
    }
}

#[derive(Clone)]
struct Level {
    width: u32,
    height: u32,
    data: Vec<[u8; 4]>,
}

impl std::fmt::Debug for Texture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Texture")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("srgb", &self.srgb)
            .field("levels", &self.cpu_levels().len())
            .finish_non_exhaustive()
    }
}

impl Texture {
    /// Makes a texture from tightly packed rows of straight (not premultiplied) RGBA8.
    pub fn from_rgba8(width: u32, height: u32, rgba: &[u8], srgb: bool) -> Self {
        let data = rgba.as_chunks::<4>().0.to_vec();
        Self::from_level(
            Level {
                width,
                height,
                data,
            },
            srgb,
        )
    }

    /// Decodes a PNG, JPEG or WebP image as a color texture. The bytes can come from
    /// `include_bytes!` or from an embedded media file.
    pub fn decode(bytes: &[u8]) -> Result<Self, image::ImageError> {
        let image = image::load_from_memory(bytes)?.to_rgba8();
        Ok(Self::from_rgba8(
            image.width(),
            image.height(),
            image.as_raw(),
            true,
        ))
    }

    /// A color texture from an fframes image: a media file, a decoded video frame, a
    /// rendered frame of another video, an svgr tree from `ctx.render_svgr`, or a 3D render.
    /// Use a new texture for each frame of a video, so a screen in the scene plays it.
    ///
    /// The texture makes its mipmap levels on the CPU, once for each image: a texture of
    /// the same image again (a static svgr tree, a media file) reuses them. Resize large
    /// video frames first (`into_resized_image`). An image that stays on the GPU (an svgr
    /// image or a 3D render on the Skia Vulkan backend with the `wgpu` feature) gets its
    /// levels on the GPU with the `gpu` feature and does not leave the GPU.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_image(image: &fframes::media::ImageData<'_>) -> Self {
        let source = &image.image;
        let levels = images::levels(source, || {
            #[cfg(feature = "gpu")]
            if let Some(layer) = fframes::resolve_wgpu_layer(source) {
                return Arc::new(Levels {
                    cpu: OnceLock::new(),
                    layer: Some(Arc::downgrade(&layer)),
                    size: (layer.texture().width(), layer.texture().height()),
                });
            }
            // an image on the GPU without the `gpu` feature is read back
            Levels::cpu(match fframes::image_pixels(source) {
                Some(pixels) => levels_of_premultiplied(&pixels, true),
                None => transparent(),
            })
        });
        Self {
            _image: Some(Arc::clone(source)),
            ..Self::with_levels(levels, true)
        }
    }

    /// Decodes a PNG, JPEG or WebP image of linear data: a normal, roughness, metalness or
    /// ambient occlusion map.
    pub fn decode_linear(bytes: &[u8]) -> Result<Self, image::ImageError> {
        let image = image::load_from_memory(bytes)?.to_rgba8();
        Ok(Self::from_rgba8(
            image.width(),
            image.height(),
            image.as_raw(),
            false,
        ))
    }

    /// Makes a color texture from `f(u, v)`, which returns an sRGB color for `u` and `v` in
    /// `0..1`.
    pub fn from_fn(width: u32, height: u32, f: impl Fn(f32, f32) -> fframes::Color) -> Self {
        Self::from_fn_data(width, height, true, |u, v| {
            let c = f(u, v);
            [c.r, c.g, c.b, c.a]
        })
    }

    /// Makes a texture from `f(u, v)`, which returns raw RGBA bytes for `u` and `v` in
    /// `0..1`. For example, a glTF metallic-roughness map has roughness in green and
    /// metalness in blue, with `srgb` false.
    pub fn from_fn_data(
        width: u32,
        height: u32,
        srgb: bool,
        f: impl Fn(f32, f32) -> [u8; 4],
    ) -> Self {
        let mut data = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            let v = (y as f32 + 0.5) / height as f32;
            for x in 0..width {
                data.push(f((x as f32 + 0.5) / width as f32, v));
            }
        }
        Self::from_level(
            Level {
                width,
                height,
                data,
            },
            srgb,
        )
    }

    /// A checkerboard of `size` by `size` texels with `cells` squares per side, in colors
    /// `a` and `b`.
    pub fn checker(size: u32, cells: u32, a: impl Into<Rgb>, b: impl Into<Rgb>) -> Self {
        let (a, b) = (a.into().to_color(), b.into().to_color());
        Self::from_fn(size, size, |u, v| {
            let cell = (u * cells as f32) as u32 + (v * cells as f32) as u32;
            if cell.is_multiple_of(2) { a } else { b }
        })
    }

    fn from_level(level: Level, srgb: bool) -> Self {
        Self::with_levels(Levels::cpu(mip_chain(level, srgb)), srgb)
    }

    fn with_levels(levels: Arc<Levels>, srgb: bool) -> Self {
        Self {
            width: levels.size.0,
            height: levels.size.1,
            levels,
            #[cfg(not(target_arch = "wasm32"))]
            _image: None,
            srgb,
            repeat: Vec2::ONE,
            offset: Vec2::ZERO,
        }
    }

    /// The mipmap levels. Clones of the texture share them.
    #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
    pub(crate) fn shared_levels(&self) -> &Arc<Levels> {
        &self.levels
    }

    /// The mipmap levels as width, height and RGBA8 texels, largest first.
    #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
    pub(crate) fn levels(&self) -> impl Iterator<Item = (u32, u32, &[[u8; 4]])> {
        self.cpu_levels()
            .iter()
            .map(|l| (l.width, l.height, l.data.as_slice()))
    }

    /// The mipmap levels on the CPU. A texture on the GPU reads them back on first use.
    fn cpu_levels(&self) -> &[Level] {
        self.levels.cpu.get_or_init(|| {
            #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
            if let Some(pixels) = self.levels.layer().and_then(|l| l.pixels()) {
                return levels_of_premultiplied(&pixels, self.srgb);
            }
            transparent()
        })
    }

    /// Sets how many times the texture repeats along U and V.
    pub fn repeat(mut self, u: f32, v: f32) -> Self {
        self.repeat = Vec2::new(u, v);
        self
    }

    /// Bilinear sample of the full-size image. Returns linear RGBA with straight alpha.
    pub fn sample(&self, uv: Vec2) -> Vec4 {
        self.sample_lod(uv, 0.0)
    }

    /// Trilinear sample at a mipmap level. Level 0 is the full size, each next level has
    /// half the size. Returns linear RGBA with straight alpha.
    pub fn sample_lod(&self, uv: Vec2, lod: f32) -> Vec4 {
        let uv = uv * self.repeat + self.offset;
        let levels = self.cpu_levels();
        let max = (levels.len() - 1) as f32;
        let lod = if lod.is_finite() {
            lod.clamp(0.0, max)
        } else {
            0.0
        };
        let l0 = lod.floor() as usize;
        let a = self.bilinear(&levels[l0], uv);
        let f = lod - l0 as f32;
        if f < 1e-3 || l0 + 1 >= levels.len() {
            return a;
        }
        a.lerp(self.bilinear(&levels[l0 + 1], uv), f)
    }

    /// Anisotropic sample like the GPU sampler (`anisotropy_clamp` 8). `dx` and `dy` are the
    /// UV changes to the next pixel right and down, measured before `repeat`. Up to 8
    /// trilinear samples along the long axis of the pixel footprint, at the mipmap level of
    /// its short axis, so that a surface seen at a low angle stays as sharp as on the GPU.
    pub fn sample_grad(&self, uv: Vec2, dx: Vec2, dy: Vec2) -> Vec4 {
        const MAX_ANISOTROPY: f32 = 8.0;
        let texels = Vec2::new(self.width as f32, self.height as f32) * self.repeat;
        let (x, y) = ((dx * texels).length(), (dy * texels).length());
        let (major, minor, axis) = if x >= y { (x, y, dx) } else { (y, x, dy) };
        let count = (major / minor.max(1e-6)).ceil().clamp(1.0, MAX_ANISOTROPY);
        let lod = (major / count).max(1e-6).log2();
        if count <= 1.0 || !count.is_finite() {
            return self.sample_lod(uv, lod);
        }
        let sum = (0..count as usize).fold(Vec4::ZERO, |sum, i| {
            let t = (i as f32 + 0.5) / count - 0.5;
            sum + self.sample_lod(uv + axis * t, lod)
        });
        sum / count
    }

    /// The mipmap level for a pixel that covers `footprint` UV units, measured before
    /// `repeat`.
    pub fn lod(&self, footprint: f32) -> f32 {
        let texels = footprint * self.repeat.max_element() * self.width.max(self.height) as f32;
        texels.max(1e-6).log2()
    }

    fn bilinear(&self, level: &Level, uv: Vec2) -> Vec4 {
        let x = uv.x * level.width as f32 - 0.5;
        let y = uv.y * level.height as f32 - 0.5;
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (w, h) = (level.width as i32, level.height as i32);
        let xi = (x0 as i32).rem_euclid(w);
        let yi = (y0 as i32).rem_euclid(h);
        let xi1 = (xi + 1) % w;
        let yi1 = (yi + 1) % h;
        let lut = self.srgb.then(srgb_lut);
        let texel = |x: i32, y: i32| decode(level.data[(y * w + x) as usize], lut);
        let top = texel(xi, yi).lerp(texel(xi1, yi), fx);
        let bottom = texel(xi, yi1).lerp(texel(xi1, yi1), fx);
        top.lerp(bottom, fy)
    }
}

/// One transparent texel, for an image that has no pixels to read.
#[cfg(not(target_arch = "wasm32"))]
fn transparent() -> Vec<Level> {
    vec![Level {
        width: 1,
        height: 1,
        data: vec![[0; 4]],
    }]
}

/// Level 0 and the levels below it.
fn mip_chain(level: Level, srgb: bool) -> Vec<Level> {
    let mut levels = vec![level];
    while let Some(last) = levels.last()
        && (last.width > 1 || last.height > 1)
    {
        let next = downsample(last, srgb);
        levels.push(next);
    }
    levels
}

/// The levels of premultiplied pixels (how fframes keeps images). The texture keeps
/// straight alpha.
#[cfg(not(target_arch = "wasm32"))]
fn levels_of_premultiplied(image: &fframes::usvgr::PreloadedImageData, srgb: bool) -> Vec<Level> {
    let data = image
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&[r, g, b, a]| match a {
            0 | 255 => [r, g, b, a],
            _ => {
                let straight = |c: u8| (u32::from(c) * 255 / u32::from(a)).min(255) as u8;
                [straight(r), straight(g), straight(b), a]
            }
        })
        .collect();
    mip_chain(
        Level {
            width: image.width,
            height: image.height,
            data,
        },
        srgb,
    )
}

/// The levels of the images that textures were made of, so that a texture of the same
/// image again reuses them.
#[cfg(not(target_arch = "wasm32"))]
mod images {
    use std::num::NonZeroUsize;
    use std::sync::{Arc, Mutex, PoisonError, Weak};

    use fframes::lru::LruCache;
    use fframes::usvgr::PreloadedImageData;

    use super::Levels;

    /// Images that live for a whole render (media files) stay in the cache. This bounds
    /// their memory.
    const CAPACITY: NonZeroUsize = NonZeroUsize::new(32).unwrap();

    type Entry = (Weak<PreloadedImageData>, Arc<Levels>);

    static IMAGES: Mutex<Option<LruCache<usize, Entry>>> = Mutex::new(None);

    /// The levels of `image`: the ones made before, or the ones `make` makes. The cache
    /// holds the image weakly. The entry goes when the image is dropped.
    pub(super) fn levels(
        image: &Arc<PreloadedImageData>,
        make: impl FnOnce() -> Arc<Levels>,
    ) -> Arc<Levels> {
        let key = Arc::as_ptr(image) as usize;
        let found = {
            let mut images = IMAGES.lock().unwrap_or_else(PoisonError::into_inner);
            let images = images.get_or_insert_with(|| LruCache::new(CAPACITY));
            let dead: Vec<usize> = (images.iter())
                .filter(|(_, (image, _))| image.strong_count() == 0)
                .map(|(key, _)| *key)
                .collect();
            for key in dead {
                images.pop(&key);
            }
            images.get(&key).map(|(_, levels)| Arc::clone(levels))
        };
        // the levels are made without the lock, so other threads go on
        found.unwrap_or_else(|| {
            let levels = make();
            (IMAGES.lock().unwrap_or_else(PoisonError::into_inner))
                .get_or_insert_with(|| LruCache::new(CAPACITY))
                .put(key, (Arc::downgrade(image), Arc::clone(&levels)));
            levels
        })
    }
}

/// Converts a stored texel to linear RGBA in `0..1`. `lut` converts sRGB color channels;
/// without it the texel is linear data. Alpha is always linear.
#[inline]
fn decode(c: [u8; 4], lut: Option<&[f32; 256]>) -> Vec4 {
    match lut {
        Some(lut) => Vec4::new(
            lut[c[0] as usize],
            lut[c[1] as usize],
            lut[c[2] as usize],
            f32::from(c[3]) / 255.0,
        ),
        None => Vec4::from_array(c.map(f32::from)) / 255.0,
    }
}

/// Makes the next mipmap level: half the size, each texel the mean of four texels in
/// linear light.
fn downsample(level: &Level, srgb: bool) -> Level {
    let width = (level.width / 2).max(1);
    let height = (level.height / 2).max(1);
    let lut = srgb.then(srgb_lut);
    let encode = |v: f32, srgb: bool| {
        let v = v.clamp(0.0, 1.0);
        let v = if srgb { linear_to_srgb(v) } else { v };
        (v * 255.0 + 0.5) as u8
    };
    let mut data = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let mut sum = Vec4::ZERO;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let sx = (x * 2 + dx).min(level.width - 1);
                let sy = (y * 2 + dy).min(level.height - 1);
                sum += decode(level.data[(sy * level.width + sx) as usize], lut);
            }
            let mean = sum / 4.0;
            data.push([
                encode(mean.x, srgb),
                encode(mean.y, srgb),
                encode(mean.z, srgb),
                encode(mean.w, false),
            ]);
        }
    }
    Level {
        width,
        height,
        data,
    }
}

/// sRGB to linear conversion for every 8-bit value.
fn srgb_lut() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| std::array::from_fn(|i| srgb_to_linear(i as f32 / 255.0)))
}
