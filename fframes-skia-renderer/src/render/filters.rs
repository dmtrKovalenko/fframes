//! SVG filter primitives to Skia ImageFilter conversion.
//!
//! Walks the usvgr filter DAG and builds a composed Skia ImageFilter tree.
//!
//! Mirrors the behavior of Skia's SVG DOM (`modules/svg`), which the old
//! serialize-to-string render path used: every primitive result is tagged with
//! its `color-interpolation-filters` colorspace and conversions are inserted
//! when a primitive consumes an input produced in a different colorspace.

use fframes::usvgr;
use fframes::usvgr::filter::{self, ColorInterpolation, Input, Kind};
use skia_safe::{self, ImageFilter, image_filters};
use std::collections::HashMap;
use std::sync::Arc;

use super::RenderCache;
use super::convert::convert_blend_mode;

/// A converted filter primitive result plus the metadata needed to consume it.
#[derive(Clone)]
struct PrimitiveResult {
    filter: ImageFilter,
    /// Colorspace the result was produced in (`color-interpolation-filters`).
    color_interpolation: ColorInterpolation,
    /// The primitive's subregion; used by `feTile` as the tile source area.
    subregion: usvgr::NonZeroRect,
}

/// Build a composed ImageFilter from a list of usvgr filters.
///
/// SVG allows multiple `<filter>` elements to be applied to a single group.
/// Each filter contains a list of primitives that form a DAG.
pub fn build_filter_chain(
    filters: &[Arc<filter::Filter>],
    cache: &mut RenderCache,
) -> Option<ImageFilter> {
    let mut result: Option<ImageFilter> = None;

    for filter in filters {
        let filter_result = build_single_filter(filter, cache);
        result = match (result, filter_result) {
            (None, f) => f,
            (Some(existing), None) => Some(existing),
            (Some(existing), Some(new)) => image_filters::compose(new, existing),
        };
    }

    result
}

/// Build an ImageFilter from a single usvgr Filter (which has multiple primitives).
fn build_single_filter(filter: &filter::Filter, cache: &mut RenderCache) -> Option<ImageFilter> {
    let primitives = filter.primitives();
    if primitives.is_empty() {
        return None;
    }

    // Map of named results from filter primitives
    let mut results: HashMap<&str, PrimitiveResult> = HashMap::new();
    let mut last_result: Option<PrimitiveResult> = None;

    for primitive in primitives {
        let crop_rect = primitive_crop_rect(primitive, filter);
        let cs = primitive_colorspace(primitive, &results, &last_result);
        let image_filter =
            convert_primitive(primitive, cs, filter, &results, &last_result, crop_rect, cache);

        if let Some(f) = image_filter {
            let result = PrimitiveResult {
                filter: f,
                color_interpolation: cs,
                subregion: primitive.rect(),
            };
            if !primitive.result().is_empty() {
                results.insert(primitive.result(), result.clone());
            }
            last_result = Some(result);
        }
    }

    // The composed chain is consumed by the canvas in sRGB, and the filter's
    // output is confined to its region (this matters when several filters
    // are chained on one element).
    let last = last_result?;
    let out = convert_colorspace(
        Some(last.filter),
        last.color_interpolation,
        ColorInterpolation::SRGB,
    )?;
    let region = filter.rect();
    image_filters::crop(
        skia_safe::Rect::from_xywh(region.x(), region.y(), region.width(), region.height()),
        None,
        out,
    )
}

/// A primitive's subregion, clamped to the filter region (per spec the
/// subregion never extends past `x`/`y`/`width`/`height` of the filter).
fn primitive_crop_rect(primitive: &filter::Primitive, filter: &filter::Filter) -> skia_safe::Rect {
    let rect = primitive.rect();
    let region = filter.rect();
    let mut crop = skia_safe::Rect::from_xywh(rect.x(), rect.y(), rect.width(), rect.height());
    let region = skia_safe::Rect::from_xywh(
        region.x(),
        region.y(),
        region.width(),
        region.height(),
    );
    if !crop.intersect(region) {
        // A subregion entirely outside the filter region produces nothing.
        return skia_safe::Rect::new_empty();
    }
    crop
}

/// Insert a gamma-conversion color filter when a result produced in `from`
/// is consumed in `to`.  `None` stands for the raw source bitmap (sRGB).
fn convert_colorspace(
    filter: Option<ImageFilter>,
    from: ColorInterpolation,
    to: ColorInterpolation,
) -> Option<ImageFilter> {
    if from == to {
        return filter;
    }
    let cf = match to {
        ColorInterpolation::LinearRGB => skia_safe::color_filters::srgb_to_linear_gamma(),
        ColorInterpolation::SRGB => skia_safe::color_filters::linear_to_srgb_gamma(),
    };
    image_filters::color_filter(cf, filter, None)
}

