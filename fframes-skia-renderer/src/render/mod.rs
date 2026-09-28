//! Direct usvgr::Tree to Skia Canvas renderer.
//!
//! This module bypasses Skia's SVG DOM entirely, walking the usvgr tree
//! and issuing Skia Canvas draw calls directly. This eliminates the
//! serialize-to-string + reparse overhead that the SVG DOM path requires.
//!
//! A [`RenderCache`] is used to avoid rebuilding Skia paths, paints, images
//! and whole recorded pictures from scratch on every frame.

mod convert;
mod filters;
mod fingerprint;
mod image;
mod shader;

pub use shader::compile_shader;

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::Arc;

use fframes::usvgr;
use skia_safe::{Canvas, Matrix};

use convert::{convert_blend_mode, convert_path, to_skia_paint, to_skia_stroke_paint};
use fingerprint::{
    exact_content_fingerprint, fill_fingerprint, group_fingerprint, image_fingerprint,
    path_geometry_fingerprint, stroke_fingerprint,
};
use image::SkiaImage;

/// A cached Skia object together with the fingerprint of the resolved
/// `usvgr` state it was built from.  See [`fingerprint`] for why the
/// `static_hash` key alone is not enough to prove an entry is still valid.
struct Cached<T> {
    fingerprint: u64,
    value: T,
}

/// Two-generation map: entries used during the current frame live in
/// `current`; at the start of a frame the previous `current` becomes
/// `previous`, and whatever was still in `previous` (not used for a whole
/// frame) is dropped.  Truly static entries are touched every frame and keep
/// getting stolen back into `current`, so the cache is bounded by roughly two
/// frames worth of entries.
struct Generational<T> {
    current: HashMap<u64, Cached<T>>,
    previous: HashMap<u64, Cached<T>>,
}

impl<T> Default for Generational<T> {
    fn default() -> Self {
        Self {
            current: HashMap::new(),
            previous: HashMap::new(),
        }
    }
}

impl<T> Generational<T> {
    fn begin_frame(&mut self) {
        std::mem::swap(&mut self.current, &mut self.previous);
        self.current.clear();
    }

    /// Move `key` from the previous generation into the current one, so an
    /// entry used this frame survives the next `begin_frame`.
    fn promote(&mut self, key: u64) {
        if !self.current.contains_key(&key)
            && let Some(entry) = self.previous.remove(&key)
        {
            self.current.insert(key, entry);
        }
    }

    /// Look up `key` in both generations.  An entry whose fingerprint does not
    /// match the node being rendered is stale and is treated as a miss (and
    /// replaced by the following `insert`).
    fn get(&mut self, key: u64, fingerprint: u64) -> Option<&T> {
        self.promote(key);
        self.current
            .get(&key)
            .filter(|entry| entry.fingerprint == fingerprint)
            .map(|entry| &entry.value)
    }

    fn insert(&mut self, key: u64, fingerprint: u64, value: T) -> &T {
        &self
            .current
            .entry(key)
            .insert_entry(Cached { fingerprint, value })
            .into_mut()
            .value
    }

    /// `get`, building and inserting the value when it is missing or stale.
    /// Returns `None` only when `build` does.
    fn try_get_or_insert_with(
        &mut self,
        key: u64,
        fingerprint: u64,
        build: impl FnOnce() -> Option<T>,
    ) -> Option<&T> {
        self.promote(key);
        let entry = match self.current.entry(key) {
            Entry::Occupied(entry) if entry.get().fingerprint == fingerprint => entry,
            entry => entry.insert_entry(Cached {
                fingerprint,
                value: build()?,
            }),
        };
        Some(&entry.into_mut().value)
    }
}

/// Caches expensive Skia objects across frames so they are not rebuilt every
/// time `render_tree` is called.
///
/// Paths, paints and pictures are keyed by `static_hash` — a stable
/// content-identity hash assigned at compile time by the `svgr!` macro to
/// nodes whose lexical content never changes.  Nodes without a `static_hash`
/// are frame-dependent and are converted fresh each frame.  Images are keyed
/// by the address of their `Arc<PreloadedImageData>`; the cache entry holds
/// a clone of the `Arc`, so the address can not be recycled while the entry
/// exists.  Every entry is validated with a runtime [`fingerprint`] of the
/// resolved node before reuse and evicted after two frames without a hit
/// (see [`Generational`]).
#[derive(Default)]
pub struct RenderCache {
    /// `static_hash` → converted `skia_safe::Path`
    paths: Generational<skia_safe::Path>,
    /// `static_hash` → recorded `skia_safe::Picture` of an entire static
    /// group.  Replaying a picture is dramatically cheaper than re-traversing
    /// the subtree and re-issuing every draw call.
    pictures: Generational<skia_safe::Picture>,
    /// `static_hash` → fill `Paint` (avoids recreating gradient shaders per frame)
    fill_paints: Generational<skia_safe::Paint>,
    /// `static_hash` → stroke `Paint`
    stroke_paints: Generational<skia_safe::Paint>,
    /// `Arc<PreloadedImageData>` address → Skia image viewing its pixels.
    /// Video frames come as a fresh `Arc` every frame and simply age out.
    images: Generational<SkiaImage>,
    /// Compiled `fframes::Shader` programs, see [`shader`].
    shaders: shader::ShaderCache,
    /// Groups with filters rendered into GPU images, see [`render_cached_filtered_layer`].
    filtered_layers: Generational<FilteredLayer>,
    /// `static_hash` → whether the static subtree contains filters, such subtrees are
    /// not recorded as pictures so the filtered groups inside can be rasterized.
    static_has_filters: HashMap<u64, bool>,
}

