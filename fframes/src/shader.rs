use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::media::ImageData;
use crate::{Color, Frame};

static NEXT_SHADER_ID: AtomicU64 = AtomicU64::new(1);

/// The language the shader source was written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderLanguage {
    /// Plain SkSL with a `half4 main(float2 coord)` entry point.
    Sksl,
    /// Shadertoy GLSL with a `void mainImage(out vec4, in vec2)` entry point,
    /// translated to SkSL by [`Shader::shadertoy`].
    Shadertoy,
}

/// A compiled-once GPU shader definition. Cheap to clone.
///
/// Create it once (in the video constructor or a `OnceLock`) and call
/// [`Shader::draw`] from `render_frame`. The renderer compiles and caches the
/// program the first time it is drawn; compilation errors are logged once and
/// the layer is skipped. Use `fframes_skia_renderer::compile_shader` in a test
/// to validate a shader ahead of rendering.
#[derive(Clone)]
pub struct Shader {
    inner: Arc<ShaderInner>,
}

struct ShaderInner {
    id: u64,
    language: ShaderLanguage,
    sksl: String,
}

impl std::fmt::Debug for Shader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shader")
            .field("id", &self.inner.id)
            .field("language", &self.inner.language)
            .finish()
    }
}

impl Shader {
    /// A shader written in SkSL, Skia's shading language (GLSL ES 2 with
    /// `float2`/`half4` style types; `vec2`/`vec4`/`mat3` are accepted too).
    ///
    /// ```glsl
    /// uniform float3 iResolution;
    /// uniform float iTime;
    ///
    /// half4 main(float2 coord) {
    ///     float2 uv = coord / iResolution.xy;
    ///     return half4(uv, 0.5 + 0.5 * sin(iTime), 1.0);
    /// }
    /// ```
    ///
    /// The returned color is premultiplied alpha.
    pub fn sksl(source: impl Into<String>) -> Self {
        Self::new(ShaderLanguage::Sksl, source.into())
    }

    /// A shader pasted from Shadertoy: `void mainImage(out vec4 fragColor, in vec2 fragCoord)`.
    ///
    /// `iResolution`, `iTime`, `iTimeDelta`, `iFrame`, `iMouse` and `iDate` are
    /// declared for you, `fragCoord` has its origin at the bottom-left like on
    /// Shadertoy, and the output is opaque (Shadertoy ignores alpha).
    ///
    /// Translation is textual, so the shader must stay within what SkSL
    /// supports: object-like `#define NAME value` macros are expanded,
    /// `precision` statements are dropped, but function-like macros,
    /// `while` loops, non-constant loop bounds and `texture()` are not
    /// available. To sample an image declare `uniform shader iChannel0;` and
    /// call `iChannel0.eval(uv * iResolution.xy)`, then pass the image with
    /// [`ShaderUniforms::image`].
    pub fn shadertoy(source: impl AsRef<str>) -> Self {
        Self::new(
            ShaderLanguage::Shadertoy,
            shadertoy_to_sksl(source.as_ref()),
        )
    }

    fn new(language: ShaderLanguage, sksl: String) -> Self {
        Self {
            inner: Arc::new(ShaderInner {
                id: NEXT_SHADER_ID.fetch_add(1, Ordering::Relaxed),
                language,
                sksl,
            }),
        }
    }

    /// Unique id of this shader, stable for its lifetime. Renderers key their
    /// compiled program caches by it.
    pub fn id(&self) -> u64 {
        self.inner.id
    }

    pub fn language(&self) -> ShaderLanguage {
        self.inner.language
    }

    /// The final SkSL source handed to Skia (after the Shadertoy translation).
    pub fn sksl_source(&self) -> &str {
        &self.inner.sksl
    }

    /// Queue this shader for the current frame and get an image to place with
    /// `<image href={layer.href()} x y width height />`.
    ///
    /// The element's `width`/`height` define the area the shader covers and
    /// the value of `iResolution`; `preserveAspectRatio` is ignored.
    pub fn draw(&self, frame: &Frame, uniforms: ShaderUniforms) -> ImageData<'static> {
        let draw = ShaderDraw {
            shader: self.clone(),
            uniforms: uniforms.values,
            time: frame.seconds(),
            time_delta: 1.0 / frame.fps.max(1) as f32,
            frame: frame.index,
        };

        registry::create_marker(draw)
    }
}

