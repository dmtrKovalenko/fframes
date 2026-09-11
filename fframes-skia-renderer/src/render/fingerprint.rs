//! Runtime fingerprints of resolved `usvgr` nodes.
//!
//! The `svgr!` macro assigns a `static_hash` to nodes whose lexical content is
//! fully known at compile time, and [`RenderCache`](super::RenderCache) uses
//! it as the cache key.  The macro cannot see everything that influences how a
//! node finally renders, though: a node may inherit `fill` from a parent
//! produced by another `svgr!` invocation, reference a gradient defined in a
//! different subtree, or resolve percentage lengths against a viewport that
//! changes per frame.  Before a cached entry is reused, its fingerprint — a
//! hash of the *resolved* rendering state — is compared with the fingerprint
//! of the node being drawn, so a stale entry is rebuilt instead of replayed.
//!
//! Fingerprints are cheap on purpose: path geometry is what `static_hash`
//! vouches for, so large paths only mix in their bounds, segment count and a
//! strided sample of points.

use std::hash::Hasher;
use std::sync::Arc;

use fframes::usvgr;
use fframes::usvgr::filter;
use fframes::usvgr::tiny_skia_path;

/// FxHash (the hasher rustc uses): a couple of instructions per word, which
/// matters because every static group is fingerprinted on every frame.
#[derive(Default)]
struct FxHasher(u64);

impl FxHasher {
    const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

    #[inline]
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(Self::SEED);
    }

    #[inline]
    fn f32(&mut self, value: f32) {
        self.add(value.to_bits() as u64);
    }

    #[inline]
    fn bool(&mut self, value: bool) {
        self.add(value as u64);
    }

    #[inline]
    fn str(&mut self, value: &str) {
        self.write(value.as_bytes());
        self.add(value.len() as u64);
    }

    fn rect(&mut self, rect: usvgr::NonZeroRect) {
        self.f32(rect.x());
        self.f32(rect.y());
        self.f32(rect.width());
        self.f32(rect.height());
    }

    fn transform(&mut self, ts: usvgr::Transform) {
        self.f32(ts.sx);
        self.f32(ts.kx);
        self.f32(ts.ky);
        self.f32(ts.sy);
        self.f32(ts.tx);
        self.f32(ts.ty);
    }

    fn color(&mut self, color: usvgr::Color) {
        self.add(u64::from_le_bytes([
            color.red,
            color.green,
            color.blue,
            0,
            0,
            0,
            0,
            0,
        ]));
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(word));
        }
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add(i as u64);
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(i as u64);
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }
}

/// Fingerprint of a group and its whole subtree, including the clip paths,
/// masks, filters and paint servers it resolves to.
pub(super) fn group_fingerprint(group: &usvgr::Group) -> u64 {
    let mut hasher = FxHasher::default();
    hash_group(&mut hasher, group);
    hasher.finish()
}

/// Fingerprint of the geometry a cached `skia_safe::Path` was converted from.
pub(super) fn path_geometry_fingerprint(path: &tiny_skia_path::Path) -> u64 {
    let mut hasher = FxHasher::default();
    hash_path_geometry(&mut hasher, path);
    hasher.finish()
}

/// Fingerprint of the pixels a cached Skia image views.
pub(super) fn image_fingerprint(pixels: &Arc<usvgr::PreloadedImageData>) -> u64 {
    let mut hasher = FxHasher::default();
    hash_image_data(&mut hasher, pixels);
    hasher.finish()
}

/// Fingerprint of everything a cached fill `Paint` depends on.
pub(super) fn fill_fingerprint(fill: &usvgr::Fill, anti_alias: bool) -> u64 {
    let mut hasher = FxHasher::default();
    hasher.bool(anti_alias);
    hash_fill(&mut hasher, fill);
    hasher.finish()
}

