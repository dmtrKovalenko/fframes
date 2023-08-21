use crate::error::Result;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
pub use usvgr::PreloadedImageData;

#[derive(Debug, Clone)]
pub struct ImageData {
    #[cfg(not(target_arch = "wasm32"))]
    pub image: Arc<usvgr::PreloadedImageData>,
    #[cfg(target_arch = "wasm32")]
    pub base64_data: std::borrow::Cow<'static, str>,
    pub filename: String,
}

impl ImageData {
    pub fn href(&self) -> &str {
        #[cfg(not(target_arch = "wasm32"))]
        {
            &self.filename
        }
        #[cfg(target_arch = "wasm32")]
        {
            &self.base64_data
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn decode_image(filename: &str, data: &[u8]) -> Result<usvgr::PreloadedImageData> {
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