/// A single value bound to a named `uniform` of a [`Shader`].
#[derive(Debug, Clone)]
pub enum ShaderUniformValue {
    Float(f32),
    Float2([f32; 2]),
    Float3([f32; 3]),
    Float4([f32; 4]),
    Int(i32),
    /// A child `uniform shader` sampled with `.eval(coord)` in image pixels.
    #[cfg(not(target_arch = "wasm32"))]
    Image(Arc<usvgr::PreloadedImageData>),
}

/// Named uniform values for one [`Shader::draw`] call.
///
/// Values whose name is not declared by the shader, or whose type does not
/// match the declaration, are ignored (and logged once by the renderer).
#[derive(Debug, Clone, Default)]
pub struct ShaderUniforms {
    values: Vec<(Cow<'static, str>, ShaderUniformValue)>,
}

impl ShaderUniforms {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(mut self, name: impl Into<Cow<'static, str>>, value: ShaderUniformValue) -> Self {
        self.values.push((name.into(), value));
        self
    }

    /// `uniform float name;`
    pub fn float(self, name: impl Into<Cow<'static, str>>, value: f32) -> Self {
        self.set(name, ShaderUniformValue::Float(value))
    }

    /// `uniform float2 name;`
    pub fn float2(self, name: impl Into<Cow<'static, str>>, x: f32, y: f32) -> Self {
        self.set(name, ShaderUniformValue::Float2([x, y]))
    }

    /// `uniform float3 name;`
    pub fn float3(self, name: impl Into<Cow<'static, str>>, x: f32, y: f32, z: f32) -> Self {
        self.set(name, ShaderUniformValue::Float3([x, y, z]))
    }

    /// `uniform float4 name;`
    pub fn float4(
        self,
        name: impl Into<Cow<'static, str>>,
        x: f32,
        y: f32,
        z: f32,
        w: f32,
    ) -> Self {
        self.set(name, ShaderUniformValue::Float4([x, y, z, w]))
    }

    /// `uniform int name;`
    pub fn int(self, name: impl Into<Cow<'static, str>>, value: i32) -> Self {
        self.set(name, ShaderUniformValue::Int(value))
    }

    /// `uniform float4 name;` (or `half4`) as unpremultiplied RGBA in `0..1`.
    /// Pair it with animated colors: `.color("uTint", frame.animate(&tint))`.
    pub fn color(self, name: impl Into<Cow<'static, str>>, color: Color) -> Self {
        self.float4(
            name,
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
            color.a as f32 / 255.0,
        )
    }

    /// `uniform shader name;` bound to an image, e.g. `ctx.get_image(..)` or a
    /// synced video frame. Sample it with `name.eval(coord)` where `coord` is
    /// in the image's pixels. Ignored in the editor.
    #[allow(unused_variables)]
    pub fn image(self, name: impl Into<Cow<'static, str>>, image: &ImageData<'_>) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        return self.set(name, ShaderUniformValue::Image(image.href()));
        #[cfg(target_arch = "wasm32")]
        return self;
    }

    pub fn values(&self) -> &[(Cow<'static, str>, ShaderUniformValue)] {
        &self.values
    }
}

/// Everything a renderer needs to execute one queued [`Shader::draw`].
#[derive(Debug, Clone)]
pub struct ShaderDraw {
    pub shader: Shader,
    pub uniforms: Vec<(Cow<'static, str>, ShaderUniformValue)>,
    /// Seconds since the start of the video or scene (`iTime`).
    pub time: f32,
    /// Duration of one frame in seconds (`iTimeDelta`).
    pub time_delta: f32,
    /// Frame index (`iFrame`).
    pub frame: usize,
}

/// Returns the shader draw request if `image` is a placeholder created by
/// [`Shader::draw`]. Called by rendering backends for every raster image.
#[cfg(not(target_arch = "wasm32"))]
pub fn resolve_shader_draw(image: &Arc<usvgr::PreloadedImageData>) -> Option<Arc<ShaderDraw>> {
    registry::resolve(image)
}

#[cfg(not(target_arch = "wasm32"))]
mod registry {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex, Weak};

    use super::ShaderDraw;
    use crate::media::{ImageData, ImageMetadata};

    const MARKER_PREFIX: &str = "fframes-shader:";
    /// A single transparent pixel: renderers that do not execute shaders draw
    /// nothing in their place.
    static MARKER_PIXEL: [u8; 4] = [0, 0, 0, 0];
    /// Dead entries are swept every this many insertions.
    const SWEEP_INTERVAL: usize = 256;