struct FilteredLayer {
    image: skia_safe::Image,
    /// Device position of the image's top left corner.
    origin: skia_safe::IPoint,
    /// Translation of the device matrix the layer was rendered with.
    translation: (f32, f32),
}

impl RenderCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn begin_frame(&mut self) {
        self.paths.begin_frame();
        self.pictures.begin_frame();
        self.fill_paints.begin_frame();
        self.stroke_paints.begin_frame();
        self.images.begin_frame();
        self.filtered_layers.begin_frame();
    }

    fn static_has_filters(&mut self, hash: u64, group: &usvgr::Group) -> bool {
        *self
            .static_has_filters
            .entry(hash)
            .or_insert_with(|| subtree_has_filters(group))
    }

    /// Look up or create the Skia image for a `PreloadedImageData`.
    fn image(&mut self, pixels: &Arc<usvgr::PreloadedImageData>) -> Option<&SkiaImage> {
        self.images.try_get_or_insert_with(
            Arc::as_ptr(pixels) as u64,
            image_fingerprint(pixels),
            || SkiaImage::new(pixels),
        )
    }

    /// Convert a path, reusing the cached conversion for static paths.
    fn convert_path(&mut self, path: &usvgr::Path) -> skia_safe::Path {
        let Some(hash) = path.static_hash() else {
            return convert_path(path.data());
        };

        let fingerprint = path_geometry_fingerprint(path.data());
        if let Some(cached) = self.paths.get(hash, fingerprint) {
            return cached.clone();
        }

        self.paths
            .insert(hash, fingerprint, convert_path(path.data()))
            .clone()
    }
}

/// Render a `usvgr::Tree` directly onto a Skia `Canvas`.
///
/// Pass a `RenderCache` that persists across frames to get cross-frame caching of
/// Skia paths and image assets.
pub fn render_tree(tree: &usvgr::Tree, canvas: &Canvas, cache: &mut RenderCache) {
    cache.begin_frame();

    let ts = tree.view_box().to_transform(tree.size());

    canvas.save();
    canvas.concat(&to_matrix(ts));
    render_nodes(tree.root(), canvas, cache);
    canvas.restore();
}

fn render_nodes(parent: &usvgr::Group, canvas: &Canvas, cache: &mut RenderCache) {
    for node in parent.children() {
        render_node(node, canvas, cache);
    }
}

fn render_node(node: &usvgr::Node, canvas: &Canvas, cache: &mut RenderCache) {
    match node {
        usvgr::Node::Group(group) => {
            render_group(group, canvas, cache);
        }
        usvgr::Node::Path(path) => {
            render_path(path, canvas, cache);
        }
        usvgr::Node::Image(image) => {
            render_image(image, canvas, cache);
        }
        usvgr::Node::Text(text) => {
            // Text is pre-flattened to paths by usvgr
            render_group(text.flattened(), canvas, cache);
        }
    }
}