/// Fingerprint of everything a cached stroke `Paint` depends on.
pub(super) fn stroke_fingerprint(stroke: &usvgr::Stroke, anti_alias: bool) -> u64 {
    let mut hasher = FxHasher::default();
    hasher.bool(anti_alias);
    hash_stroke(&mut hasher, stroke);
    hasher.finish()
}

fn hash_group(h: &mut FxHasher, group: &usvgr::Group) {
    h.transform(group.transform());
    h.f32(group.opacity().get());
    h.write_u8(group.blend_mode() as u8);
    h.bool(group.isolate());

    h.bool(group.clip_path().is_some());
    if let Some(clip_path) = group.clip_path() {
        hash_clip_path(h, clip_path);
    }

    h.bool(group.mask().is_some());
    if let Some(mask) = group.mask() {
        hash_mask(h, mask);
    }

    h.write_usize(group.filters().len());
    for filter in group.filters() {
        hash_filter(h, filter);
    }

    h.write_usize(group.children().len());
    for child in group.children() {
        hash_node(h, child);
    }
}

fn hash_node(h: &mut FxHasher, node: &usvgr::Node) {
    match node {
        usvgr::Node::Group(group) => {
            h.write_u8(0);
            hash_group(h, group);
        }
        usvgr::Node::Path(path) => {
            h.write_u8(1);
            hash_path(h, path);
        }
        usvgr::Node::Image(image) => {
            h.write_u8(2);
            hash_image(h, image);
        }
        usvgr::Node::Text(text) => {
            h.write_u8(3);
            hash_group(h, text.flattened());
        }
    }
}

fn hash_clip_path(h: &mut FxHasher, clip_path: &usvgr::ClipPath) {
    h.transform(clip_path.transform());
    hash_group(h, clip_path.root());
    h.bool(clip_path.clip_path().is_some());
    if let Some(nested) = clip_path.clip_path() {
        hash_clip_path(h, nested);
    }
}

fn hash_mask(h: &mut FxHasher, mask: &usvgr::Mask) {
    h.rect(mask.rect());
    h.write_u8(mask.kind() as u8);
    hash_group(h, mask.root());
    h.bool(mask.mask().is_some());
    if let Some(nested) = mask.mask() {
        hash_mask(h, nested);
    }
}

fn hash_path(h: &mut FxHasher, path: &usvgr::Path) {
    h.write_u8(path.visibility() as u8);
    h.write_u8(path.paint_order() as u8);
    h.write_u8(path.rendering_mode() as u8);

    h.bool(path.fill().is_some());
    if let Some(fill) = path.fill() {
        hash_fill(h, fill);
    }

    h.bool(path.stroke().is_some());
    if let Some(stroke) = path.stroke() {
        hash_stroke(h, stroke);
    }

    hash_path_geometry(h, path.data());
}

/// Paths up to this many points are hashed completely; longer ones are
/// sampled.  Geometry that changes without moving the bounds (a `rx="10%"`
/// resolving against a different viewport) is caught either way for the
/// small shapes where that happens.
const FULL_HASH_POINTS: usize = 256;
const SAMPLED_POINTS: usize = 32;

fn hash_path_geometry(h: &mut FxHasher, path: &tiny_skia_path::Path) {
    let bounds = path.bounds();
    h.f32(bounds.x());
    h.f32(bounds.y());
    h.f32(bounds.width());
    h.f32(bounds.height());
    h.write_usize(path.len());

    let points = path.points();
    let stride = (points.len() / FULL_HASH_POINTS).max(1);
    for point in points
        .iter()
        .step_by(stride)
        .take(FULL_HASH_POINTS.max(SAMPLED_POINTS))
    {
        h.f32(point.x);
        h.f32(point.y);
    }
}

fn hash_fill(h: &mut FxHasher, fill: &usvgr::Fill) {
    hash_paint(h, fill.paint());
    h.f32(fill.opacity().get());
    h.write_u8(fill.rule() as u8);
}