/// Resolve an SVG filter Input reference to a Skia ImageFilter in the
/// colorspace `target_cs` of the consuming primitive.
///
/// - `SourceGraphic` -> None (Skia treats None as the source bitmap)
/// - `SourceAlpha` -> A color matrix that extracts alpha
/// - `Reference(name)` -> lookup in the results map
fn resolve_input(
    input: &Input,
    target_cs: ColorInterpolation,
    results: &HashMap<&str, PrimitiveResult>,
    last_result: &Option<PrimitiveResult>,
) -> Option<ImageFilter> {
    match input {
        Input::SourceGraphic => convert_colorspace(None, ColorInterpolation::SRGB, target_cs),
        Input::SourceAlpha => {
            // Extract alpha channel: multiply RGB by 0, keep A.
            // Alpha-only output is identical in both colorspaces, so no
            // gamma conversion is needed.
            let matrix: [f32; 20] = [
                0.0, 0.0, 0.0, 0.0, 0.0, // R
                0.0, 0.0, 0.0, 0.0, 0.0, // G
                0.0, 0.0, 0.0, 0.0, 0.0, // B
                0.0, 0.0, 0.0, 1.0, 0.0, // A
            ];
            let cf = skia_safe::color_filters::matrix_row_major(&matrix, None);
            image_filters::color_filter(cf, None, None)
        }
        Input::Reference(name) => match results.get(name.as_str()).or(last_result.as_ref()) {
            Some(res) => convert_colorspace(
                Some(res.filter.clone()),
                res.color_interpolation,
                target_cs,
            ),
            None => convert_colorspace(None, ColorInterpolation::SRGB, target_cs),
        },
    }
}

/// The colorspace an input arrives in.  The source bitmap is sRGB; named
/// results carry the colorspace they were produced in.
fn input_colorspace(
    input: &Input,
    results: &HashMap<&str, PrimitiveResult>,
    last_result: &Option<PrimitiveResult>,
) -> ColorInterpolation {
    match input {
        Input::SourceGraphic | Input::SourceAlpha => ColorInterpolation::SRGB,
        Input::Reference(name) => results
            .get(name.as_str())
            .or(last_result.as_ref())
            .map(|res| res.color_interpolation)
            .unwrap_or(ColorInterpolation::SRGB),
    }
}

/// The colorspace a primitive operates in and produces its result in.
///
/// Special cases (mirroring svgr/resvg, the reference renderer for usvgr
/// trees):
/// - `feDisplacementMap`: per the filter spec its `in` image must remain in
///   its current colorspace, so the primitive operates in input1's
///   colorspace rather than its own `color-interpolation-filters` value.
/// - `feFlood`/`feImage`/`feTile`: generators whose output is defined in
///   sRGB (flood colors and image pixels are sRGB values; tile copies
///   pixels verbatim).
fn primitive_colorspace(
    primitive: &filter::Primitive,
    results: &HashMap<&str, PrimitiveResult>,
    last_result: &Option<PrimitiveResult>,
) -> ColorInterpolation {
    match primitive.kind() {
        Kind::DisplacementMap(fe) => input_colorspace(fe.input1(), results, last_result),
        Kind::Flood(_) | Kind::Image(_) | Kind::Tile(_) => ColorInterpolation::SRGB,
        _ => primitive.color_interpolation(),
    }
}

/// The subregion that an input covers — the referenced primitive's subregion,
/// or the whole filter region for `SourceGraphic`/`SourceAlpha`.
fn input_subregion(
    input: &Input,
    filter: &filter::Filter,
    results: &HashMap<&str, PrimitiveResult>,
    last_result: &Option<PrimitiveResult>,
) -> usvgr::NonZeroRect {
    match input {
        Input::Reference(name) => results
            .get(name.as_str())
            .or(last_result.as_ref())
            .map(|res| res.subregion)
            .unwrap_or_else(|| filter.rect()),
        _ => filter.rect(),
    }
}

fn convert_color_channel(channel: filter::ColorChannel) -> skia_safe::ColorChannel {
    match channel {
        filter::ColorChannel::R => skia_safe::ColorChannel::R,
        filter::ColorChannel::G => skia_safe::ColorChannel::G,
        filter::ColorChannel::B => skia_safe::ColorChannel::B,
        filter::ColorChannel::A => skia_safe::ColorChannel::A,
    }
}