fn render_group(group: &usvgr::Group, canvas: &Canvas, cache: &mut RenderCache) {
    // Static groups are recorded into a Picture once and replayed on later
    // frames, skipping the subtree traversal, paint creation, filter chain
    // building and every individual draw call.
    let static_group = group
        .static_hash()
        .filter(|hash| !cache.static_has_filters(*hash, group))
        .map(|hash| (hash, group_fingerprint(group)));

    canvas.save();
    canvas.concat(&to_matrix(group.transform()));

    if let Some(picture) = static_group.and_then(|(hash, fp)| cache.pictures.get(hash, fp)) {
        canvas.draw_picture(picture, None, None);
        canvas.restore();
        return;
    }

    if let Some((hash, fingerprint)) = static_group {
        // First encounter of this static group: record all child draw commands
        // into a Picture so subsequent frames can replay them in a single call.
        let bbox = group.layer_bounding_box();
        let bounds = skia_safe::Rect::from_xywh(bbox.x(), bbox.y(), bbox.width(), bbox.height());

        let mut recorder = skia_safe::PictureRecorder::new();
        let rec_canvas = recorder.begin_recording(bounds, false);

        if group.should_isolate() {
            render_isolated_group(group, rec_canvas, cache);
        } else {
            render_nodes(group, rec_canvas, cache);
        }

        if let Some(picture) = recorder.finish_recording_as_picture(Some(&bounds)) {
            canvas.draw_picture(&picture, None, None);
            cache.pictures.insert(hash, fingerprint, picture);
        }
    } else if group.should_isolate() {
        render_isolated_group(group, canvas, cache);
    } else {
        render_nodes(group, canvas, cache);
    }

    canvas.restore();
}

/// Render an isolated group with opacity, blend mode, filters, clip-path, and/or mask.
fn render_isolated_group(group: &usvgr::Group, canvas: &Canvas, cache: &mut RenderCache) {
    let has_clip = group.clip_path().is_some();
    let has_mask = group.mask().is_some();
    let has_filters = !group.filters().is_empty();
    let has_opacity = group.opacity().get() < 1.0;
    let has_blend = group.blend_mode() != usvgr::BlendMode::Normal;

    // Step 1: Apply clip path first (restricts the drawing area)
    if let Some(clip_path) = group.clip_path() {
        canvas.save();
        apply_clip_path(clip_path, canvas, cache);
    }

    // An opacity group around a single shape or image that can not overlap itself looks
    // the same when the shape is drawn with the opacity applied to its paint. That skips
    // an offscreen layer, which on the GPU is a separate render pass.
    if has_opacity
        && !has_filters
        && !has_mask
        && !has_blend
        && !group.isolate()
        && render_folded_opacity(group, canvas, cache, group.opacity().get())
    {
        if has_clip {
            canvas.restore();
        }
        return;
    }

    if has_filters && render_cached_filtered_layer(group, canvas, cache) {
        if has_clip {
            canvas.restore();
        }
        return;
    }

    // Step 2: Create an isolation layer for opacity, blend, filters, or an
    // explicit `isolation: isolate` (which confines children's
    // mix-blend-mode to the group's own backdrop).  Clip-only groups skip
    // the layer — it would not change their output.
    if has_filters || has_opacity || has_blend || has_mask || group.isolate() {
        let mut layer_paint = skia_safe::Paint::default();
        layer_paint.set_alpha_f(group.opacity().get());
        layer_paint.set_blend_mode(convert_blend_mode(group.blend_mode()));

        let filter = if has_filters {
            filters::build_filter_chain(group.filters(), cache)
        } else {
            None
        };

        let bbox = group.layer_bounding_box();
        let layer_bounds =
            skia_safe::Rect::from_xywh(bbox.x(), bbox.y(), bbox.width(), bbox.height());

        // SVG rendering order is filter -> clip -> mask -> opacity.  When a
        // mask is present the filter goes into an inner layer so the mask
        // (DstIn in the outer layer) operates on the *filtered* output, not
        // the raw content.  Without a mask the filter rides on the outer
        // layer's paint directly.
        let inner_filter = if has_mask {
            filter
        } else {
            if let Some(filter) = filter {
                layer_paint.set_image_filter(filter);
            }
            None
        };

        canvas.save_layer(
            &skia_safe::canvas::SaveLayerRec::default()
                .paint(&layer_paint)
                .bounds(&layer_bounds),
        );

        if let Some(filter) = inner_filter {
            let mut filter_paint = skia_safe::Paint::default();
            filter_paint.set_image_filter(filter);
            canvas.save_layer(
                &skia_safe::canvas::SaveLayerRec::default()
                    .paint(&filter_paint)
                    .bounds(&layer_bounds),
            );
            render_nodes(group, canvas, cache);
            canvas.restore();
        } else {
            render_nodes(group, canvas, cache);
        }

        if let Some(mask) = group.mask() {
            apply_mask(mask, canvas, cache);
        }

        canvas.restore();
    } else {
        render_nodes(group, canvas, cache);
    }

    if has_clip {
        canvas.restore();
    }
}

fn subtree_has_filters(group: &usvgr::Group) -> bool {
    !group.filters().is_empty()
        || group.children().iter().any(|node| match node {
            usvgr::Node::Group(group) => subtree_has_filters(group),
            _ => false,
        })
}

