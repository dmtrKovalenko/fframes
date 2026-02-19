//! Direct usvgr::Tree to Skia Canvas renderer.
//!
//! This module bypasses Skia's SVG DOM entirely, walking the usvgr tree
//! and issuing Skia Canvas draw calls directly. This eliminates the
//! serialize-to-string + reparse overhead that the SVG DOM path requires.
//!
//! A [`RenderCache`] is used to avoid rebuilding Skia paths and images from
//! scratch on every frame.  Image assets are pre-registered as
//! [`FFramesSkiaImage`](crate::resource_provider::FFramesSkiaImage) instances
//! (the same type used by the `ResourceProvider` in the old SVG-DOM path) so
//! that the `skia_safe::Image` is created once and reused across frames.

mod convert;
mod filters;

use std::collections::HashMap;
use std::sync::Arc;

use fframes::usvgr;
use fframes::usvgr::tiny_skia_path;
use skia_safe::{Canvas, Matrix};

use crate::resource_provider::FFramesSkiaImage;
use convert::{convert_blend_mode, convert_path, to_skia_paint, to_skia_stroke_paint};

/// Caches expensive Skia objects across frames so they are not rebuilt every
/// time `render_tree` is called.
///
/// - **Paths**: keyed by `static_hash` (a stable content-identity hash set at
///   compile-time by the `svgr!` macro).  Only paths with a `static_hash` are
///   cached — frame-dependent paths (`static_hash == None`) are converted
///   fresh each frame since their geometry changes.
/// - **Images**: keyed by `Arc<PreloadedImageData>` pointer identity (static
///   images only — ephemeral video frames bypass the cache entirely).
///
/// Hash-keyed caches (paths, pictures, paints) use a two-generation eviction
/// scheme: entries from the current frame live in the primary map; at the
/// start of each frame, last frame's primaries become `prev_*` and entries
/// that survive two frames without a hit are dropped.  This bounds memory
/// to roughly 2 frames worth of "stable-but-changing" entries (nodes whose
/// `static_hash` changes every frame because they contain expressions that
/// don't lexically reference `frame`).
pub struct RenderCache {
    /// `static_hash` (u64) → converted `skia_safe::Path`
    paths: HashMap<u64, skia_safe::Path>,
    prev_paths: HashMap<u64, skia_safe::Path>,
    /// `Arc<PreloadedImageData>` raw-pointer → pre-registered `FFramesSkiaImage`
    /// (only static images; ephemeral images are created fresh)
    images: HashMap<usize, FFramesSkiaImage>,
    /// `static_hash` (u64) → recorded `skia_safe::Picture` for entire static groups.
    /// Replaying a Picture is dramatically cheaper than re-traversing and re-drawing
    /// the entire subtree on every frame.
    pictures: HashMap<u64, skia_safe::Picture>,
    prev_pictures: HashMap<u64, skia_safe::Picture>,
    /// `static_hash` (u64) → cached fill `Paint` (avoids recreating gradient shaders per frame)
    fill_paints: HashMap<u64, skia_safe::Paint>,
    prev_fill_paints: HashMap<u64, skia_safe::Paint>,
    /// `static_hash` (u64) → cached stroke `Paint`
    stroke_paints: HashMap<u64, skia_safe::Paint>,
    prev_stroke_paints: HashMap<u64, skia_safe::Paint>,
}

impl RenderCache {
    pub fn new() -> Self {
        Self {
            paths: HashMap::new(),
            prev_paths: HashMap::new(),
            images: HashMap::new(),
            pictures: HashMap::new(),
            prev_pictures: HashMap::new(),
            fill_paints: HashMap::new(),
            prev_fill_paints: HashMap::new(),
            stroke_paints: HashMap::new(),
            prev_stroke_paints: HashMap::new(),
        }
    }

    /// Rotate the generational caches at the start of each frame.
    ///
    /// Current-frame entries become "previous".  Whatever was in "previous"
    /// (entries not accessed during the last frame) is dropped.
    ///
    /// Truly static entries are accessed every frame and are always stolen
    /// back from `prev_*` into the current map on first access.
    /// "Stable-but-changing" entries get a *new* hash each frame so the old
    /// hash is never looked up again — it survives one generation in `prev_*`
    /// and is dropped on the next rotation.
    fn begin_frame(&mut self) {
        std::mem::swap(&mut self.pictures, &mut self.prev_pictures);
        self.pictures.clear();

        std::mem::swap(&mut self.fill_paints, &mut self.prev_fill_paints);
        self.fill_paints.clear();

        std::mem::swap(&mut self.stroke_paints, &mut self.prev_stroke_paints);
        self.stroke_paints.clear();

        std::mem::swap(&mut self.paths, &mut self.prev_paths);
        self.paths.clear();
    }

