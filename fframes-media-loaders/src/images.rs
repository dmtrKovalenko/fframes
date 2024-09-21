#[cfg(not(target_arch = "wasm32"))]
pub use usvgr::PreloadedImageData;

#[derive(Debug, Clone)]
pub struct ImageData {
    #[cfg(not(target_arch = "wasm32"))]
    pub image: std::sync::Arc<usvgr::PreloadedImageData>,
    #[cfg(target_arch = "wasm32")]
    pub base64_data: std::borrow::Cow<'static, str>,
    pub filename: String,
}

impl ImageData {
    #[cfg(target_arch = "wasm32")]
    pub fn href(&self) -> &str {
        &self.base64_data
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn href(&self) -> std::sync::Arc<usvgr::PreloadedImageData> {
        std::sync::Arc::clone(&self.image)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn decode_image(
    filename: &str,
    data: &[u8],
) -> crate::error::Result<usvgr::PreloadedImageData> {
    let buffer =
        image::load_from_memory(data).map_err(crate::error::FFramesMediaError::ImageError)?;

    Ok(usvgr::PreloadedImageData::new(
        if filename.ends_with(".png") {
            "png".to_owned()
        } else {
            "jpeg".to_owned()
        },
        buffer.width(),
        buffer.height(),
        &buffer.to_rgba8().into_raw(),
    ))
}