/// Draws a group with filters from its rasterized layer, rendering and caching the layer
/// first when needed. Filters run on the GPU every time they are drawn (also when a
/// recorded picture is replayed), a cached layer is a single textured draw.
///
/// The same content moved by whole pixels reuses the layer. Content that is only blurred
/// is smooth enough to reuse at any sub-pixel offset as well, which covers animated glows
/// and bokeh. Returns false when the group can not be cached (its content can not be
/// fingerprinted exactly or the canvas has no surface, e.g. while recording a picture).
fn render_cached_filtered_layer(
    group: &usvgr::Group,
    canvas: &Canvas,
    cache: &mut RenderCache,
) -> bool {
    use std::hash::{Hash, Hasher};

    let matrix = canvas.local_to_device_as_3x3();
    if matrix.has_perspective() {
        return false;
    }

    let content = match group.static_hash() {
        Some(hash) => (hash, group_fingerprint(group)),
        None => match exact_content_fingerprint(group) {
            Some(fingerprint) => (0, fingerprint),
            None => return false,
        },
    };

    let bbox = group.layer_bounding_box();
    let local_bounds = skia_safe::Rect::from_xywh(bbox.x(), bbox.y(), bbox.width(), bbox.height());
    let device_bounds = matrix.map_rect(local_bounds).0;
    let screen = {
        let size = canvas.base_layer_size();
        skia_safe::Rect::from_iwh(size.width, size.height)
    };
    // a layer that is partly off screen is cut to the screen, it only fits its position
    let on_screen = skia_safe::Contains::contains(&screen, device_bounds);
    let smooth = on_screen && group.mask().is_none() && only_blurs(group, &matrix);

    let translation = (matrix.translate_x(), matrix.translate_y());
    let key = {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        content.hash(&mut hasher);
        for value in [
            matrix.scale_x(),
            matrix.skew_x(),
            matrix.skew_y(),
            matrix.scale_y(),
        ] {
            value.to_bits().hash(&mut hasher);
        }
        if !on_screen {
            (translation.0.to_bits(), translation.1.to_bits()).hash(&mut hasher);
        } else if !smooth {
            (
                translation.0.fract().to_bits(),
                translation.1.fract().to_bits(),
            )
                .hash(&mut hasher);
        }
        hasher.finish()
    };

    if cache.filtered_layers.get(key, key).is_none() {
        let Some(layer) = rasterize_filtered_layer(group, canvas, &matrix, local_bounds, cache)
        else {
            return false;
        };
        cache.filtered_layers.insert(key, key, layer);
    }
    let Some(layer) = cache.filtered_layers.get(key, key) else {
        return false;
    };

    let mut paint = skia_safe::Paint::default();
    paint.set_alpha_f(group.opacity().get());
    paint.set_blend_mode(convert_blend_mode(group.blend_mode()));

    let left = layer.origin.x as f32 + (translation.0 - layer.translation.0);
    let top = layer.origin.y as f32 + (translation.1 - layer.translation.1);
    let sampling = if left.fract() == 0.0 && top.fract() == 0.0 {
        skia_safe::SamplingOptions::default()
    } else {
        skia_safe::SamplingOptions::new(skia_safe::FilterMode::Linear, skia_safe::MipmapMode::None)
    };

    canvas.save();
    canvas.reset_matrix();
    canvas.draw_image_with_sampling_options(&layer.image, (left, top), sampling, Some(&paint));
    canvas.restore();
    true
}

/// Every filter of the group only blurs, by at least 1.5 device pixels.
fn only_blurs(group: &usvgr::Group, matrix: &Matrix) -> bool {
    let scale = matrix
        .scale_x()
        .hypot(matrix.skew_y())
        .min(matrix.skew_x().hypot(matrix.scale_y()));

    !group.filters().is_empty()
        && group.filters().iter().all(|filter| {
            filter
                .primitives()
                .iter()
                .all(|primitive| match primitive.kind() {
                    usvgr::filter::Kind::GaussianBlur(blur) => {
                        blur.std_dev_x().get().min(blur.std_dev_y().get()) * scale >= 1.5
                    }
                    _ => false,
                })
        })
}