fn hash_stroke(h: &mut FxHasher, stroke: &usvgr::Stroke) {
    hash_paint(h, stroke.paint());
    h.f32(stroke.opacity().get());
    h.f32(stroke.width().get());
    h.f32(stroke.miterlimit().get());
    h.f32(stroke.dashoffset());
    h.write_u8(stroke.linecap() as u8);
    h.write_u8(stroke.linejoin() as u8);
    h.bool(stroke.dasharray().is_some());
    for dash in stroke.dasharray().unwrap_or_default() {
        h.f32(*dash);
    }
}

fn hash_paint(h: &mut FxHasher, paint: &usvgr::Paint) {
    match paint {
        usvgr::Paint::Color(color) => {
            h.write_u8(0);
            h.color(*color);
        }
        usvgr::Paint::LinearGradient(gradient) => {
            h.write_u8(1);
            h.f32(gradient.x1());
            h.f32(gradient.y1());
            h.f32(gradient.x2());
            h.f32(gradient.y2());
            hash_base_gradient(h, gradient);
        }
        usvgr::Paint::RadialGradient(gradient) => {
            h.write_u8(2);
            h.f32(gradient.cx());
            h.f32(gradient.cy());
            h.f32(gradient.r().get());
            h.f32(gradient.fx());
            h.f32(gradient.fy());
            hash_base_gradient(h, gradient);
        }
        usvgr::Paint::Pattern(pattern) => {
            h.write_u8(3);
            h.rect(pattern.rect());
            h.transform(pattern.transform());
            h.bool(pattern.view_box().is_some());
            if let Some(view_box) = pattern.view_box() {
                hash_view_box(h, view_box);
            }
            hash_group(h, pattern.root());
        }
    }
}

fn hash_base_gradient(h: &mut FxHasher, gradient: &usvgr::BaseGradient) {
    h.transform(gradient.transform());
    h.write_u8(gradient.spread_method() as u8);
    h.write_usize(gradient.stops().len());
    for stop in gradient.stops() {
        h.f32(stop.offset().get());
        h.f32(stop.opacity().get());
        h.color(stop.color());
    }
}

fn hash_view_box(h: &mut FxHasher, view_box: usvgr::ViewBox) {
    h.rect(view_box.rect);
    h.bool(view_box.aspect.defer);
    h.write_u8(view_box.aspect.align as u8);
    h.bool(view_box.aspect.slice);
}

fn hash_image(h: &mut FxHasher, image: &usvgr::Image) {
    h.write_u8(image.visibility() as u8);
    h.write_u8(image.rendering_mode() as u8);
    hash_view_box(h, image.view_box());
    hash_image_kind(h, image.kind());
}

fn hash_image_kind(h: &mut FxHasher, kind: &usvgr::ImageKind) {
    match kind {
        usvgr::ImageKind::DATA(data) => {
            h.write_u8(0);
            hash_image_data(h, data);
        }
        usvgr::ImageKind::SVG { tree, .. } => {
            h.write_u8(1);
            h.f32(tree.size().width());
            h.f32(tree.size().height());
            hash_view_box(h, tree.view_box());
            hash_group(h, tree.root());
        }
    }
}

/// The cache keeps a clone of every `Arc` it has seen this frame, so the
/// address identifies the allocation for as long as the entry exists.
fn hash_image_data(h: &mut FxHasher, data: &Arc<usvgr::PreloadedImageData>) {
    h.write_usize(Arc::as_ptr(data) as usize);
    h.write_u32(data.width);
    h.write_u32(data.height);
}

fn hash_input(h: &mut FxHasher, input: &filter::Input) {
    match input {
        filter::Input::SourceGraphic => h.write_u8(0),
        filter::Input::SourceAlpha => h.write_u8(1),
        filter::Input::Reference(name) => {
            h.write_u8(2);
            h.str(name);
        }
    }
}

