//! Skia images backed by `usvgr::PreloadedImageData`.

use std::sync::Arc;

use fframes::usvgr::PreloadedImageData;
use skia_safe::{AlphaType, ColorType, Data, ISize, Image, ImageInfo, images::raster_from_data};

/// A Skia image that views the pixels of a `PreloadedImageData` without
/// copying them.
///
/// The `Arc` is held alongside the Skia image so the pixels can not be freed
/// (or their address recycled by a new allocation) while the image is
/// cached — a GPU backend may read them as late as the flush that follows
/// the draw.
pub(super) struct SkiaImage {
    _pixels: Arc<PreloadedImageData>,
    image: Image,
}

impl SkiaImage {
    pub(super) fn new(pixels: &Arc<PreloadedImageData>) -> Option<Self> {
        let image_info = ImageInfo::new(
            ISize::new(pixels.width as i32, pixels.height as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            None,
        );

        let image = raster_from_data(
            &image_info,
            // SAFETY: `_pixels` keeps the backing allocation alive for as
            // long as this struct, and `image` never outlives it.
            unsafe { Data::new_bytes(&pixels.data) },
            pixels.width as usize * 4,
        )?;

        Some(Self {
            _pixels: Arc::clone(pixels),
            image,
        })
    }

    pub(super) fn image(&self) -> &Image {
        &self.image
    }
}