/// Renders the isolated layer of `group` (children, filters and mask, without the group's
/// opacity and blend mode, which apply when the layer is drawn) into its own GPU image.
fn rasterize_filtered_layer(
    group: &usvgr::Group,
    canvas: &Canvas,
    matrix: &Matrix,
    local_bounds: skia_safe::Rect,
    cache: &mut RenderCache,
) -> Option<FilteredLayer> {
    let device_size = canvas.base_layer_size();
    let device_bounds: skia_safe::IRect =
        skia_safe::RoundOut::round_out(&matrix.map_rect(local_bounds).0);
    let bounds = skia_safe::IRect::intersect(
        &device_bounds,
        &skia_safe::IRect::from_wh(device_size.width, device_size.height),
    )?;

    let info = canvas
        .image_info()
        .with_dimensions((bounds.width(), bounds.height()));
    let mut surface = canvas.new_surface(&info, None)?;
    let layer_canvas = surface.canvas();
    layer_canvas.clear(skia_safe::Color::TRANSPARENT);
    layer_canvas.translate((-bounds.left as f32, -bounds.top as f32));
    layer_canvas.concat(matrix);

    let filter = filters::build_filter_chain(group.filters(), cache);
    let mut filter_paint = skia_safe::Paint::default();
    if let Some(filter) = filter {
        filter_paint.set_image_filter(filter);
    }
    layer_canvas.save_layer(
        &skia_safe::canvas::SaveLayerRec::default()
            .paint(&filter_paint)
            .bounds(&local_bounds),
    );
    render_nodes(group, layer_canvas, cache);
    layer_canvas.restore();

    if let Some(mask) = group.mask() {
        apply_mask(mask, layer_canvas, cache);
    }

    Some(FilteredLayer {
        image: surface.image_snapshot(),
        origin: skia_safe::IPoint::new(bounds.left, bounds.top),
        translation: (matrix.translate_x(), matrix.translate_y()),
    })
}

/// Draws the only child of `group` faded by `alpha` when that looks exactly like the
/// group composited with that opacity, returns false without drawing anything otherwise.
fn render_folded_opacity(
    group: &usvgr::Group,
    canvas: &Canvas,
    cache: &mut RenderCache,
    alpha: f32,
) -> bool {
    let [child] = group.children() else {
        return false;
    };

    match child {
        usvgr::Node::Path(path) if path.fill().is_none() || path.stroke().is_none() => {
            render_path_with_alpha(path, canvas, cache, alpha);
            true
        }
        usvgr::Node::Image(image) => match image.kind() {
            // shader images are drawn by the shader path
            usvgr::ImageKind::DATA(img) if fframes::resolve_shader_draw(img).is_some() => false,
            usvgr::ImageKind::DATA(img) => {
                render_raster_image_with_alpha(image, img, canvas, cache, alpha);
                true
            }
            usvgr::ImageKind::SVG { .. } => false,
        },
        usvgr::Node::Group(inner) if !inner.should_isolate() => {
            canvas.save();
            canvas.concat(&to_matrix(inner.transform()));
            let folded = render_folded_opacity(inner, canvas, cache, alpha);
            canvas.restore();
            folded
        }
        _ => false,
    }
}

/// Build a tiling Skia shader from an SVG `<pattern>` paint server.
///
/// The pattern content is recorded into a `Picture` once per use and replayed
/// as a picture shader.  `usvgr` resolves `patternUnits`/`patternContentUnits`
/// to user space at parse time, so `rect()`, `transform()` and the content are
/// already in user coordinates here.
pub(super) fn render_pattern_shader(
    pattern: &usvgr::Pattern,
    cache: &mut RenderCache,
) -> Option<skia_safe::Shader> {
    let rect = pattern.rect();
    let tile = skia_safe::Rect::from_wh(rect.width(), rect.height());

    let mut recorder = skia_safe::PictureRecorder::new();
    let rec_canvas = recorder.begin_recording(tile, false);

    if let Some(view_box) = pattern.view_box() {
        let viewport = usvgr::Size::from_wh(rect.width(), rect.height())?;
        rec_canvas.concat(&to_matrix(view_box.to_transform(viewport)));
    }
    render_nodes(pattern.root(), rec_canvas, cache);
    let picture = recorder.finish_recording_as_picture(Some(&tile))?;

    // Pattern tiles are placed starting at rect's origin, then transformed by
    // patternTransform.
    let mut local_matrix = to_matrix(pattern.transform());
    local_matrix.pre_translate((rect.x(), rect.y()));

    Some(picture.to_shader(
        Some((skia_safe::TileMode::Repeat, skia_safe::TileMode::Repeat)),
        skia_safe::FilterMode::Linear,
        Some(&local_matrix),
        Some(&tile),
    ))
}

fn apply_clip_path(clip: &usvgr::ClipPath, canvas: &Canvas, cache: &mut RenderCache) {
    match build_clip_path(clip, cache) {
        Some(path) => {
            canvas.clip_path(&path, skia_safe::ClipOp::Intersect, true);
        }
        None => {
            // A clip path with no usable geometry clips everything away.
            canvas.clip_rect(
                skia_safe::Rect::new_empty(),
                skia_safe::ClipOp::Intersect,
                false,
            );
        }
    }
}