/// SVG distant light angles to a Skia direction vector.
fn distant_light_direction(light: &filter::DistantLight) -> skia_safe::Point3 {
    let azimuth = light.azimuth.to_radians();
    let elevation = light.elevation.to_radians();
    skia_safe::Point3::new(
        azimuth.cos() * elevation.cos(),
        azimuth.sin() * elevation.cos(),
        elevation.sin(),
    )
}

fn lighting_color(color: usvgr::Color) -> skia_safe::Color {
    skia_safe::Color::from_rgb(color.red, color.green, color.blue)
}

/// Convert a single usvgr filter primitive to a Skia ImageFilter.
///
/// `cs` is the colorspace the primitive operates in (see
/// [`primitive_colorspace`]); inputs are converted into it as needed.
fn convert_primitive(
    primitive: &filter::Primitive,
    cs: ColorInterpolation,
    filter: &filter::Filter,
    results: &HashMap<&str, PrimitiveResult>,
    last_result: &Option<PrimitiveResult>,
    crop_rect: skia_safe::Rect,
    cache: &mut RenderCache,
) -> Option<ImageFilter> {
    match primitive.kind() {
        Kind::GaussianBlur(fe) => {
            let input = resolve_input(fe.input(), cs, results, last_result);
            image_filters::blur(
                (fe.std_dev_x().get(), fe.std_dev_y().get()),
                None,
                input,
                crop_rect,
            )
        }

        Kind::Offset(fe) => {
            let input = resolve_input(fe.input(), cs, results, last_result);
            image_filters::offset(skia_safe::Vector::new(fe.dx(), fe.dy()), input, crop_rect)
        }

        Kind::Flood(fe) => {
            let color = skia_safe::Color::from_argb(
                fe.opacity().to_u8(),
                fe.color().red,
                fe.color().green,
                fe.color().blue,
            );
            let shader = skia_safe::shaders::color(color);
            image_filters::shader(shader, crop_rect)
        }

        Kind::Blend(fe) => {
            // SVG `in` (input1) is the source/foreground, `in2` (input2) is the destination/background.
            // Skia's blend() signature is blend(mode, background, foreground, crop_rect).
            let foreground = resolve_input(fe.input1(), cs, results, last_result);
            let background = resolve_input(fe.input2(), cs, results, last_result);
            let mode = convert_blend_mode(fe.mode());
            image_filters::blend(mode, background, foreground, crop_rect)
        }

        Kind::Composite(fe) => {
            // SVG `in` (input1) is the source/foreground, `in2` (input2) is the destination/background.
            // Skia's blend() signature is blend(mode, background, foreground, crop_rect).
            let foreground = resolve_input(fe.input1(), cs, results, last_result);
            let background = resolve_input(fe.input2(), cs, results, last_result);

            match fe.operator() {
                filter::CompositeOperator::Over => image_filters::blend(
                    skia_safe::BlendMode::SrcOver,
                    background,
                    foreground,
                    crop_rect,
                ),
                filter::CompositeOperator::In => image_filters::blend(
                    skia_safe::BlendMode::SrcIn,
                    background,
                    foreground,
                    crop_rect,
                ),
                filter::CompositeOperator::Out => image_filters::blend(
                    skia_safe::BlendMode::SrcOut,
                    background,
                    foreground,
                    crop_rect,
                ),
                filter::CompositeOperator::Atop => image_filters::blend(
                    skia_safe::BlendMode::SrcATop,
                    background,
                    foreground,
                    crop_rect,
                ),
                filter::CompositeOperator::Xor => image_filters::blend(
                    skia_safe::BlendMode::Xor,
                    background,
                    foreground,
                    crop_rect,
                ),
                filter::CompositeOperator::Arithmetic { k1, k2, k3, k4 } => {
                    // Skia arithmetic: result = k1*fg*bg + k2*fg + k3*bg + k4
                    // SVG arithmetic:  result = k1*in*in2 + k2*in + k3*in2 + k4
                    // So SVG `in` (foreground) maps to k2, SVG `in2` (background) maps to k3.
                    image_filters::arithmetic(
                        k1, k2, k3, k4, true, background, foreground, crop_rect,
                    )
                }
            }
        }

        Kind::Merge(fe) => {
            let filters: Vec<Option<ImageFilter>> = fe
                .inputs()
                .iter()
                .map(|input| resolve_input(input, cs, results, last_result))
                .collect();

            image_filters::merge(filters, crop_rect)
        }

        Kind::DropShadow(fe) => {
            let input = resolve_input(fe.input(), cs, results, last_result);
            let color = skia_safe::Color4f::new(
                fe.color().red as f32 / 255.0,
                fe.color().green as f32 / 255.0,
                fe.color().blue as f32 / 255.0,
                fe.opacity().get(),
            );
            image_filters::drop_shadow(
                skia_safe::Vector::new(fe.dx(), fe.dy()),
                (fe.std_dev_x().get(), fe.std_dev_y().get()),
                color,
                None, // color space
                input,
                crop_rect,
            )
        }

        Kind::ComponentTransfer(fe) => {
            let input = resolve_input(fe.input(), cs, results, last_result);

            let table_r = build_transfer_table(fe.func_r());
            let table_g = build_transfer_table(fe.func_g());
            let table_b = build_transfer_table(fe.func_b());
            let table_a = build_transfer_table(fe.func_a());

            let cf = skia_safe::color_filters::table_argb(
                table_a.as_ref().map(|t| t as &[u8; 256]),
                table_r.as_ref().map(|t| t as &[u8; 256]),
                table_g.as_ref().map(|t| t as &[u8; 256]),
                table_b.as_ref().map(|t| t as &[u8; 256]),
            )?;

            image_filters::color_filter(cf, input, crop_rect)
        }

        Kind::ColorMatrix(fe) => {
            let input = resolve_input(fe.input(), cs, results, last_result);
            let cf = convert_color_matrix(fe.kind())?;
            image_filters::color_filter(cf, input, crop_rect)
        }

        Kind::Morphology(fe) => {
            let input = resolve_input(fe.input(), cs, results, last_result);
            match fe.operator() {
                filter::MorphologyOperator::Erode => image_filters::erode(
                    (fe.radius_x().get(), fe.radius_y().get()),
                    input,
                    crop_rect,
                ),
                filter::MorphologyOperator::Dilate => image_filters::dilate(
                    (fe.radius_x().get(), fe.radius_y().get()),
                    input,
                    crop_rect,
                ),
            }
        }

        Kind::Tile(fe) => {
            let input = resolve_input(fe.input(), cs, results, last_result);
            // feTile repeats the *input's* subregion across this primitive's
            // subregion.
            let src_rect = input_subregion(fe.input(), filter, results, last_result);
            let src = skia_safe::Rect::from_xywh(
                src_rect.x(),
                src_rect.y(),
                src_rect.width(),
                src_rect.height(),
            );
            let prim_rect = primitive.rect();
            let dst = skia_safe::Rect::from_xywh(
                prim_rect.x(),
                prim_rect.y(),
                prim_rect.width(),
                prim_rect.height(),
            );
            image_filters::tile(src, dst, input)
        }

        Kind::Turbulence(fe) => {
            // feTurbulence ignores its input; it is a generator like feFlood.
            let tile_size = if fe.stitch_tiles() {
                let rect = primitive.rect();
                Some(skia_safe::ISize::new(
                    rect.width() as i32,
                    rect.height() as i32,
                ))
            } else {
                None
            };
            let base_frequency = (fe.base_frequency_x().get(), fe.base_frequency_y().get());
            let shader = match fe.kind() {
                filter::TurbulenceKind::FractalNoise => skia_safe::shaders::fractal_noise(
                    base_frequency,
                    fe.num_octaves() as usize,
                    fe.seed() as f32,
                    tile_size,
                ),
                filter::TurbulenceKind::Turbulence => skia_safe::shaders::turbulence(
                    base_frequency,
                    fe.num_octaves() as usize,
                    fe.seed() as f32,
                    tile_size,
                ),
            }?;
            image_filters::shader(shader, crop_rect)
        }

        Kind::ConvolveMatrix(fe) => {
            let input = resolve_input(fe.input(), cs, results, last_result);
            let m = fe.matrix();
            // SVG applies the kernel rotated 180° relative to Skia's
            // correlation (Skia computes sum(kernel[pos] * src(xy + pos - offset))),
            // so reverse the kernel values; the target point then maps to
            // Skia's kernel offset directly.
            let kernel: Vec<f32> = m.data().iter().rev().copied().collect();
            let kernel_offset =
                skia_safe::IPoint::new(m.target_x() as i32, m.target_y() as i32);
            let tile_mode = match fe.edge_mode() {
                filter::EdgeMode::Duplicate => skia_safe::TileMode::Clamp,
                filter::EdgeMode::Wrap => skia_safe::TileMode::Repeat,
                filter::EdgeMode::None => skia_safe::TileMode::Decal,
            };
            image_filters::matrix_convolution(
                skia_safe::ISize::new(m.columns() as i32, m.rows() as i32),
                &kernel,
                1.0 / fe.divisor().get(),
                fe.bias() * 255.0,
                kernel_offset,
                tile_mode,
                !fe.preserve_alpha(),
                input,
                crop_rect,
            )
        }

        Kind::DisplacementMap(fe) => {
            // SVG `in` (input1) is the image being displaced, `in2` (input2)
            // is the displacement map.
            let color = resolve_input(fe.input1(), cs, results, last_result);
            let displacement = resolve_input(fe.input2(), cs, results, last_result);
            image_filters::displacement_map(
                (
                    convert_color_channel(fe.x_channel_selector()),
                    convert_color_channel(fe.y_channel_selector()),
                ),
                fe.scale(),
                displacement,
                color,
                crop_rect,
            )
        }

        Kind::DiffuseLighting(fe) => {
            let input = resolve_input(fe.input(), cs, results, last_result);
            let color = lighting_color(fe.lighting_color());
            match fe.light_source() {
                filter::LightSource::DistantLight(light) => image_filters::distant_lit_diffuse(
                    distant_light_direction(&light),
                    color,
                    fe.surface_scale(),
                    fe.diffuse_constant(),
                    input,
                    crop_rect,
                ),
                filter::LightSource::PointLight(light) => image_filters::point_lit_diffuse(
                    skia_safe::Point3::new(light.x, light.y, light.z),
                    color,
                    fe.surface_scale(),
                    fe.diffuse_constant(),
                    input,
                    crop_rect,
                ),
                filter::LightSource::SpotLight(light) => image_filters::spot_lit_diffuse(
                    skia_safe::Point3::new(light.x, light.y, light.z),
                    skia_safe::Point3::new(
                        light.points_at_x,
                        light.points_at_y,
                        light.points_at_z,
                    ),
                    light.specular_exponent.get(),
                    light.limiting_cone_angle.unwrap_or(180.0),
                    color,
                    fe.surface_scale(),
                    fe.diffuse_constant(),
                    input,
                    crop_rect,
                ),
            }
        }

        Kind::SpecularLighting(fe) => {
            let input = resolve_input(fe.input(), cs, results, last_result);
            let color = lighting_color(fe.lighting_color());
            match fe.light_source() {
                filter::LightSource::DistantLight(light) => image_filters::distant_lit_specular(
                    distant_light_direction(&light),
                    color,
                    fe.surface_scale(),
                    fe.specular_constant(),
                    fe.specular_exponent(),
                    input,
                    crop_rect,
                ),
                filter::LightSource::PointLight(light) => image_filters::point_lit_specular(
                    skia_safe::Point3::new(light.x, light.y, light.z),
                    color,
                    fe.surface_scale(),
                    fe.specular_constant(),
                    fe.specular_exponent(),
                    input,
                    crop_rect,
                ),
                filter::LightSource::SpotLight(light) => image_filters::spot_lit_specular(
                    skia_safe::Point3::new(light.x, light.y, light.z),
                    skia_safe::Point3::new(
                        light.points_at_x,
                        light.points_at_y,
                        light.points_at_z,
                    ),
                    light.specular_exponent.get(),
                    light.limiting_cone_angle.unwrap_or(180.0),
                    color,
                    fe.surface_scale(),
                    fe.specular_constant(),
                    fe.specular_exponent(),
                    input,
                    crop_rect,
                ),
            }
        }

        Kind::Image(fe) => {
            // feImage ignores its input; it renders an image (or a referenced
            // node) into the primitive subregion, like a one-shot <image>.
            let subregion = primitive.rect();
            let bounds = skia_safe::Rect::from_xywh(
                subregion.x(),
                subregion.y(),
                subregion.width(),
                subregion.height(),
            );

            let mut recorder = skia_safe::PictureRecorder::new();
            let rec_canvas = recorder.begin_recording(bounds, false);
            match fe.data() {
                filter::ImageKind::Image(kind) => {
                    let view_box = usvgr::ViewBox {
                        rect: subregion,
                        aspect: fe.aspect(),
                    };
                    super::render_image_kind(
                        kind,
                        view_box,
                        fe.rendering_mode(),
                        rec_canvas,
                        cache,
                    );
                }
                filter::ImageKind::Use(group) => {
                    rec_canvas.translate((subregion.x(), subregion.y()));
                    super::render_group(group, rec_canvas, cache);
                }
            }
            let picture = recorder.finish_recording_as_picture(Some(&bounds))?;
            image_filters::picture(picture, &bounds)
        }
    }
}