fn hash_light_source(h: &mut FxHasher, light: filter::LightSource) {
    match light {
        filter::LightSource::DistantLight(light) => {
            h.write_u8(0);
            h.f32(light.azimuth);
            h.f32(light.elevation);
        }
        filter::LightSource::PointLight(light) => {
            h.write_u8(1);
            h.f32(light.x);
            h.f32(light.y);
            h.f32(light.z);
        }
        filter::LightSource::SpotLight(light) => {
            h.write_u8(2);
            h.f32(light.x);
            h.f32(light.y);
            h.f32(light.z);
            h.f32(light.points_at_x);
            h.f32(light.points_at_y);
            h.f32(light.points_at_z);
            h.f32(light.specular_exponent.get());
            h.f32(light.limiting_cone_angle.unwrap_or(f32::NAN));
        }
    }
}

fn hash_transfer_function(h: &mut FxHasher, func: &filter::TransferFunction) {
    match func {
        filter::TransferFunction::Identity => h.write_u8(0),
        filter::TransferFunction::Table(values) => {
            h.write_u8(1);
            h.write_usize(values.len());
            values.iter().for_each(|v| h.f32(*v));
        }
        filter::TransferFunction::Discrete(values) => {
            h.write_u8(2);
            h.write_usize(values.len());
            values.iter().for_each(|v| h.f32(*v));
        }
        filter::TransferFunction::Linear { slope, intercept } => {
            h.write_u8(3);
            h.f32(*slope);
            h.f32(*intercept);
        }
        filter::TransferFunction::Gamma {
            amplitude,
            exponent,
            offset,
        } => {
            h.write_u8(4);
            h.f32(*amplitude);
            h.f32(*exponent);
            h.f32(*offset);
        }
    }
}

fn hash_filter(h: &mut FxHasher, filter: &filter::Filter) {
    h.rect(filter.rect());
    h.write_usize(filter.primitives().len());
    for primitive in filter.primitives() {
        h.rect(primitive.rect());
        h.write_u8(primitive.color_interpolation() as u8);
        h.str(primitive.result());
        hash_filter_kind(h, primitive.kind());
    }
}