    /// Look up or create a [`FFramesSkiaImage`] asset from `PreloadedImageData`.
    ///
    /// Uses `Arc` pointer identity as the cache key so that the same `Arc`
    /// (i.e. the same static image) always returns the same pre-built
    /// `skia_safe::Image`.
    fn get_or_create_image_asset(
        &mut self,
        arc_img: &Arc<usvgr::PreloadedImageData>,
    ) -> Option<&FFramesSkiaImage> {
        let key = Arc::as_ptr(arc_img) as usize;
        match self.images.entry(key) {
            std::collections::hash_map::Entry::Occupied(e) => Some(e.into_mut()),
            std::collections::hash_map::Entry::Vacant(e) => {
                let asset = FFramesSkiaImage::new(arc_img)?;
                Some(e.insert(asset))
            }
        }
    }

    /// Look up a cached Picture, checking both current and previous generation.
    /// If found in the previous generation, steals it into the current map.
    fn get_picture(&mut self, hash: u64) -> Option<&skia_safe::Picture> {
        if self.pictures.contains_key(&hash) {
            return self.pictures.get(&hash);
        }
        if let Some(pic) = self.prev_pictures.remove(&hash) {
            self.pictures.insert(hash, pic);
            return self.pictures.get(&hash);
        }
        None
    }

    /// Insert a Picture into the current-frame cache.
    fn insert_picture(&mut self, hash: u64, picture: skia_safe::Picture) {
        self.pictures.insert(hash, picture);
    }

    /// Look up a cached fill paint, checking both generations.
    fn get_fill_paint(&mut self, hash: u64) -> Option<&skia_safe::Paint> {
        if self.fill_paints.contains_key(&hash) {
            return self.fill_paints.get(&hash);
        }
        if let Some(paint) = self.prev_fill_paints.remove(&hash) {
            self.fill_paints.insert(hash, paint);
            return self.fill_paints.get(&hash);
        }
        None
    }

    /// Look up a cached stroke paint, checking both generations.
    fn get_stroke_paint(&mut self, hash: u64) -> Option<&skia_safe::Paint> {
        if self.stroke_paints.contains_key(&hash) {
            return self.stroke_paints.get(&hash);
        }
        if let Some(paint) = self.prev_stroke_paints.remove(&hash) {
            self.stroke_paints.insert(hash, paint);
            return self.stroke_paints.get(&hash);
        }
        None
    }
}

/// Convert a path, using the cache if a `static_hash` is available.
///
/// Paths with a `static_hash` are structurally identical across frames and
/// their converted `skia_safe::Path` can be safely reused.  Paths without a
/// hash are frame-dependent and must be converted every time.
fn cached_convert_path(
    cache: &mut RenderCache,
    static_hash: Option<u64>,
    path_data: &tiny_skia_path::Path,
) -> skia_safe::Path {
    if let Some(hash) = static_hash {
        // Check current frame cache
        if let Some(p) = cache.paths.get(&hash) {
            return p.clone();
        }
        // Steal from previous frame
        if let Some(p) = cache.prev_paths.remove(&hash) {
            cache.paths.insert(hash, p);
            return cache.paths.get(&hash).unwrap().clone();
        }
        // Convert and insert into current frame
        let converted = convert_path(path_data);
        cache.paths.insert(hash, converted);
        cache.paths.get(&hash).unwrap().clone()
    } else {
        convert_path(path_data)
    }
}