    struct Entry {
        image: Weak<usvgr::PreloadedImageData>,
        draw: Arc<ShaderDraw>,
    }

    #[derive(Default)]
    struct Registry {
        entries: HashMap<usize, Entry>,
        inserts: usize,
    }

    static REGISTRY: Mutex<Option<Registry>> = Mutex::new(None);
    static NEXT_MARKER: AtomicU64 = AtomicU64::new(0);

    pub(super) fn create_marker(draw: ShaderDraw) -> ImageData<'static> {
        // Unique per draw so no id-keyed cache can hand a previous frame's
        // placeholder (and so its uniforms) to this one.
        let sequence = NEXT_MARKER.fetch_add(1, Ordering::Relaxed);
        let id = format!("{MARKER_PREFIX}{}:{sequence}", draw.shader.id());
        let image = Arc::new(usvgr::PreloadedImageData {
            data: std::borrow::Cow::Borrowed(&MARKER_PIXEL),
            width: 1,
            height: 1,
            id: id.clone(),
        });

        let mut guard = REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
        let registry = guard.get_or_insert_with(Registry::default);
        registry.inserts += 1;
        if registry.inserts.is_multiple_of(SWEEP_INTERVAL) {
            registry
                .entries
                .retain(|_, entry| entry.image.strong_count() > 0);
        }
        registry.entries.insert(
            Arc::as_ptr(&image) as usize,
            Entry {
                image: Arc::downgrade(&image),
                draw: Arc::new(draw),
            },
        );
        drop(guard);

        ImageData::new_from_raw_data(
            image,
            id,
            ImageMetadata {
                width: 1,
                height: 1,
            },
        )
    }

    pub(super) fn resolve(image: &Arc<usvgr::PreloadedImageData>) -> Option<Arc<ShaderDraw>> {
        // Cheap reject for every regular image without touching the lock.
        if !image.id.starts_with(MARKER_PREFIX) {
            return None;
        }

        let guard = REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
        let entry = guard
            .as_ref()?
            .entries
            .get(&(Arc::as_ptr(image) as usize))?;

        Weak::ptr_eq(&entry.image, &Arc::downgrade(image)).then(|| Arc::clone(&entry.draw))
    }
}

#[cfg(target_arch = "wasm32")]
mod registry {
    use super::ShaderDraw;
    use crate::media::{ImageData, ImageMetadata, OwnedSharedString};

    /// Shown in the editor, which previews frames as DOM SVG and can not run
    /// Skia shaders.
    const PLACEHOLDER: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 160 90' preserveAspectRatio='none'%3E%3Cdefs%3E%3ClinearGradient id='g' x1='0' y1='0' x2='1' y2='1'%3E%3Cstop offset='0' stop-color='%231e1b4b'/%3E%3Cstop offset='1' stop-color='%23701a75'/%3E%3C/linearGradient%3E%3C/defs%3E%3Crect width='160' height='90' fill='url(%23g)'/%3E%3Ctext x='80' y='48' fill='%23e9d5ff' font-family='sans-serif' font-size='8' text-anchor='middle'%3EGPU shader (render to preview)%3C/text%3E%3C/svg%3E";

    pub(super) fn create_marker(draw: ShaderDraw) -> ImageData<'static> {
        ImageData::new_from_web_source(
            OwnedSharedString::BorrowedStatic(PLACEHOLDER),
            PLACEHOLDER.to_owned(),
            format!("fframes-shader:{}", draw.shader.id()),
            ImageMetadata {
                width: 160,
                height: 90,
            },
        )
    }
}

/// Declarations prepended to every Shadertoy shader.
const SHADERTOY_PRELUDE: &str = "\
uniform float3 iResolution;
uniform float iTime;
uniform float iTimeDelta;
uniform int iFrame;
uniform float4 iMouse;
uniform float4 iDate;
";

/// Entry point appended to every Shadertoy shader. SkSL coordinates start at
/// the top-left, Shadertoy's `fragCoord` at the bottom-left.
const SHADERTOY_MAIN: &str = "
half4 main(float2 fframesCoord) {
    float4 fframesColor = float4(0.0, 0.0, 0.0, 1.0);
    mainImage(fframesColor, float2(fframesCoord.x, iResolution.y - fframesCoord.y));
    return half4(half3(clamp(fframesColor.rgb, 0.0, 1.0)), 1.0);
}
";

