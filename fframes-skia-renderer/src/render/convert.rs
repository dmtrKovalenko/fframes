//! Type conversions from usvgr types to Skia types.

use fframes::usvgr;
use fframes::usvgr::tiny_skia_path;
use skia_safe::{self, Paint, PathBuilder};

/// Convert a tiny_skia_path::Path to a skia_safe::Path.
pub fn convert_path(path: &tiny_skia_path::Path) -> skia_safe::Path {
    let mut builder = PathBuilder::new();

    for segment in path.segments() {
        match segment {
            tiny_skia_path::PathSegment::MoveTo(p) => {
                builder.move_to((p.x, p.y));
            }
            tiny_skia_path::PathSegment::LineTo(p) => {
                builder.line_to((p.x, p.y));
            }
            tiny_skia_path::PathSegment::QuadTo(p1, p2) => {
                builder.quad_to((p1.x, p1.y), (p2.x, p2.y));
            }
            tiny_skia_path::PathSegment::CubicTo(p1, p2, p3) => {
                builder.cubic_to((p1.x, p1.y), (p2.x, p2.y), (p3.x, p3.y));
            }
            tiny_skia_path::PathSegment::Close => {
                builder.close();
            }
        }
    }

    builder.detach()
}

/// Convert a usvgr Paint + opacity to a Skia Paint.
///
/// Returns None if the paint cannot be created (e.g. degenerate gradient).
pub fn to_skia_paint(
    paint: &usvgr::Paint,
    opacity: usvgr::Opacity,
    anti_alias: bool,
    cache: &mut super::RenderCache,
) -> Option<Paint> {
    let mut sk_paint = Paint::default();
    sk_paint.set_anti_alias(anti_alias);

    match paint {
        usvgr::Paint::Color(c) => {
            sk_paint.set_color(skia_safe::Color::from_argb(
                opacity.to_u8(),
                c.red,
                c.green,
                c.blue,
            ));
        }
        usvgr::Paint::LinearGradient(lg) => {
            let shader = convert_linear_gradient(lg, opacity)?;
            sk_paint.set_shader(shader);
        }
        usvgr::Paint::RadialGradient(rg) => {
            let shader = convert_radial_gradient(rg, opacity)?;
            sk_paint.set_shader(shader);
        }
        usvgr::Paint::Pattern(pattern) => {
            let shader = super::render_pattern_shader(pattern, cache)?;
            sk_paint.set_shader(shader);
            // fill-opacity / stroke-opacity multiplies the pattern content.
            sk_paint.set_alpha_f(opacity.get());
        }
    }

    Some(sk_paint)
}

/// Convert a usvgr Stroke to a Skia Paint configured for stroking.
pub fn to_skia_stroke_paint(
    stroke: &usvgr::Stroke,
    anti_alias: bool,
    cache: &mut super::RenderCache,
) -> Option<Paint> {
    let mut paint = to_skia_paint(stroke.paint(), stroke.opacity(), anti_alias, cache)?;
    paint.set_style(skia_safe::PaintStyle::Stroke);
    paint.set_stroke_width(stroke.width().get());
    paint.set_stroke_cap(convert_line_cap(stroke.linecap()));
    paint.set_stroke_join(convert_line_join(stroke.linejoin()));
    paint.set_stroke_miter(stroke.miterlimit().get());

    // Dash pattern
    if let Some(dasharray) = stroke.dasharray() {
        let effect = skia_safe::PathEffect::dash(dasharray, stroke.dashoffset());
        paint.set_path_effect(effect);
    }

    Some(paint)
}

fn convert_linear_gradient(
    gradient: &usvgr::LinearGradient,
    opacity: usvgr::Opacity,
) -> Option<skia_safe::Shader> {
    let (colors, positions) = convert_gradient_stops(gradient, opacity);
    let mode = convert_spread_method(gradient.spread_method());
    let transform = super::to_matrix(gradient.transform());

    let start = skia_safe::Point::new(gradient.x1(), gradient.y1());
    let end = skia_safe::Point::new(gradient.x2(), gradient.y2());

    skia_safe::Shader::linear_gradient(
        (start, end),
        skia_safe::gradient_shader::GradientShaderColors::Colors(&colors),
        Some(positions.as_slice()),
        mode,
        None,
        Some(&transform),
    )
}