/// Render a `usvgr::Tree` directly onto a Skia `Canvas`.
///
/// This is the main entry point replacing the old `tree.to_string()` + `Dom::from_str()` path.
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
    // Fast path: replay a cached Picture for fully-static groups.
    // This skips the entire subtree traversal, paint creation, filter
    // chain building, and all Skia draw calls — replaying a single Picture instead.
    if let Some(hash) = group.static_hash() {
        if let Some(picture) = cache.get_picture(hash) {
            canvas.save();
            canvas.concat(&to_matrix(group.transform()));
            canvas.draw_picture(picture, None, None);
            canvas.restore();
            return;
        }
    }

    canvas.save();
    canvas.concat(&to_matrix(group.transform()));

    if let Some(hash) = group.static_hash() {
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
            cache.insert_picture(hash, picture);
        }
    } else {
        if group.should_isolate() {
            render_isolated_group(group, canvas, cache);
        } else {
            render_nodes(group, canvas, cache);
        }
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

    // Step 2: Create an isolation layer for opacity, blend, and filters
    if has_filters || has_opacity || has_blend || has_mask {
        let mut layer_paint = skia_safe::Paint::default();
        layer_paint.set_alpha_f(group.opacity().get());
        layer_paint.set_blend_mode(convert_blend_mode(group.blend_mode()));

        if has_filters {
            if let Some(filter) = filters::build_filter_chain(group.filters()) {
                layer_paint.set_image_filter(filter);
            }
        }

        let bbox = group.layer_bounding_box();
        let layer_bounds =
            skia_safe::Rect::from_xywh(bbox.x(), bbox.y(), bbox.width(), bbox.height());

        canvas.save_layer(
            &skia_safe::canvas::SaveLayerRec::default()
                .paint(&layer_paint)
                .bounds(&layer_bounds),
        );

        if has_mask {
            render_nodes(group, canvas, cache);
            if let Some(mask) = group.mask() {
                apply_mask(mask, canvas, cache);
            }
        } else {
            render_nodes(group, canvas, cache);
        }

        canvas.restore();
    } else {
        render_nodes(group, canvas, cache);
    }

    if has_clip {
        canvas.restore();
    }
}

fn apply_clip_path(clip: &usvgr::ClipPath, canvas: &Canvas, cache: &mut RenderCache) {
    let clip_transform = to_matrix(clip.transform());

    for child in clip.root().children() {
        clip_child(child, canvas, cache, &clip_transform);
    }

    if let Some(nested_clip) = clip.clip_path() {
        apply_clip_path(nested_clip, canvas, cache);
    }
}

fn apply_clip_from_group(
    group: &usvgr::Group,
    canvas: &Canvas,
    cache: &mut RenderCache,
    transform: &Matrix,
) {
    for child in group.children() {
        clip_child(child, canvas, cache, transform);
    }
}

fn clip_child(child: &usvgr::Node, canvas: &Canvas, cache: &mut RenderCache, transform: &Matrix) {
    match child {
        usvgr::Node::Path(path) => {
            if path.visibility() != usvgr::Visibility::Visible {
                return;
            }
            let fill_type = path
                .fill()
                .map(|f| match f.rule() {
                    usvgr::FillRule::NonZero => skia_safe::PathFillType::Winding,
                    usvgr::FillRule::EvenOdd => skia_safe::PathFillType::EvenOdd,
                })
                .unwrap_or(skia_safe::PathFillType::Winding);

            let mut sk_path = cached_convert_path(cache, path.static_hash(), path.data());
            sk_path.set_fill_type(fill_type);
            // Transform the clip path geometry directly instead of using
            // canvas.concat + save/restore.  The old approach (save, concat,
            // clip_path, restore) undid the clip because Skia's restore pops
            // the entire canvas state including clip regions.
            let sk_path = sk_path.make_transform(transform);
            canvas.clip_path(&sk_path, skia_safe::ClipOp::Intersect, true);
        }
        usvgr::Node::Text(text) => {
            apply_clip_from_group(text.flattened(), canvas, cache, transform);
        }
        usvgr::Node::Group(group) => {
            let mut combined = *transform;
            combined.pre_concat(&to_matrix(group.transform()));
            apply_clip_from_group(group, canvas, cache, &combined);
        }
        _ => {}
    }
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

    canvas.save_layer(
        &skia_safe::canvas::SaveLayerRec::default()
            .paint(&mask_paint)
            .bounds(&bounds),
    );

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
    if path.visibility() != usvgr::Visibility::Visible {
        return;
    }

    if path.paint_order() == usvgr::PaintOrder::FillAndStroke {
        fill_path(path, canvas, cache);
        stroke_path(path, canvas, cache);
    } else {
        stroke_path(path, canvas, cache);
        fill_path(path, canvas, cache);
    }
}

