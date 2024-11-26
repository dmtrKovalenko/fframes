#[cfg(not(target_arch = "wasm32"))]
pub use usvgr::PreloadedImageData;

/// A more efficient version of `Cow` that is cheap to clone
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
pub enum Base64ImageData {
    BorrowedStatic(&'static str),
    OwnedShared(std::sync::Arc<str>),
}

#[cfg(target_arch = "wasm32")]
impl Base64ImageData {
    pub fn new_owned<T: Into<std::sync::Arc<str>>>(s: T) -> Self {
        Base64ImageData::OwnedShared(s.into())
    }

    pub fn as_str(&self) -> &str {
        match self {
            Base64ImageData::BorrowedStatic(s) => s,
            Base64ImageData::OwnedShared(s) => s,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct ImageData {
    pub filename: String,
    pub metadata: ImageMetadata,

    #[cfg(not(target_arch = "wasm32"))]
    image: std::sync::Arc<usvgr::PreloadedImageData>,
    #[cfg(target_arch = "wasm32")]
    base64_data: Base64ImageData,
}

impl ImageData {
    #[cfg(target_arch = "wasm32")]
    pub fn href(&self) -> &str {
        &self.base64_data.as_str()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn href(&self) -> std::sync::Arc<usvgr::PreloadedImageData> {
        std::sync::Arc::clone(&self.image)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_from_raw_data(
        data: std::sync::Arc<usvgr::PreloadedImageData>,
        filename: String,
        metadata: ImageMetadata,
    ) -> Self {
        ImageData {
            filename,
            metadata,
            image: data,
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new_from_raw_data(
        data: Base64ImageData,
        filename: String,
        metadata: ImageMetadata,
    ) -> Self {
        ImageData {
            filename,
            metadata,
            base64_data: data.clone(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_bytes(&self) -> &[u8] {
        &self.image.data
    }

    #[cfg(target_arch = "wasm32")]
    pub fn get_bytes_mut(&mut self) -> &mut [u8] {
        unimplemented!()
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
        filename.to_owned(),
        buffer.width(),
        buffer.height(),
        &buffer.to_rgba8().into_raw(),
    ))
}