/// Resolve a clipPath into a single Skia path: the *union* of its children's
/// geometry, intersected with the clipPath's own nested clip-path.
///
/// SVG composes sibling clip shapes additively, so they cannot be applied as
/// sequential canvas clips (which intersect).  Returns None when no geometry
/// contributes — per spec that hides the clipped element entirely.
fn build_clip_path(clip: &usvgr::ClipPath, cache: &mut RenderCache) -> Option<skia_safe::Path> {
    let result = build_clip_group(clip.root(), &to_matrix(clip.transform()), cache)?;

    if let Some(nested) = clip.clip_path() {
        let nested_path = build_clip_path(nested, cache)?;
        return result.op(&nested_path, skia_safe::PathOp::Intersect);
    }

    Some(result)
}

/// Union of the clip geometry contributed by a group's children.
fn build_clip_group(
    group: &usvgr::Group,
    transform: &Matrix,
    cache: &mut RenderCache,
) -> Option<skia_safe::Path> {
    let mut result: Option<skia_safe::Path> = None;

    for child in group.children() {
        let contribution = match child {
            usvgr::Node::Path(path) => {
                if path.visibility() != usvgr::Visibility::Visible {
                    continue;
                }
                let fill_type = path
                    .fill()
                    .map(|f| match f.rule() {
                        usvgr::FillRule::NonZero => skia_safe::PathFillType::Winding,
                        usvgr::FillRule::EvenOdd => skia_safe::PathFillType::EvenOdd,
                    })
                    .unwrap_or(skia_safe::PathFillType::Winding);

                let mut sk_path = cache.convert_path(path);
                sk_path.set_fill_type(fill_type);
                Some(sk_path.make_transform(transform))
            }
            usvgr::Node::Text(text) => build_clip_group(text.flattened(), transform, cache),
            usvgr::Node::Group(child_group) => {
                let mut combined = *transform;
                combined.pre_concat(&to_matrix(child_group.transform()));
                let sub = build_clip_group(child_group, &combined, cache);
                // A clip-path on a clip child intersects that child's
                // contribution before it joins the union.
                match (sub, child_group.clip_path()) {
                    (Some(sub), Some(nested)) => build_clip_path(nested, cache)
                        .and_then(|nested| sub.op(&nested, skia_safe::PathOp::Intersect)),
                    (sub, None) => sub,
                    (None, _) => None,
                }
            }
            _ => None,
        };

        result = match (result, contribution) {
            (None, contribution) => contribution,
            (result, None) => result,
            (Some(result), Some(contribution)) => {
                result.op(&contribution, skia_safe::PathOp::Union)
            }
        };
    }

    result
}

/// Apply a mask to the current layer content using DstIn blending.
fn apply_mask(mask: &usvgr::Mask, canvas: &Canvas, cache: &mut RenderCache) {
    let mut mask_paint = skia_safe::Paint::default();
    mask_paint.set_blend_mode(skia_safe::BlendMode::DstIn);

    let mask_rect = mask.rect();
    let bounds = skia_safe::Rect::from_xywh(
        mask_rect.x(),
        mask_rect.y(),
        mask_rect.width(),
        mask_rect.height(),
    );

    // The DstIn layer has to cover everything the group drew, since on
    // restore only the pixels under the layer are multiplied by the mask.
    // Skia expands a bounded layer to the clip for blend modes that affect
    // transparent pixels anyway, so no bounds hint is given here.
    canvas.save_layer(&skia_safe::canvas::SaveLayerRec::default().paint(&mask_paint));
    // The mask region (x/y/width/height) hard-clips the mask content.
    canvas.clip_rect(bounds, skia_safe::ClipOp::Intersect, true);

    if mask.kind() == usvgr::MaskType::Luminance {
        let luma_cf = skia_safe::ColorFilter::luma();
        let mut luma_paint = skia_safe::Paint::default();
        luma_paint.set_color_filter(luma_cf);

        canvas.save_layer(
            &skia_safe::canvas::SaveLayerRec::default()
                .paint(&luma_paint)
                .bounds(&bounds),
        );
        render_nodes(mask.root(), canvas, cache);
        canvas.restore();
    } else {
        render_nodes(mask.root(), canvas, cache);
    }

    canvas.restore();

    if let Some(nested_mask) = mask.mask() {
        apply_mask(nested_mask, canvas, cache);
    }
}

fn render_path(path: &usvgr::Path, canvas: &Canvas, cache: &mut RenderCache) {
    render_path_with_alpha(path, canvas, cache, 1.0);
}