fn fill_path(path: &usvgr::Path, canvas: &Canvas, cache: &mut RenderCache) {
    let Some(fill) = path.fill() else { return };

    let bounds = path.data().bounds();
    if bounds.width() == 0.0 || bounds.height() == 0.0 {
        return;
    }

    let fill_type = match fill.rule() {
        usvgr::FillRule::NonZero => skia_safe::PathFillType::Winding,
        usvgr::FillRule::EvenOdd => skia_safe::PathFillType::EvenOdd,
    };

    // Get or convert the Skia path (cached for static paths)
    let mut sk_path = cached_convert_path(cache, path.static_hash(), path.data());
    sk_path.set_fill_type(fill_type);

    // Fast path: use a cached Paint for static paths (avoids recreating
    // gradient shaders, dash effects, and other expensive paint state per frame)
    if let Some(hash) = path.static_hash() {
        if let Some(paint) = cache.get_fill_paint(hash) {
            canvas.draw_path(&sk_path, paint);
            return;
        }
    }

    let anti_alias = path.rendering_mode().use_shape_antialiasing();
    let Some(mut paint) = to_skia_paint(fill.paint(), fill.opacity(), anti_alias) else {
        return;
    };
    paint.set_style(skia_safe::PaintStyle::Fill);

    canvas.draw_path(&sk_path, &paint);

    if let Some(hash) = path.static_hash() {
        cache.fill_paints.insert(hash, paint);
    }
}

fn stroke_path(path: &usvgr::Path, canvas: &Canvas, cache: &mut RenderCache) {
    let Some(stroke) = path.stroke() else { return };

    let sk_path = cached_convert_path(cache, path.static_hash(), path.data());

    // Fast path: use a cached Paint for static paths
    if let Some(hash) = path.static_hash() {
        if let Some(paint) = cache.get_stroke_paint(hash) {
            canvas.draw_path(&sk_path, paint);
            return;
        }
    }

    let anti_alias = path.rendering_mode().use_shape_antialiasing();
    let Some(paint) = to_skia_stroke_paint(stroke, anti_alias) else {
        return;
    };

    canvas.draw_path(&sk_path, &paint);

    if let Some(hash) = path.static_hash() {
        cache.stroke_paints.insert(hash, paint);
    }
}

fn render_image(image: &usvgr::Image, canvas: &Canvas, cache: &mut RenderCache) {
    if image.visibility() != usvgr::Visibility::Visible {
        return;
    }

    match image.kind() {
        usvgr::ImageKind::DATA(data) => {
            render_raster_image(data, image.view_box(), canvas, cache);
        }
        usvgr::ImageKind::SVG { tree, .. } => {
            render_svg_image(tree, image.view_box(), canvas, cache);
        }
    }
}

fn render_raster_image(
    img: &Arc<usvgr::PreloadedImageData>,
    view_box: usvgr::ViewBox,
    canvas: &Canvas,
    cache: &mut RenderCache,
) {
    // Static images (from include_media_dir!) have strong_count > 1 because
    // the image_source HashMap holds a persistent Arc clone. These are safe
    // to cache by Arc pointer identity — the pointer is stable across frames.
    //
    // Ephemeral images (video frames) have strong_count == 1 — only the
    // usvgr::Tree owns them. After `drop(tree)` the allocator may reuse
    // the address for a new Arc, causing a stale cache hit (returning the
    // previous frame's Skia Image) or, worse, a use-after-free when the
    // PreloadedImageData used Cow::Owned. Create the Skia Image fresh
    // for these — the cost is negligible compared to video decode + resize.
    let ephemeral_asset;
    let sk_image = if Arc::strong_count(img) > 1 {
        let Some(asset) = cache.get_or_create_image_asset(img) else {
            return;
        };
        asset.image()
    } else {
        let Some(asset) = FFramesSkiaImage::new(img) else {
            return;
        };
        ephemeral_asset = asset;
        ephemeral_asset.image()
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
    // translate(x, y) * ts — left-multiplying a translate just offsets tx, ty.
    canvas.concat(&to_matrix(usvgr::Transform::from_row(
        ts.sx,
        ts.ky,
        ts.kx,
        ts.sy,
        ts.tx + vb_rect.x(),
        ts.ty + vb_rect.y(),
    )));

    let rect = skia_safe::Rect::from_wh(img.width as f32, img.height as f32);
    canvas.draw_image_rect(
        sk_image,
        Some((&rect, skia_safe::canvas::SrcRectConstraint::Strict)),
        &rect,
        &skia_safe::Paint::default(),
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
    canvas.translate((vb.x(), vb.y()));

    let ts = tree.view_box().to_transform(tree.size());
    canvas.concat(&to_matrix(ts));

    render_nodes(tree.root(), canvas, cache);
    canvas.restore();
}

/// Convert a usvgr Transform to a Skia Matrix.
fn to_matrix(ts: usvgr::Transform) -> Matrix {
    Matrix::new_all(ts.sx, ts.kx, ts.tx, ts.ky, ts.sy, ts.ty, 0.0, 0.0, 1.0)
}
