use std::borrow::Cow;
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
pub use usvgr::PreloadedImageData;

/// A more efficient version of `Cow` that is cheap to clone
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone)]
pub enum OwnedSharedString {
    BorrowedStatic(&'static str),
    OwnedShared(Arc<str>),
}

#[cfg(target_arch = "wasm32")]
impl OwnedSharedString {
    pub fn new_owned<T: Into<Arc<str>>>(s: T) -> Self {
        OwnedSharedString::OwnedShared(s.into())
    }

    pub fn as_str(&self) -> &str {
        match self {
            OwnedSharedString::BorrowedStatic(s) => s,
            OwnedSharedString::OwnedShared(s) => s,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImageData<'a> {
    pub filename: String,
    pub metadata: ImageMetadata,

    #[cfg(target_arch = "wasm32")]
    /// The URL to the image using the preview
    pub web_url: String,
    /// This could be anything available to be rendered in the web preview
    /// base64, blob, url, etc
    #[cfg(target_arch = "wasm32")]
    web_source_data: OwnedSharedString,
    #[cfg(target_arch = "wasm32")]
    _marker: std::marker::PhantomData<&'a ()>,

    #[cfg(not(target_arch = "wasm32"))]
    pub image: Arc<usvgr::PreloadedImageData>,
    #[allow(dead_code)]
    #[cfg(not(target_arch = "wasm32"))]
    container_bytes: Option<&'a [u8]>,

    #[cfg(feature = "exif")]
    pub exif_data: Arc<std::sync::OnceLock<Option<ExifData>>>,
}

impl<'a> ImageData<'a> {
    #[cfg(target_arch = "wasm32")]
    pub fn href(&self) -> &str {
        crate::IS_PREVIEW_RENDERING.with(|force_base64| {
            if force_base64.load(std::sync::atomic::Ordering::Relaxed) {
                self.web_source_data.as_str()
            } else {
                &self.web_url
            }
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn href(&self) -> Arc<usvgr::PreloadedImageData> {
        Arc::clone(&self.image)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_from_raw_data(
        data: Arc<usvgr::PreloadedImageData>,
        filename: String,
        metadata: ImageMetadata,
    ) -> Self {
        ImageData {
            filename,
            metadata,
            image: data,
            container_bytes: None,
            #[cfg(feature = "exif")]
            exif_data: Arc::new(std::sync::OnceLock::new()),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_from_bytes(filename: &str, bytes: &'a [u8]) -> crate::Result<Self> {
        let buffer = image::load_from_memory(bytes)?;
        let image = usvgr::PreloadedImageData::new(
            filename.to_owned(),
            buffer.width(),
            buffer.height(),
            &buffer.to_rgba8().into_raw(),
        );

        let metadata = ImageMetadata {
            width: image.width,
            height: image.height,
        };

        Ok(Self {
            filename: filename.to_string(),
            metadata,
            #[cfg(not(target_arch = "wasm32"))]
            image: Arc::new(image),
            container_bytes: Some(bytes),
            #[cfg(feature = "exif")]
            exif_data: Arc::new(std::sync::OnceLock::new()),
        })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new_from_web_source(
        data: OwnedSharedString,
        file_url: String,
        filename: String,
        metadata: ImageMetadata,
    ) -> Self {
        ImageData {
            filename,
            metadata,
            web_url: file_url,
            web_source_data: data.clone(),
            #[cfg(feature = "exif")]
            exif_data: Arc::new(std::sync::OnceLock::new()),
            _marker: std::marker::PhantomData,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn get_container_bytes(&self) -> crate::Result<Cow<'_, [u8]>> {
        let Some(container_bytes) = self.container_bytes else {
            return Err(crate::error::FFramesMediaError::MediaDirectoryProvided);
        };

        Ok(Cow::Borrowed(container_bytes))
    }

    #[cfg(target_arch = "wasm32")]
    pub fn get_container_bytes(&self) -> crate::Result<Cow<'_, [u8]>> {
        use base64::Engine;
        use base64::engine::general_purpose::STANDARD as base64_engine;

        let encoded_str = self.web_source_data.as_str();
        let base64_str = if let Some(index) = encoded_str.find("base64,") {
            &encoded_str[index + 7..] // 7 is the length of "base64,"
        } else {
            encoded_str
        };

        let bytes = base64_engine.decode(base64_str)?;
        Ok(Cow::Owned(bytes))
    }

    /// Parses the EXIF data from the container on request, caches all the data and returns
    /// the requested field if it exists, faster after the first call.
    #[cfg(feature = "exif")]
    pub fn get_exif_data(&self, tag: exif::Tag) -> Option<&exif::Field> {
        let exif_data = self
            .exif_data
            .get_or_init(|| {
                let container_bytes = self.get_container_bytes().ok()?;
                ExifData::parse(container_bytes.as_ref()).ok()
            })
            .as_ref()?;

        exif_data.0.get_field(tag, exif::In::PRIMARY)
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn decode_image(
    filename: &str,
    data: &[u8],
) -> crate::error::Result<usvgr::PreloadedImageData> {
    let buffer = image::load_from_memory(data)?;

    Ok(usvgr::PreloadedImageData::new(
        filename.to_owned(),
        buffer.width(),
        buffer.height(),
        &buffer.to_rgba8().into_raw(),
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ImageMetadata {
    pub width: u32,
    pub height: u32,
}

#[cfg(feature = "exif")]
pub struct ExifData(exif::Exif);

// We always guarantee that the exif data would not have a mutable access if it is initialized
#[cfg(feature = "exif")]
unsafe impl Sync for ExifData {}
#[cfg(feature = "exif")]
unsafe impl Send for ExifData {}

#[cfg(feature = "exif")]
impl std::fmt::Debug for ExifData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExifData")
            .field(
                "fields",
                &self
                    .0
                    .fields()
                    .map(|field| field.display_value().to_string())
                    .collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "exif")]
impl ExifData {
    pub fn parse(buffer: &[u8]) -> crate::Result<Self> {
        let cursor = std::io::Cursor::new(buffer);
        let mut bufreader = std::io::BufReader::new(cursor);

        let exifreader = exif::Reader::new();
        let exif = exifreader.read_from_container(&mut bufreader)?;

        Ok(ExifData(exif))
    }
}