fn hash_filter_kind(h: &mut FxHasher, kind: &filter::Kind) {
    use filter::Kind;

    match kind {
        Kind::Blend(fe) => {
            h.write_u8(0);
            hash_input(h, fe.input1());
            hash_input(h, fe.input2());
            h.write_u8(fe.mode() as u8);
        }
        Kind::ColorMatrix(fe) => {
            h.write_u8(1);
            hash_input(h, fe.input());
            match fe.kind() {
                filter::ColorMatrixKind::Matrix(values) => {
                    h.write_u8(0);
                    values.iter().for_each(|v| h.f32(*v));
                }
                filter::ColorMatrixKind::Saturate(v) => {
                    h.write_u8(1);
                    h.f32(v.get());
                }
                filter::ColorMatrixKind::HueRotate(v) => {
                    h.write_u8(2);
                    h.f32(*v);
                }
                filter::ColorMatrixKind::LuminanceToAlpha => h.write_u8(3),
            }
        }
        Kind::ComponentTransfer(fe) => {
            h.write_u8(2);
            hash_input(h, fe.input());
            hash_transfer_function(h, fe.func_r());
            hash_transfer_function(h, fe.func_g());
            hash_transfer_function(h, fe.func_b());
            hash_transfer_function(h, fe.func_a());
        }
        Kind::Composite(fe) => {
            h.write_u8(3);
            hash_input(h, fe.input1());
            hash_input(h, fe.input2());
            match fe.operator() {
                filter::CompositeOperator::Over => h.write_u8(0),
                filter::CompositeOperator::In => h.write_u8(1),
                filter::CompositeOperator::Out => h.write_u8(2),
                filter::CompositeOperator::Atop => h.write_u8(3),
                filter::CompositeOperator::Xor => h.write_u8(4),
                filter::CompositeOperator::Arithmetic { k1, k2, k3, k4 } => {
                    h.write_u8(5);
                    h.f32(k1);
                    h.f32(k2);
                    h.f32(k3);
                    h.f32(k4);
                }
            }
        }
        Kind::ConvolveMatrix(fe) => {
            h.write_u8(4);
            hash_input(h, fe.input());
            let matrix = fe.matrix();
            h.write_u32(matrix.target_x());
            h.write_u32(matrix.target_y());
            h.write_u32(matrix.columns());
            h.write_u32(matrix.rows());
            matrix.data().iter().for_each(|v| h.f32(*v));
            h.f32(fe.divisor().get());
            h.f32(fe.bias());
            h.write_u8(fe.edge_mode() as u8);
            h.bool(fe.preserve_alpha());
        }
        Kind::DiffuseLighting(fe) => {
            h.write_u8(5);
            hash_input(h, fe.input());
            h.f32(fe.surface_scale());
            h.f32(fe.diffuse_constant());
            h.color(fe.lighting_color());
            hash_light_source(h, fe.light_source());
        }
        Kind::DisplacementMap(fe) => {
            h.write_u8(6);
            hash_input(h, fe.input1());
            hash_input(h, fe.input2());
            h.f32(fe.scale());
            h.write_u8(fe.x_channel_selector() as u8);
            h.write_u8(fe.y_channel_selector() as u8);
        }
        Kind::DropShadow(fe) => {
            h.write_u8(7);
            hash_input(h, fe.input());
            h.f32(fe.dx());
            h.f32(fe.dy());
            h.f32(fe.std_dev_x().get());
            h.f32(fe.std_dev_y().get());
            h.color(fe.color());
            h.f32(fe.opacity().get());
        }
        Kind::Flood(fe) => {
            h.write_u8(8);
            h.color(fe.color());
            h.f32(fe.opacity().get());
        }
        Kind::GaussianBlur(fe) => {
            h.write_u8(9);
            hash_input(h, fe.input());
            h.f32(fe.std_dev_x().get());
            h.f32(fe.std_dev_y().get());
        }
        Kind::Image(fe) => {
            h.write_u8(10);
            h.bool(fe.aspect().defer);
            h.write_u8(fe.aspect().align as u8);
            h.bool(fe.aspect().slice);
            h.write_u8(fe.rendering_mode() as u8);
            match fe.data() {
                filter::ImageKind::Image(kind) => {
                    h.write_u8(0);
                    hash_image_kind(h, kind);
                }
                filter::ImageKind::Use(group) => {
                    h.write_u8(1);
                    hash_group(h, group);
                }
            }
        }
        Kind::Merge(fe) => {
            h.write_u8(11);
            h.write_usize(fe.inputs().len());
            fe.inputs().iter().for_each(|input| hash_input(h, input));
        }
        Kind::Morphology(fe) => {
            h.write_u8(12);
            hash_input(h, fe.input());
            h.write_u8(fe.operator() as u8);
            h.f32(fe.radius_x().get());
            h.f32(fe.radius_y().get());
        }
        Kind::Offset(fe) => {
            h.write_u8(13);
            hash_input(h, fe.input());
            h.f32(fe.dx());
            h.f32(fe.dy());
        }
        Kind::SpecularLighting(fe) => {
            h.write_u8(14);
            hash_input(h, fe.input());
            h.f32(fe.surface_scale());
            h.f32(fe.specular_constant());
            h.f32(fe.specular_exponent());
            h.color(fe.lighting_color());
            hash_light_source(h, fe.light_source());
        }
        Kind::Tile(fe) => {
            h.write_u8(15);
            hash_input(h, fe.input());
        }
        Kind::Turbulence(fe) => {
            h.write_u8(16);
            h.f32(fe.base_frequency_x().get());
            h.f32(fe.base_frequency_y().get());
            h.write_u32(fe.num_octaves());
            h.add(fe.seed() as i64 as u64);
            h.bool(fe.stitch_tiles());
            h.write_u8(fe.kind() as u8);
        }
    }
}