fn convert_radial_gradient(
    gradient: &usvgr::RadialGradient,
    opacity: usvgr::Opacity,
) -> Option<skia_safe::Shader> {
    let (colors, positions) = convert_gradient_stops(gradient, opacity);
    let mode = convert_spread_method(gradient.spread_method());
    let transform = super::to_matrix(gradient.transform());

    let center = skia_safe::Point::new(gradient.cx(), gradient.cy());
    let focal = skia_safe::Point::new(gradient.fx(), gradient.fy());

    skia_safe::Shader::two_point_conical_gradient(
        focal,
        0.0,
        center,
        gradient.r().get(),
        skia_safe::gradient_shader::GradientShaderColors::Colors(&colors),
        Some(positions.as_slice()),
        mode,
        None,
        Some(&transform),
    )
}

fn convert_gradient_stops(
    gradient: &usvgr::BaseGradient,
    opacity: usvgr::Opacity,
) -> (Vec<skia_safe::Color>, Vec<f32>) {
    let mut colors = Vec::with_capacity(gradient.stops().len());
    let mut positions = Vec::with_capacity(gradient.stops().len());

    for stop in gradient.stops() {
        let alpha = stop.opacity() * opacity;
        colors.push(skia_safe::Color::from_argb(
            alpha.to_u8(),
            stop.color().red,
            stop.color().green,
            stop.color().blue,
        ));
        positions.push(stop.offset().get());
    }

    (colors, positions)
}

fn convert_spread_method(method: usvgr::SpreadMethod) -> skia_safe::TileMode {
    match method {
        usvgr::SpreadMethod::Pad => skia_safe::TileMode::Clamp,
        usvgr::SpreadMethod::Reflect => skia_safe::TileMode::Mirror,
        usvgr::SpreadMethod::Repeat => skia_safe::TileMode::Repeat,
    }
}

fn convert_line_cap(cap: usvgr::LineCap) -> skia_safe::paint::Cap {
    match cap {
        usvgr::LineCap::Butt => skia_safe::paint::Cap::Butt,
        usvgr::LineCap::Round => skia_safe::paint::Cap::Round,
        usvgr::LineCap::Square => skia_safe::paint::Cap::Square,
    }
}

fn convert_line_join(join: usvgr::LineJoin) -> skia_safe::paint::Join {
    match join {
        usvgr::LineJoin::Miter | usvgr::LineJoin::MiterClip => skia_safe::paint::Join::Miter,
        usvgr::LineJoin::Round => skia_safe::paint::Join::Round,
        usvgr::LineJoin::Bevel => skia_safe::paint::Join::Bevel,
    }
}

/// Convert usvgr BlendMode to Skia BlendMode.
pub fn convert_blend_mode(mode: usvgr::BlendMode) -> skia_safe::BlendMode {
    match mode {
        usvgr::BlendMode::Normal => skia_safe::BlendMode::SrcOver,
        usvgr::BlendMode::Multiply => skia_safe::BlendMode::Multiply,
        usvgr::BlendMode::Screen => skia_safe::BlendMode::Screen,
        usvgr::BlendMode::Overlay => skia_safe::BlendMode::Overlay,
        usvgr::BlendMode::Darken => skia_safe::BlendMode::Darken,
        usvgr::BlendMode::Lighten => skia_safe::BlendMode::Lighten,
        usvgr::BlendMode::ColorDodge => skia_safe::BlendMode::ColorDodge,
        usvgr::BlendMode::ColorBurn => skia_safe::BlendMode::ColorBurn,
        usvgr::BlendMode::HardLight => skia_safe::BlendMode::HardLight,
        usvgr::BlendMode::SoftLight => skia_safe::BlendMode::SoftLight,
        usvgr::BlendMode::Difference => skia_safe::BlendMode::Difference,
        usvgr::BlendMode::Exclusion => skia_safe::BlendMode::Exclusion,
        usvgr::BlendMode::Hue => skia_safe::BlendMode::Hue,
        usvgr::BlendMode::Saturation => skia_safe::BlendMode::Saturation,
        usvgr::BlendMode::Color => skia_safe::BlendMode::Color,
        usvgr::BlendMode::Luminosity => skia_safe::BlendMode::Luminosity,
    }
}