/// Renders a path with its paints faded by `alpha`, which is how an opacity group
/// holding only this path looks when the path does not overlap itself.
fn render_path_with_alpha(
    path: &usvgr::Path,
    canvas: &Canvas,
    cache: &mut RenderCache,
    alpha: f32,
) {
    if path.visibility() != usvgr::Visibility::Visible {
        return;
    }

    if path.paint_order() == usvgr::PaintOrder::FillAndStroke {
        fill_path(path, canvas, cache, alpha);
        stroke_path(path, canvas, cache, alpha);
    } else {
        stroke_path(path, canvas, cache, alpha);
        fill_path(path, canvas, cache, alpha);
    }
}

fn draw_path_with_alpha(
    canvas: &Canvas,
    path: &skia_safe::Path,
    paint: &skia_safe::Paint,
    alpha: f32,
) {
    if alpha < 1.0 {
        let mut paint = paint.clone();
        paint.set_alpha_f(paint.alpha_f() * alpha);
        canvas.draw_path(path, &paint);
    } else {
        canvas.draw_path(path, paint);
    }
}

fn fill_path(path: &usvgr::Path, canvas: &Canvas, cache: &mut RenderCache, alpha: f32) {
    let Some(fill) = path.fill() else { return };

    let bounds = path.data().bounds();
    if bounds.width() == 0.0 || bounds.height() == 0.0 {
        return;
    }

    let fill_type = match fill.rule() {
        usvgr::FillRule::NonZero => skia_safe::PathFillType::Winding,
        usvgr::FillRule::EvenOdd => skia_safe::PathFillType::EvenOdd,
    };

    let mut sk_path = cache.convert_path(path);
    sk_path.set_fill_type(fill_type);

    let anti_alias = path.rendering_mode().use_shape_antialiasing();
    let cache_key = path
        .static_hash()
        .map(|hash| (hash, fill_fingerprint(fill, anti_alias)));

    // Static paths reuse their Paint, which avoids recreating gradient
    // shaders and other expensive paint state every frame.
    if let Some(paint) = cache_key.and_then(|(hash, fp)| cache.fill_paints.get(hash, fp)) {
        draw_path_with_alpha(canvas, &sk_path, paint, alpha);
        return;
    }

    let Some(mut paint) = to_skia_paint(fill.paint(), fill.opacity(), anti_alias, cache) else {
        return;
    };
    paint.set_style(skia_safe::PaintStyle::Fill);

    draw_path_with_alpha(canvas, &sk_path, &paint, alpha);

    if let Some((hash, fingerprint)) = cache_key {
        cache.fill_paints.insert(hash, fingerprint, paint);
    }
}

fn stroke_path(path: &usvgr::Path, canvas: &Canvas, cache: &mut RenderCache, alpha: f32) {
    let Some(stroke) = path.stroke() else { return };

    let sk_path = cache.convert_path(path);

    let anti_alias = path.rendering_mode().use_shape_antialiasing();
    let cache_key = path
        .static_hash()
        .map(|hash| (hash, stroke_fingerprint(stroke, anti_alias)));

    if let Some(paint) = cache_key.and_then(|(hash, fp)| cache.stroke_paints.get(hash, fp)) {
        draw_path_with_alpha(canvas, &sk_path, paint, alpha);
        return;
    }

    let Some(paint) = to_skia_stroke_paint(stroke, anti_alias, cache) else {
        return;
    };

    draw_path_with_alpha(canvas, &sk_path, &paint, alpha);

    if let Some((hash, fingerprint)) = cache_key {
        cache.stroke_paints.insert(hash, fingerprint, paint);
    }
}

fn render_image(image: &usvgr::Image, canvas: &Canvas, cache: &mut RenderCache) {
    if image.visibility() != usvgr::Visibility::Visible {
        return;
    }

    render_image_kind(
        image.kind(),
        image.view_box(),
        image.rendering_mode(),
        canvas,
        cache,
    );
}

/// Draws a raster image faded by `alpha`, see [`render_path_with_alpha`].
fn render_raster_image_with_alpha(
    image: &usvgr::Image,
    img: &Arc<usvgr::PreloadedImageData>,
    canvas: &Canvas,
    cache: &mut RenderCache,
    alpha: f32,
) {
    if image.visibility() == usvgr::Visibility::Visible {
        render_raster_image(
            img,
            image.view_box(),
            image.rendering_mode(),
            canvas,
            cache,
            alpha,
        );
    }
}

/// Render an image payload (raster or nested SVG) into the `view_box`.
///
/// Shared between `<image>` elements and the `feImage` filter primitive.
pub(super) fn render_image_kind(
    kind: &usvgr::ImageKind,
    view_box: usvgr::ViewBox,
    rendering_mode: usvgr::ImageRendering,
    canvas: &Canvas,
    cache: &mut RenderCache,
) {
    match kind {
        usvgr::ImageKind::DATA(data) => {
            if let Some(draw) = fframes::resolve_shader_draw(data) {
                shader::render_shader(&draw, view_box, canvas, cache);
                return;
            }

            render_raster_image(data, view_box, rendering_mode, canvas, cache, 1.0);
        }
        usvgr::ImageKind::SVG { tree, .. } => {
            render_svg_image(tree, view_box, canvas, cache);
        }
    }
}

