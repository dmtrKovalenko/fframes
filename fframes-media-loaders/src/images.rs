#[cfg(not(target_arch = "wasm32"))]
pub use usvgr::PreloadedImageData;

#[cfg(feature = "exif")]
use exif::Field;
#[cfg(feature = "exif")]
use std::sync::OnceLock;
#[cfg(feature = "exif")]
#[derive(Debug, Clone)]
pub struct CExif {
    pub fields: Vec<Field>,
}

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

    #[cfg(target_arch = "wasm32")]
    /// The URL to the image using the preview
    pub web_url: String,

    #[cfg(not(target_arch = "wasm32"))]
    image: std::sync::Arc<usvgr::PreloadedImageData>,
    #[cfg(target_arch = "wasm32")]
    base64_data: Base64ImageData,

    #[cfg(feature = "exif")]
    pub exif_data: Option<CExif>,

    #[cfg(feature = "exif")]
    datetime_cache: OnceLock<Option<Field>>,
}

impl ImageData {
    #[cfg(target_arch = "wasm32")]
    pub fn href(&self) -> &str {
        crate::IS_PREVIEW_RENDERING.with(|force_base64| {
            if force_base64.load(std::sync::atomic::Ordering::Relaxed) {
                self.base64_data.as_str()
            } else {
                &self.web_url
            }
        })
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
        #[cfg(feature = "exif")] exif_data: Option<CExif>,
    ) -> Self {
        ImageData {
            filename,
            metadata,
            image: data,
            #[cfg(feature = "exif")]
            exif_data,
            #[cfg(feature = "exif")]
            datetime_cache: OnceLock::new(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_from_bytes(filename: &str, bytes: &[u8]) -> crate::Result<Self> {
        let image = decode_image(filename, bytes)?;
        #[cfg(feature = "exif")]
        let exif_data = parse_exif_data(filename, bytes);

        let metadata = ImageMetadata {
            width: image.width,
            height: image.height,
        };

        Ok(Self {
            filename: filename.to_string(),
            metadata,
            #[cfg(not(target_arch = "wasm32"))]
            image: std::sync::Arc::new(image),
            #[cfg(feature = "exif")]
            exif_data,
            #[cfg(feature = "exif")]
            datetime_cache: OnceLock::new(),
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new_from_base_64_data(
        data: Base64ImageData,
        file_url: String,
        filename: String,
        metadata: ImageMetadata,
    ) -> Self {
        ImageData {
            filename,
            metadata,
            web_url: file_url,
            base64_data: data.clone(),
            #[cfg(feature = "exif")]
            exif_data: None,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_bytes(&self) -> &[u8] {
        &self.image.data
    }

    #[cfg(target_arch = "wasm32")]
    pub fn get_bytes(&mut self) -> &[u8] {
        self.base64_data.as_str().as_bytes()
    }

    #[cfg(feature = "exif")]
    pub fn get_year(&self) -> String {
        let field_opt = self.datetime_cache.get_or_init(|| {
            self.exif_data.as_ref().and_then(|exif| {
                exif.fields
                    .iter()
                    .find(|entry| entry.tag == exif::Tag::DateTime)
                    .cloned()
            })
        });

        field_opt
            .as_ref()
            .map(|field| {
                field
                    .display_value()
                    .to_string()
                    .get(..4)
                    .unwrap_or("")
                    .to_string()
            })
            .unwrap_or_else(|| "2075".to_string())
    }
}

#[cfg(feature = "exif")]
pub fn parse_exif_data(path: impl AsRef<std::path::Path>, photo: &[u8]) -> Option<CExif> {
    let cursor = std::io::Cursor::new(photo);
    let mut bufreader = std::io::BufReader::new(cursor);
    let exifreader = exif::Reader::new();

    match exifreader.read_from_container(&mut bufreader) {
        Ok(exif) => Some(CExif {
            fields: exif.fields().cloned().collect(),
        }),
        Err(e) => {
            eprintln!(
                "Error to parse file: {} with error: {}",
                path.as_ref().display(),
                e
            );
            None
        }
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
