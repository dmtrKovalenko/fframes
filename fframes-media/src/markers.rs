//! Markers: images in a frame that stand for something a rendering backend draws itself,
//! for example a shader or a GPU texture. The backend resolves a marker to its value.
//!
//! A marker is a 1x1 transparent image with a unique id. A renderer that does not know
//! the marker draws nothing. A cache keyed by the id never returns an old marker.

use std::borrow::Cow;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use crate::{ImageData, ImageMetadata};

static PIXEL: [u8; 4] = [0; 4];

/// The markers of one kind of value: the live ones of the frames in flight, a few dozen at
/// most, each with the image that stands for it.
pub struct Markers<T> {
    prefix: &'static str,
    next: AtomicU64,
    entries: Mutex<Vec<(Weak<usvgr::PreloadedImageData>, Arc<T>)>>,
}

impl<T> Markers<T> {
    pub const fn new(prefix: &'static str) -> Self {
        Self {
            prefix,
            next: AtomicU64::new(0),
            entries: Mutex::new(Vec::new()),
        }
    }

    /// A marker that resolves to `value`. `width` and `height` are what the frame reports
    /// for the image.
    pub fn create(&self, value: T, width: u32, height: u32) -> ImageData<'static> {
        let image = self.image();
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        // a value lives as long as its image: drop the ones whose frames are done
        entries.retain(|(image, _)| image.strong_count() > 0);
        entries.push((Arc::downgrade(&image), Arc::new(value)));
        Self::image_data(image, width, height)
    }

    /// A marker that resolves to nothing: an image that draws nothing.
    pub fn empty(&self, width: u32, height: u32) -> ImageData<'static> {
        Self::image_data(self.image(), width, height)
    }

    /// The value of `image` if it is one of these markers.
    pub fn resolve(&self, image: &Arc<usvgr::PreloadedImageData>) -> Option<Arc<T>> {
        if !image.id.starts_with(self.prefix) {
            return None;
        }
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries
            .iter()
            .find(|(weak, _)| weak.as_ptr() == Arc::as_ptr(image))
            .map(|(_, value)| Arc::clone(value))
    }

    /// Drops the values for which `keep` is false.
    pub fn retain(&self, keep: impl Fn(&T) -> bool) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.retain(|(_, value)| keep(value));
    }

    fn image(&self) -> Arc<usvgr::PreloadedImageData> {
        Arc::new(usvgr::PreloadedImageData {
            data: Cow::Borrowed(&PIXEL),
            width: 1,
            height: 1,
            id: format!(
                "{}{}",
                self.prefix,
                self.next.fetch_add(1, Ordering::Relaxed)
            ),
        })
    }

    fn image_data(
        image: Arc<usvgr::PreloadedImageData>,
        width: u32,
        height: u32,
    ) -> ImageData<'static> {
        let id = image.id.clone();
        ImageData::new_from_raw_data(image, id, ImageMetadata { width, height })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_resolve_while_their_image_lives() {
        static MARKERS: Markers<u32> = Markers::new("test:");
        let marker = MARKERS.create(7, 4, 4);
        let image = marker.href();
        assert!(image.id.starts_with("test:"));
        assert_eq!(MARKERS.resolve(&image).as_deref(), Some(&7));

        let plain = Arc::new(usvgr::PreloadedImageData {
            data: Cow::Owned(vec![255; 4]),
            width: 1,
            height: 1,
            id: "photo.png".to_owned(),
        });
        assert!(MARKERS.resolve(&plain).is_none());

        drop(marker);
        drop(image);
        // the next marker sweeps the dead one
        let _next = MARKERS.create(8, 1, 1);
        assert_eq!(MARKERS.entries.lock().unwrap().len(), 1);
    }
}