/// Build a 256-entry transfer function table from a usvgr TransferFunction.
fn build_transfer_table(func: &filter::TransferFunction) -> Option<Box<[u8; 256]>> {
    match func {
        filter::TransferFunction::Identity => None,
        filter::TransferFunction::Table(values) => {
            let mut table = Box::new([0u8; 256]);
            if values.is_empty() {
                return None;
            }
            if values.len() == 1 {
                let value = (values[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                table.fill(value);
                return Some(table);
            }
            for i in 0..256 {
                let t = i as f32 / 255.0;
                let n = values.len() - 1;
                let k = (t * n as f32).floor() as usize;
                let k = k.min(n - 1);
                let frac = t * n as f32 - k as f32;
                let value = values[k] + frac * (values[k + 1] - values[k]);
                table[i] = (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
            Some(table)
        }
        filter::TransferFunction::Discrete(values) => {
            let mut table = Box::new([0u8; 256]);
            if values.is_empty() {
                return None;
            }
            for i in 0..256 {
                let t = i as f32 / 255.0;
                let k = (t * values.len() as f32).floor() as usize;
                let k = k.min(values.len() - 1);
                table[i] = (values[k].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
            Some(table)
        }
        filter::TransferFunction::Linear { slope, intercept } => {
            let mut table = Box::new([0u8; 256]);
            for i in 0..256 {
                let t = i as f32 / 255.0;
                let value = slope * t + intercept;
                table[i] = (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
            Some(table)
        }
        filter::TransferFunction::Gamma {
            amplitude,
            exponent,
            offset,
        } => {
            let mut table = Box::new([0u8; 256]);
            for i in 0..256 {
                let t = i as f32 / 255.0;
                let value = amplitude * t.powf(*exponent) + offset;
                table[i] = (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
            Some(table)
        }
    }
}

/// Convert a usvgr ColorMatrixKind to a Skia ColorFilter.
fn convert_color_matrix(kind: &filter::ColorMatrixKind) -> Option<skia_safe::ColorFilter> {
    match kind {
        filter::ColorMatrixKind::Matrix(values) => {
            if values.len() != 20 {
                return None;
            }
            let mut array = [0f32; 20];
            array.copy_from_slice(values);
            Some(skia_safe::color_filters::matrix_row_major(&array, None))
        }
        filter::ColorMatrixKind::Saturate(s) => {
            let s = s.get();
            let matrix: [f32; 20] = [
                0.213 + 0.787 * s,
                0.715 - 0.715 * s,
                0.072 - 0.072 * s,
                0.0,
                0.0,
                0.213 - 0.213 * s,
                0.715 + 0.285 * s,
                0.072 - 0.072 * s,
                0.0,
                0.0,
                0.213 - 0.213 * s,
                0.715 - 0.715 * s,
                0.072 + 0.928 * s,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
            ];
            Some(skia_safe::color_filters::matrix_row_major(&matrix, None))
        }
        filter::ColorMatrixKind::HueRotate(angle) => {
            let angle = angle.to_radians();
            let cos = angle.cos();
            let sin = angle.sin();
            let matrix: [f32; 20] = [
                0.213 + cos * 0.787 - sin * 0.213,
                0.715 - cos * 0.715 - sin * 0.715,
                0.072 - cos * 0.072 + sin * 0.928,
                0.0,
                0.0,
                0.213 - cos * 0.213 + sin * 0.143,
                0.715 + cos * 0.285 + sin * 0.140,
                0.072 - cos * 0.072 - sin * 0.283,
                0.0,
                0.0,
                0.213 - cos * 0.213 - sin * 0.787,
                0.715 - cos * 0.715 + sin * 0.715,
                0.072 + cos * 0.928 + sin * 0.072,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0,
                0.0,
            ];
            Some(skia_safe::color_filters::matrix_row_major(&matrix, None))
        }
        filter::ColorMatrixKind::LuminanceToAlpha => {
            let matrix: [f32; 20] = [
                0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.2126,
                0.7152, 0.0722, 0.0, 0.0,
            ];
            Some(skia_safe::color_filters::matrix_row_major(&matrix, None))
        }
    }
}
