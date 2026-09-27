//! Execution of `fframes::Shader` layers as Skia runtime effects.
//!
//! A shader layer reaches the renderer as an `<image>` whose pixels are a
//! placeholder registered by `Shader::draw`. Instead of drawing the
//! placeholder, the renderer builds an `SkRuntimeEffect` shader and fills the
//! element rect with it on the same canvas, so the program runs on the GPU
//! surface (or Skia's CPU raster pipeline) with the current transform, clip
//! and layer opacity applied.

use std::collections::{HashMap, HashSet};

use fframes::{ShaderDraw, ShaderUniformValue, usvgr};
use skia_safe::runtime_effect::{ChildPtr, ChildType, uniform};
use skia_safe::{Canvas, Data, Paint, Rect, RuntimeEffect};

use super::RenderCache;

/// Compile a shader to a Skia runtime effect.
///
/// The renderer does this lazily on first draw and only logs failures, so
/// call it from a test to catch SkSL errors before rendering a video:
///
/// ```rust
/// let shader = fframes::Shader::shadertoy(include_str!("tunnel.glsl"));
/// fframes_skia_renderer::render::compile_shader(&shader).unwrap();
/// ```
pub fn compile_shader(shader: &fframes::Shader) -> Result<RuntimeEffect, String> {
    RuntimeEffect::make_for_shader(shader.sksl_source(), None)
}

/// Compiled programs and "already reported" warnings, per renderer thread.
#[derive(Default)]
pub(super) struct ShaderCache {
    /// `Shader::id` → compiled effect, or `None` when compilation failed
    /// (the error was logged and the layer is skipped from then on).
    effects: HashMap<u64, Option<RuntimeEffect>>,
    warned: HashSet<(u64, String)>,
}

impl ShaderCache {
    fn effect(&mut self, shader: &fframes::Shader) -> Option<RuntimeEffect> {
        self.effects
            .entry(shader.id())
            .or_insert_with(|| {
                compile_shader(shader)
                    .map_err(|err| {
                        eprintln!("fframes: failed to compile shader #{}:\n{err}", shader.id())
                    })
                    .ok()
            })
            .clone()
    }

    fn warn_once(&mut self, shader: &fframes::Shader, message: String) {
        if self.warned.insert((shader.id(), message.clone())) {
            eprintln!("fframes: shader #{}: {message}", shader.id());
        }
    }
}

/// Fill the `<image>` element rect with the shader.
pub(super) fn render_shader(
    draw: &ShaderDraw,
    view_box: usvgr::ViewBox,
    canvas: &Canvas,
    cache: &mut RenderCache,
) {
    let rect = view_box.rect;
    let (width, height) = (rect.width(), rect.height());

    let Some(effect) = cache.shaders.effect(&draw.shader) else {
        return;
    };

    let uniforms = pack_uniforms(&effect, draw, width, height, cache);
    let Some(children) = bind_children(&effect, draw, cache) else {
        return;
    };

    let Some(shader) = effect.make_shader(Data::new_copy(&uniforms), &children, None) else {
        cache
            .shaders
            .warn_once(&draw.shader, "Skia refused to build the shader".into());
        return;
    };

    let mut paint = Paint::default();
    paint.set_shader(shader);
    paint.set_anti_alias(true);

    canvas.save();
    // Shader coordinates start at the element's top-left corner, in the
    // element's own units, so `iResolution` and `coord` agree at any scale.
    canvas.translate((rect.x(), rect.y()));
    canvas.draw_rect(Rect::from_wh(width, height), &paint);
    canvas.restore();
}

fn pack_uniforms(
    effect: &RuntimeEffect,
    draw: &ShaderDraw,
    width: f32,
    height: f32,
    cache: &mut RenderCache,
) -> Vec<u8> {
    let mut data = vec![0u8; effect.uniform_size()];

    for declared in effect.uniforms() {
        let name = declared.name();
        let value = match name {
            "iResolution" => Some(ShaderUniformValue::Float3([width, height, 1.0])),
            "iTime" => Some(ShaderUniformValue::Float(draw.time)),
            "iTimeDelta" => Some(ShaderUniformValue::Float(draw.time_delta)),
            "iFrame" => Some(ShaderUniformValue::Int(draw.frame as i32)),
            _ => draw
                .uniforms
                .iter()
                .rev()
                .find(|(user_name, _)| user_name == name)
                .map(|(_, value)| value.clone()),
        };

        let Some(value) = value else {
            // Unset uniforms stay zero, which is what Shadertoy's
            // `iMouse`/`iDate` expect. Only report user-declared ones.
            if !matches!(name, "iMouse" | "iDate") {
                cache
                    .shaders
                    .warn_once(&draw.shader, format!("uniform `{name}` is not set"));
            }
            continue;
        };

        let bytes: Vec<u8> = match (declared.ty(), &value) {
            (uniform::Type::Float, ShaderUniformValue::Float(v)) => v.to_ne_bytes().to_vec(),
            (uniform::Type::Float2, ShaderUniformValue::Float2(v)) => f32_bytes(v),
            (uniform::Type::Float3, ShaderUniformValue::Float3(v)) => f32_bytes(v),
            (uniform::Type::Float4, ShaderUniformValue::Float4(v)) => f32_bytes(v),
            (uniform::Type::Int, ShaderUniformValue::Int(v)) => v.to_ne_bytes().to_vec(),
            // An `int` built-in declared as `float` (common in pasted code).
            (uniform::Type::Float, ShaderUniformValue::Int(v)) => {
                (*v as f32).to_ne_bytes().to_vec()
            }
            (ty, value) => {
                cache.shaders.warn_once(
                    &draw.shader,
                    format!("uniform `{name}` is declared as {ty:?} but got {value:?}"),
                );
                continue;
            }
        };

        let offset = declared.offset();
        if let Some(slot) = data.get_mut(offset..offset + bytes.len()) {
            slot.copy_from_slice(&bytes);
        }
    }

    data
}

fn f32_bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_ne_bytes()).collect()
}

/// Every `uniform shader` child must be bound; missing images are bound to a
/// transparent shader so the rest of the program still runs.
fn bind_children(
    effect: &RuntimeEffect,
    draw: &ShaderDraw,
    cache: &mut RenderCache,
) -> Option<Vec<ChildPtr>> {
    let mut children = Vec::with_capacity(effect.children().len());

    for child in effect.children() {
        if child.ty() != ChildType::Shader {
            cache.shaders.warn_once(
                &draw.shader,
                format!(
                    "child `{}`: only `uniform shader` children are supported",
                    child.name()
                ),
            );
            return None;
        }

        let pixels = draw
            .uniforms
            .iter()
            .rev()
            .find_map(|(name, value)| match value {
                ShaderUniformValue::Image(pixels) if name == child.name() => Some(pixels),
                _ => None,
            });

        let image_shader = pixels.and_then(|pixels| {
            cache.image(pixels).and_then(|image| {
                image.image().to_shader(
                    (skia_safe::TileMode::Clamp, skia_safe::TileMode::Clamp),
                    skia_safe::SamplingOptions::new(
                        skia_safe::FilterMode::Linear,
                        skia_safe::MipmapMode::None,
                    ),
                    None,
                )
            })
        });

        let shader = image_shader.unwrap_or_else(|| {
            cache.shaders.warn_once(
                &draw.shader,
                format!("child `{}` has no image bound", child.name()),
            );
            skia_safe::shaders::color(skia_safe::Color::TRANSPARENT)
        });

        children.push(ChildPtr::Shader(shader));
    }

    Some(children)
}
