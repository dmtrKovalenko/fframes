//! SVG filter primitives to Skia ImageFilter conversion.
//!
//! Walks the usvgr filter DAG and builds a composed Skia ImageFilter tree.

#[allow(unused_imports)]
use fframes::usvgr;
use fframes::usvgr::filter::{self, Input, Kind};
use skia_safe::{self, ImageFilter, image_filters};
use std::collections::HashMap;
#[allow(unused_imports)]
use std::sync::Arc;

use super::convert::convert_blend_mode;

/// Build a composed ImageFilter from a list of usvgr filters.
///
/// SVG allows multiple `<filter>` elements to be applied to a single group.
/// Each filter contains a list of primitives that form a DAG.
pub fn build_filter_chain(filters: &[Arc<filter::Filter>]) -> Option<ImageFilter> {
    let mut result: Option<ImageFilter> = None;

    for filter in filters {
        let filter_result = build_single_filter(filter);
        result = match (result, filter_result) {
            (None, f) => f,
            (Some(existing), None) => Some(existing),
            (Some(existing), Some(new)) => image_filters::compose(new, existing),
        };
    }

    result
}

/// Build an ImageFilter from a single usvgr Filter (which has multiple primitives).
fn build_single_filter(filter: &filter::Filter) -> Option<ImageFilter> {
    let primitives = filter.primitives();
    if primitives.is_empty() {
        return None;
    }

    // Map of named results from filter primitives
    let mut results: HashMap<&str, ImageFilter> = HashMap::new();
    let mut last_result: Option<ImageFilter> = None;

    for primitive in primitives {
        let crop_rect = primitive_crop_rect(primitive);
        let image_filter = convert_primitive(primitive, &results, &last_result, crop_rect);

        if let Some(ref filter) = image_filter {
            if !primitive.result().is_empty() {
                results.insert(primitive.result(), filter.clone());
            }
            last_result = image_filter;
        }
    }

    last_result
}

fn primitive_crop_rect(primitive: &filter::Primitive) -> skia_safe::IRect {
    let rect = primitive.rect();
    skia_safe::IRect::from_xywh(
        rect.x() as i32,
        rect.y() as i32,
        rect.width() as i32,
        rect.height() as i32,
    )
}

/// Resolve an SVG filter Input reference to a Skia ImageFilter.
///
/// - `SourceGraphic` -> None (Skia treats None as the source bitmap)
/// - `SourceAlpha` -> A color matrix that extracts alpha
/// - `Reference(name)` -> lookup in the results map
fn resolve_input(
    input: &Input,
    results: &HashMap<&str, ImageFilter>,
    last_result: &Option<ImageFilter>,
) -> Option<ImageFilter> {
    match input {
        Input::SourceGraphic => None,
        Input::SourceAlpha => {
            // Extract alpha channel: multiply RGB by 0, keep A
            let matrix: [f32; 20] = [
                0.0, 0.0, 0.0, 0.0, 0.0, // R
                0.0, 0.0, 0.0, 0.0, 0.0, // G
                0.0, 0.0, 0.0, 0.0, 0.0, // B
                0.0, 0.0, 0.0, 1.0, 0.0, // A
            ];
            let cf = skia_safe::color_filters::matrix_row_major(&matrix, None);
            image_filters::color_filter(cf, None, None)
        }
        Input::Reference(name) => results
            .get(name.as_str())
            .cloned()
            .or_else(|| last_result.clone()),
    }
}

/// Convert a single usvgr filter primitive to a Skia ImageFilter.
fn convert_primitive(
    primitive: &filter::Primitive,
    results: &HashMap<&str, ImageFilter>,
    last_result: &Option<ImageFilter>,
    crop_rect: skia_safe::IRect,
) -> Option<ImageFilter> {
    match primitive.kind() {
        Kind::GaussianBlur(fe) => {
            let input = resolve_input(fe.input(), results, last_result);
            image_filters::blur(
                (fe.std_dev_x().get(), fe.std_dev_y().get()),
                None,
                input,
                crop_rect,
            )
        }

        Kind::Offset(fe) => {
            let input = resolve_input(fe.input(), results, last_result);
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
            let foreground = resolve_input(fe.input1(), results, last_result);
            let background = resolve_input(fe.input2(), results, last_result);
            let mode = convert_blend_mode(fe.mode());
            image_filters::blend(mode, background, foreground, crop_rect)
        }

        Kind::Composite(fe) => {
            // SVG `in` (input1) is the source/foreground, `in2` (input2) is the destination/background.
            // Skia's blend() signature is blend(mode, background, foreground, crop_rect).
            let foreground = resolve_input(fe.input1(), results, last_result);
            let background = resolve_input(fe.input2(), results, last_result);

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
                .map(|input| resolve_input(input, results, last_result))
                .collect();

            image_filters::merge(filters, crop_rect)
        }

        Kind::DropShadow(fe) => {
            let input = resolve_input(fe.input(), results, last_result);
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
            let input = resolve_input(fe.input(), results, last_result);

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
            let input = resolve_input(fe.input(), results, last_result);
            let cf = convert_color_matrix(fe.kind())?;
            image_filters::color_filter(cf, input, crop_rect)
        }

        Kind::Morphology(fe) => {
            let input = resolve_input(fe.input(), results, last_result);
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
            let input = resolve_input(fe.input(), results, last_result);
            let prim_rect = primitive.rect();
            let src = skia_safe::Rect::from_xywh(
                prim_rect.x(),
                prim_rect.y(),
                prim_rect.width(),
                prim_rect.height(),
            );
            image_filters::tile(src, src, input)
        }

        // Filter types not commonly used in fframes - log and skip for now
        Kind::Turbulence(_)
        | Kind::ConvolveMatrix(_)
        | Kind::DiffuseLighting(_)
        | Kind::SpecularLighting(_)
        | Kind::DisplacementMap(_)
        | Kind::Image(_) => {
            // TODO: Implement remaining filter primitives
            None
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