fn render_raster_image(
    img: &Arc<usvgr::PreloadedImageData>,
    view_box: usvgr::ViewBox,
    rendering_mode: usvgr::ImageRendering,
    canvas: &Canvas,
    cache: &mut RenderCache,
    alpha: f32,
) {
    let Some(sk_image) = cache.image(img).map(SkiaImage::image) else {
        return;
    };

    let Some(img_size) = usvgr::Size::from_wh(img.width as f32, img.height as f32) else {
        return;
    };

    // Compute the content-to-element transform as a single matrix.
    // ViewBox::to_transform maps from self.rect (content space) to the Size
    // argument (viewport).  For a raster image the content space is the image's
    // intrinsic dimensions at the origin and the viewport is the element's
    // width × height.  The element's (x, y) position is folded in by offsetting
    // the translation component, keeping this to a single canvas.concat() call.
    let vb_rect = view_box.rect;
    let Some(viewport_size) = usvgr::Size::from_wh(vb_rect.width(), vb_rect.height()) else {
        return;
    };
    let content_vb = usvgr::ViewBox {
        rect: img_size.to_non_zero_rect(0.0, 0.0),
        aspect: view_box.aspect,
    };
    let ts = content_vb.to_transform(viewport_size);

    canvas.save();
    // Clip to the element's viewport: with preserveAspectRatio="...slice" the
    // scaled content overflows the element rect and must not leak outside it.
    canvas.clip_rect(
        skia_safe::Rect::from_xywh(vb_rect.x(), vb_rect.y(), vb_rect.width(), vb_rect.height()),
        skia_safe::ClipOp::Intersect,
        true,
    );
    // translate(x, y) * ts — left-multiplying a translate just offsets tx, ty.
    canvas.concat(&to_matrix(usvgr::Transform::from_row(
        ts.sx,
        ts.ky,
        ts.kx,
        ts.sy,
        ts.tx + vb_rect.x(),
        ts.ty + vb_rect.y(),
    )));

    let sampling = match rendering_mode {
        usvgr::ImageRendering::OptimizeQuality => skia_safe::SamplingOptions::new(
            skia_safe::FilterMode::Linear,
            skia_safe::MipmapMode::None,
        ),
        usvgr::ImageRendering::OptimizeSpeed => skia_safe::SamplingOptions::new(
            skia_safe::FilterMode::Nearest,
            skia_safe::MipmapMode::None,
        ),
    };

    let rect = skia_safe::Rect::from_wh(img.width as f32, img.height as f32);
    canvas.draw_image_rect_with_sampling_options(
        sk_image,
        Some((&rect, skia_safe::canvas::SrcRectConstraint::Strict)),
        rect,
        sampling,
        &{
            let mut paint = skia_safe::Paint::default();
            paint.set_alpha_f(alpha);
            paint
        },
    );

    canvas.restore();
}

fn render_svg_image(
    tree: &usvgr::Tree,
    view_box: usvgr::ViewBox,
    canvas: &Canvas,
    cache: &mut RenderCache,
) {
    canvas.save();

    let vb = view_box.rect;
    // Nested SVG content is clipped to the element's viewport.
    canvas.clip_rect(
        skia_safe::Rect::from_xywh(vb.x(), vb.y(), vb.width(), vb.height()),
        skia_safe::ClipOp::Intersect,
        true,
    );
    canvas.translate((vb.x(), vb.y()));

    // Scale the SVG's intrinsic size to the element's rect, honoring
    // preserveAspectRatio, then apply the tree's own viewBox transform.
    if let Some(viewport_size) = usvgr::Size::from_wh(vb.width(), vb.height()) {
        let content_vb = usvgr::ViewBox {
            rect: tree.size().to_non_zero_rect(0.0, 0.0),
            aspect: view_box.aspect,
        };
        canvas.concat(&to_matrix(content_vb.to_transform(viewport_size)));
    }

    let ts = tree.view_box().to_transform(tree.size());
    canvas.concat(&to_matrix(ts));

    render_nodes(tree.root(), canvas, cache);
    canvas.restore();
}

/// Convert a usvgr Transform to a Skia Matrix.
fn to_matrix(ts: usvgr::Transform) -> Matrix {
    Matrix::new_all(ts.sx, ts.kx, ts.tx, ts.ky, ts.sy, ts.ty, 0.0, 0.0, 1.0)
}