/// Translates Shadertoy GLSL to SkSL as text. SkSL has no preprocessor, so
/// this drops `precision` statements and expands object-like `#define`s.
fn shadertoy_to_sksl(source: &str) -> String {
    let mut defines: Vec<(String, String)> = Vec::new();
    let mut body = String::with_capacity(source.len());

    for line in source.lines() {
        let trimmed = line.trim_start();

        if let Some(define) = trimmed.strip_prefix("#define") {
            let define = define.trim();
            let name_end = define
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(define.len());
            let (name, value) = define.split_at(name_end);
            // Function-like macros are left in place so SkSL reports them
            // with a line number instead of being silently mistranslated.
            if !name.is_empty() && !value.starts_with('(') {
                let value = value.split("//").next().unwrap_or("").trim();
                defines.push((name.to_owned(), expand_defines(value, &defines)));
                body.push('\n');
                continue;
            }
        }

        if trimmed.starts_with("precision ") {
            body.push('\n');
            continue;
        }

        body.push_str(&expand_defines(line, &defines));
        body.push('\n');
    }

    format!("{SHADERTOY_PRELUDE}{body}{SHADERTOY_MAIN}")
}

/// Replaces whole identifiers matching a define name with its value.
fn expand_defines(line: &str, defines: &[(String, String)]) -> String {
    if defines.is_empty() {
        return line.to_owned();
    }

    let mut out = String::with_capacity(line.len());
    let mut chars = line.char_indices().peekable();
    while let Some((start, c)) = chars.next() {
        if c.is_ascii_alphabetic() || c == '_' {
            let mut end = start + c.len_utf8();
            while let Some(&(i, next)) = chars.peek() {
                if next.is_ascii_alphanumeric() || next == '_' {
                    end = i + next.len_utf8();
                    chars.next();
                } else {
                    break;
                }
            }
            let ident = &line[start..end];
            match defines.iter().rev().find(|(name, _)| name == ident) {
                Some((_, value)) => out.push_str(value),
                None => out.push_str(ident),
            }
        } else if c.is_ascii_digit() {
            // Numbers like `1e5` or `0x1F` must not have their suffix expanded.
            out.push(c);
            while let Some(&(_, next)) = chars.peek() {
                if next.is_ascii_alphanumeric() || next == '_' || next == '.' {
                    out.push(next);
                    chars.next();
                } else {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadertoy_expands_object_like_defines() {
        let sksl = shadertoy_to_sksl(
            "#define PI 3.14159\n#define TAU (2.0 * PI) // full turn\nprecision highp float;\nfloat a = TAU * PI2 + PI;",
        );

        assert!(sksl.contains("float a = (2.0 * 3.14159) * PI2 + 3.14159;"));
        assert!(!sksl.contains("#define"));
        assert!(!sksl.contains("precision"));
        assert!(sksl.contains("mainImage(fframesColor"));
    }

    #[test]
    fn shadertoy_keeps_function_like_macros() {
        let sksl = shadertoy_to_sksl("#define SQ(x) ((x)*(x))\nfloat b = SQ(2.0);");
        assert!(sksl.contains("#define SQ(x)"));
    }

    #[test]
    fn shadertoy_does_not_touch_number_suffixes() {
        let sksl = shadertoy_to_sksl("#define e 2.0\nfloat c = 1e5 + e;");
        assert!(sksl.contains("float c = 1e5 + 2.0;"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn markers_resolve_only_while_the_image_lives() {
        let shader = Shader::sksl("half4 main(float2 p) { return half4(1); }");
        let frame = Frame::new(12, 12, 30);
        let image = shader.draw(&frame, ShaderUniforms::new().float("uX", 2.0));

        let href = image.href();
        let draw = resolve_shader_draw(&href).expect("marker resolves");
        assert_eq!(draw.shader.id(), shader.id());
        assert_eq!(draw.frame, 12);
        assert!((draw.time - 0.4).abs() < 1e-6);

        let unrelated = Arc::new(usvgr::PreloadedImageData::new("x".into(), 1, 1, &[0; 4]));
        assert!(resolve_shader_draw(&unrelated).is_none());

        let weak = Arc::downgrade(&href);
        drop(href);
        drop(image);
        assert!(weak.upgrade().is_none());
    }
}
