//! Svgr trees rendered into images while a frame renders, see
//! [`FFramesContext::render_svgr`](crate::FFramesContext::render_svgr).
//!
//! A rendering backend gives an [`SvgrRasterizer`]
//! ([`FFramesRenderBackend::svgr_rasterizer`](crate::FFramesRenderBackend::svgr_rasterizer)).
//! The render puts it into an [`SvgrImages`] and enters it on each thread that calls
//! `Video::render_frame` ([`SvgrImages::enter`]). `render_svgr` uses the images that the
//! thread entered.

use std::cell::Cell;
use std::num::NonZeroUsize;
use std::sync::{Mutex, PoisonError};

use crate::media::ImageData;
pub use crate::media::{image_pixels, rendered_image};
use crate::{Svgr, usvgr};

/// The number of images of static trees that one render keeps.
const STATIC_IMAGES: NonZeroUsize = NonZeroUsize::new(64).unwrap();

/// Renders converted svgr trees into images for
/// [`FFramesContext::render_svgr`](crate::FFramesContext::render_svgr). A backend shares
/// one rasterizer between all threads of a render.
pub trait SvgrRasterizer: Sync {
    /// Renders `tree` scaled to `width` x `height` pixels over a transparent background.
    /// Returns premultiplied RGBA pixels ([`rendered_image`]) or a texture on the GPU of
    /// the backend (`fframes::wgpu_layer`). `None` when the backend cannot render it.
    fn rasterize(&self, tree: &usvgr::Tree, width: u32, height: u32) -> Option<ImageData<'static>>;
}

/// The svgr images of one render: the rasterizer of the backend and the images of static
/// trees, which render once.
pub struct SvgrImages<'a> {
    rasterizer: Option<Box<dyn SvgrRasterizer + 'a>>,
    static_images: Mutex<lru::LruCache<(u64, u32, u32), ImageData<'static>>>,
}

/// What [`SvgrImages::enter`] makes available to the thread.
struct Scope<'s> {
    images: &'s SvgrImages<'s>,
    options: &'s usvgr::Options<'s>,
    font_db: &'s usvgr::fontdb::Database,
}

thread_local! {
    /// The [`Scope`] of the innermost [`SvgrImages::enter`] on this thread, or null.
    static ACTIVE: Cell<*const ()> = const { Cell::new(std::ptr::null()) };
}

impl<'a> SvgrImages<'a> {
    /// `rasterizer` is what [`FFramesRenderBackend::svgr_rasterizer`](crate::FFramesRenderBackend::svgr_rasterizer)
    /// returns. Without one `render_svgr` returns `None`.
    pub fn new(rasterizer: Option<Box<dyn SvgrRasterizer + 'a>>) -> Self {
        Self {
            rasterizer,
            static_images: Mutex::new(lru::LruCache::new(STATIC_IMAGES)),
        }
    }

    /// Runs `f` with these images as the images of `render_svgr` on this thread. Call
    /// `Video::render_frame` in `f`. `options` and `font_db` convert the trees. Give the
    /// ones that convert the frames.
    pub fn enter<R>(
        &self,
        options: &usvgr::Options,
        font_db: &usvgr::fontdb::Database,
        f: impl FnOnce() -> R,
    ) -> R {
        /// Puts the previous scope back, also when `f` panics.
        struct Restore(*const ());
        impl Drop for Restore {
            fn drop(&mut self) {
                ACTIVE.set(self.0);
            }
        }

        let scope = Scope {
            images: self,
            options,
            font_db,
        };
        let _restore = Restore(ACTIVE.replace(std::ptr::from_ref(&scope).cast()));
        f()
    }
}

impl Scope<'_> {
    fn render(&self, svgr: Svgr<'_>, width: u32, height: u32) -> Option<ImageData<'static>> {
        let images = self.images;
        let rasterizer = images.rasterizer.as_deref()?;
        let key = static_hash(&svgr).map(|hash| (hash, width, height));
        let cached = |key| {
            images
                .static_images
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&key)
                .cloned()
        };
        if let Some(image) = key.and_then(cached) {
            return Some(image);
        }

        let tree = svgr
            .into_svg_tree(self.options, &mut usvgr::Cache::default(), self.font_db)
            .inspect_err(|e| log::warn!("fframes: render_svgr can not convert the tree: {e}"))
            .ok()?;
        let image = rasterizer.rasterize(&tree, width, height)?;
        if let Some(key) = key {
            images
                .static_images
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .put(key, image.clone());
        }
        Some(image)
    }
}

impl std::fmt::Debug for SvgrImages<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SvgrImages").finish_non_exhaustive()
    }
}

/// Renders `svgr` with the images that this thread entered. `None` when it entered none.
pub(crate) fn render_svgr(svgr: Svgr<'_>, width: u32, height: u32) -> Option<ImageData<'static>> {
    if width == 0 || height == 0 {
        return None;
    }
    let scope = ACTIVE.get();
    if scope.is_null() {
        return None;
    }
    // SAFETY: `SvgrImages::enter` sets the pointer to a scope on its stack and puts the
    // previous value back before that scope ends. The reference does not leave this call.
    let scope = unsafe { &*scope.cast::<Scope<'_>>() };
    scope.render(svgr, width, height)
}

/// The hash of `svgr` when its whole tree is static (has no `{}`). The same hash means the
/// same tree.
fn static_hash(svgr: &Svgr<'_>) -> Option<u64> {
    #[cfg(feature = "compile-time-svgtree")]
    if let [Some(root)] = svgr.svg_tree.nodes.as_slice() {
        return root.static_hash;
    }
    let _ = svgr;
    None
}
