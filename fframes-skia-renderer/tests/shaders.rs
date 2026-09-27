use std::sync::Arc;

use fframes::media::{ImageData, ImageMetadata};
use fframes::{Color, Frame, Shader, ShaderUniforms, Svgr, usvgr};
use fframes_skia_renderer::render::{RenderCache, compile_shader, render_tree};
use skia_safe::{AlphaType, ColorType, ImageInfo, Surface};

const SIZE: i32 = 100;

struct Canvas {
    surface: Surface,
    info: ImageInfo,
    cache: RenderCache,
}

impl Canvas {
    fn new() -> Self {
        let info = ImageInfo::new(
            (SIZE, SIZE),
            ColorType::RGBA8888,
            AlphaType::Premul,
            skia_safe::ColorSpace::new_srgb(),
        );
        let surface = skia_safe::surfaces::raster(&info, None, None).expect("raster surface");
        Self {
            surface,
            info,
            cache: RenderCache::new(),
        }
    }

    fn render(&mut self, svgr: Svgr) {
        let tree = svgr
            .into_svg_tree(
                &usvgr::Options::default(),
                &mut usvgr::Cache::default(),
                &usvgr::fontdb::Database::new(),
            )
            .expect("valid svgr");

        self.surface.canvas().clear(skia_safe::Color::WHITE);
        render_tree(&tree, self.surface.canvas(), &mut self.cache);
    }

    fn pixel(&mut self, x: i32, y: i32) -> [u8; 4] {
        let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];
        assert!(
            self.surface
                .read_pixels(&self.info, &mut pixels, (SIZE * 4) as usize, (0, 0))
        );
        let i = ((y * SIZE + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    }
}

fn assert_near(actual: [u8; 4], expected: [u8; 4]) {
    let close = actual
        .iter()
        .zip(expected)
        .all(|(a, e)| (*a as i32 - e as i32).abs() <= 4);
    assert!(close, "expected ~{expected:?}, got {actual:?}");
}

#[test]
fn sksl_gradient_uses_element_resolution() {
    let shader = Shader::sksl(
        "uniform float3 iResolution;
         half4 main(float2 coord) {
             return half4(coord.x / iResolution.x, 0.0, 0.0, 1.0);
         }",
    );
    let layer = shader.draw(&Frame::new(0, 0, 30), ShaderUniforms::new());

    let mut canvas = Canvas::new();
    canvas.render(fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <image href={layer.href()} x="0" y="0" width="100" height="100" />
        </svg>
    ));

    assert!(canvas.pixel(1, 50)[0] < 8);
    assert!(canvas.pixel(98, 50)[0] > 245);
    assert_eq!(canvas.pixel(50, 50)[3], 255);
}

#[test]
fn user_uniforms_transforms_and_opacity_apply() {
    let shader = Shader::sksl(
        "uniform float4 uTint;
         half4 main(float2 coord) { return half4(uTint); }",
    );
    let layer = shader.draw(
        &Frame::new(0, 0, 30),
        ShaderUniforms::new().color("uTint", Color::hex("#0000ff")),
    );

    let mut canvas = Canvas::new();
    canvas.render(fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <g transform="translate(50 0)" opacity="0.5">
                <image href={layer.href()} x="0" y="0" width="50" height="100" />
            </g>
        </svg>
    ));

    // Left half is untouched, right half is blue at half opacity over white.
    assert_near(canvas.pixel(25, 50), [255, 255, 255, 255]);
    assert_near(canvas.pixel(75, 50), [128, 128, 255, 255]);
}

#[test]
fn shadertoy_builtins_defines_and_bottom_left_origin() {
    let shader = Shader::shadertoy(
        "#define GREEN 1.0
         void mainImage(out vec4 fragColor, in vec2 fragCoord) {
             vec2 uv = fragCoord / iResolution.xy;
             fragColor = vec4(fract(iTime), uv.y * GREEN, 0.0, 0.25);
         }",
    );
    compile_shader(&shader).expect("shadertoy shader compiles");

    // frame 15 at 30 fps: iTime = 0.5
    let layer = shader.draw(&Frame::new(15, 15, 30), ShaderUniforms::new());

    let mut canvas = Canvas::new();
    canvas.render(fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <image href={layer.href()} x="0" y="0" width="100" height="100" />
        </svg>
    ));

    let top = canvas.pixel(50, 1);
    let bottom = canvas.pixel(50, 98);
    // Shadertoy's y axis points up, and its alpha is ignored.
    assert!(
        top[1] > 245 && bottom[1] < 8,
        "top {top:?}, bottom {bottom:?}"
    );
    assert!((top[0] as i32 - 128).abs() <= 2, "iTime = 0.5, got {top:?}");
    assert_eq!(top[3], 255);
}

#[test]
fn image_children_are_sampled() {
    let red = Arc::new(usvgr::PreloadedImageData::new(
        "red.png".into(),
        2,
        2,
        &[255, 0, 0, 255].repeat(4),
    ));
    let image = ImageData::new_from_raw_data(
        red,
        "red.png".into(),
        ImageMetadata {
            width: 2,
            height: 2,
        },
    );

    let shader = Shader::sksl(
        "uniform shader iChannel0;
         half4 main(float2 coord) { return iChannel0.eval(float2(1.0)).bgra; }",
    );
    let layer = shader.draw(
        &Frame::new(0, 0, 30),
        ShaderUniforms::new().image("iChannel0", &image),
    );

    let mut canvas = Canvas::new();
    canvas.render(fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <image href={layer.href()} x="0" y="0" width="100" height="100" />
        </svg>
    ));

    // Swizzled to blue, proving the pixels went through the program.
    assert_near(canvas.pixel(50, 50), [0, 0, 255, 255]);
}

#[test]
fn invalid_shaders_report_errors_and_are_skipped() {
    let shader = Shader::sksl("half4 main(float2 coord) { return nope; }");
    let error = compile_shader(&shader).expect_err("invalid sksl");
    assert!(error.contains("nope"), "{error}");

    let layer = shader.draw(&Frame::new(0, 0, 30), ShaderUniforms::new());
    let mut canvas = Canvas::new();
    canvas.render(fframes::svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100">
            <image href={layer.href()} x="0" y="0" width="100" height="100" />
        </svg>
    ));
    assert_near(canvas.pixel(50, 50), [255, 255, 255, 255]);
}
