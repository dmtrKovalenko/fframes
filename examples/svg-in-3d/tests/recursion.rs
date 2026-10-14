//! svg -> 3D -> svg -> 3D -> frame. An svgr image on a quad (scene A), the render of A in a
//! second svgr image (B), B on a quad (scene C), the render of C in the frame. The pixels
//! of the frame land where the projections of the cameras put them, on every backend.
#![allow(clippy::unreadable_literal)]

use std::sync::atomic::{AtomicBool, Ordering};

use fframes::media::ImageData;
use fframes::three_d::*;
use fframes::{
    AudioMap, Color, Duration, FFramesContext, Frame, RenderOptions, StaticMediaProvider, Svgr,
    Video, svgr,
};
use fframes_skia_renderer::{SkiaCpuCtx, SkiaFFramesRenderer, SkiaPipelineConfig};
use svg_in_3d_example::SvgIn3dMedia;

const FRAME: u32 = 640;
/// The size of the innermost image and of the render of A.
const INNER: u32 = 256;
/// The size of B and of the render of C.
const OUTER: u32 = 512;
/// Where A is in B, and where C is in the frame.
const A_IN_B: f32 = 128.0;
const C_IN_FRAME: f32 = 64.0;

#[derive(Debug)]
struct Nested {
    gpu_3d: bool,
    /// Set when B is a texture on the GPU (`fframes::wgpu_layer`).
    b_on_gpu: AtomicBool,
}

impl Nested {
    fn new(gpu_3d: bool) -> Self {
        Self {
            gpu_3d,
            b_on_gpu: AtomicBool::new(false),
        }
    }

    /// `image` on a quad that the camera sees head on, rendered at `size` x `size`.
    fn quad(&self, image: &ImageData<'_>, size: u32) -> ImageData<'static> {
        let mut scene = Scene::new();
        scene.add(Node::mesh(
            Geometry::plane(2.0, 2.0),
            Material::unlit(0xffffff).map(Texture::from_image(image)),
        ));
        let renderer = Renderer::new(size, size).tone_mapping(ToneMapping::Linear);
        let renderer = if self.gpu_3d {
            renderer.gpu()
        } else {
            renderer.cpu()
        };
        renderer.render(&scene, &camera())
    }
}

fn camera() -> Camera {
    Camera::perspective(40.0)
        .at(0.0, 0.0, 3.0)
        .look_at(Vec3::ZERO)
}

/// The pixel of a quad render of `size` that shows texture coordinate `uv`.
fn on_quad(uv: Vec2, size: u32) -> Vec2 {
    // `Geometry::plane` has UV (0, 0) at the top left corner (-1, 1)
    let world = vec3(-1.0 + 2.0 * uv.x, 1.0 - 2.0 * uv.y, 0.0);
    camera()
        .project(world, size as f32, size as f32)
        .expect("the quad is in front of the camera")
        .truncate()
}

/// The pixel of the frame that shows pixel `p` of B.
fn b_in_frame(p: Vec2) -> Vec2 {
    on_quad(p / OUTER as f32, OUTER) + C_IN_FRAME
}

/// The pixel of the frame that shows pixel `p` of the innermost image.
fn inner_in_frame(p: Vec2) -> Vec2 {
    b_in_frame(on_quad(p / INNER as f32, INNER) + A_IN_B)
}

impl Video for Nested {
    const FPS: usize = 30;
    const WIDTH: usize = FRAME as usize;
    const HEIGHT: usize = FRAME as usize;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(1)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, _frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let inner = ctx
            .render_svgr(
                svgr!(
                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">
                        <rect x="0" y="0" width="256" height="256" fill="#ffffff" />
                        <rect x="64" y="64" width="64" height="64" fill="#ff0000" />
                        <text x="140" y="236" font-family="DM Sans" font-size="110" fill="#000000">"F"</text>
                    </svg>
                ),
                INNER,
                INNER,
            )
            .expect("the innermost image renders");
        let a = self.quad(&inner, INNER);
        let b = ctx
            .render_svgr(
                svgr!(
                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" width="512" height="512">
                        <rect x="0" y="0" width="512" height="512" fill="#808080" />
                        <image href={a.href()} x="128" y="128" width="256" height="256" />
                        <rect x="16" y="16" width="64" height="64" fill="#00ff00" />
                    </svg>
                ),
                OUTER,
                OUTER,
            )
            .expect("B renders");
        #[cfg(not(target_os = "macos"))]
        self.b_on_gpu.store(
            fframes::resolve_wgpu_layer(&b.href()).is_some(),
            Ordering::Relaxed,
        );
        let c = self.quad(&b, OUTER);
        svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640" width="640" height="640">
                <image href={c.href()} x="64" y="64" width="512" height="512" />
            </svg>
        )
    }
}

/// Straight RGBA pixels of a rendered frame.
struct Pixels(Vec<u8>);

impl Pixels {
    fn at(&self, p: Vec2) -> [u8; 3] {
        let i = ((p.y as usize) * FRAME as usize + p.x as usize) * 4;
        [self.0[i], self.0[i + 1], self.0[i + 2]]
    }

    /// The darkest channel sum in a box around `p`.
    fn darkest_near(&self, p: Vec2, radius: f32) -> u32 {
        let mut darkest = u32::MAX;
        for dy in -(radius as i32)..=radius as i32 {
            for dx in -(radius as i32)..=radius as i32 {
                let [r, g, b] = self.at(p + Vec2::new(dx as f32, dy as f32));
                darkest = darkest.min(u32::from(r) + u32::from(g) + u32::from(b));
            }
        }
        darkest
    }

    fn mean_difference(&self, other: &Pixels) -> f64 {
        let sum: u64 = (self.0.iter().zip(&other.0))
            .map(|(a, b)| u64::from(a.abs_diff(*b)))
            .sum();
        sum as f64 / self.0.len() as f64
    }
}

fn assert_color(pixels: &Pixels, p: Vec2, expected: [u8; 3], what: &str, backend: &str) {
    let actual = pixels.at(p);
    let close = actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 4);
    assert!(
        close,
        "{backend}: {what} at {p}: expected {expected:?}, got {actual:?}"
    );
}

/// Checks the landmarks of the frame: the red square and the black text of the innermost
/// image, the white around them, and the green marker and the gray of B.
fn assert_landmarks(pixels: &Pixels, backend: &str) {
    let red = inner_in_frame(Vec2::splat(96.0));
    assert_color(pixels, red, [255, 0, 0], "the red square", backend);
    let white = inner_in_frame(Vec2::new(40.0, 200.0));
    assert_color(
        pixels,
        white,
        [255, 255, 255],
        "the white of the innermost image",
        backend,
    );
    let green = b_in_frame(Vec2::splat(48.0));
    assert_color(pixels, green, [0, 255, 0], "the marker of B", backend);
    let gray = b_in_frame(Vec2::new(460.0, 460.0));
    assert_color(pixels, gray, [128, 128, 128], "the gray of B", backend);
    // The "F" covers the box from (150, 160) to (200, 236) of the innermost image. Its stem
    // is near x 160.
    let text = inner_in_frame(Vec2::new(162.0, 200.0));
    assert!(
        pixels.darkest_near(text, 4.0) < 60,
        "{backend}: no black text near {text}"
    );
    // outside C the frame is black
    assert_color(pixels, Vec2::splat(20.0), [0, 0, 0], "the frame", backend);
}

fn render(video: &Nested, backend: impl fframes::FFramesRenderBackend) -> Pixels {
    let media = SvgIn3dMedia::prepare().expect("media");
    let options = RenderOptions {
        media: Some(&media),
        ..Default::default()
    };
    Pixels(fframes::render_frame(0, video, backend, &options).expect("the frame renders"))
}

fn save(pixels: &Pixels, name: &str) {
    let Ok(dir) = std::env::var("SVG_IN_3D_TEST_OUTPUT") else {
        return;
    };
    image_save(&format!("{dir}/{name}.png"), &pixels.0);
}

fn image_save(path: &str, rgba: &[u8]) {
    fframes::RgbaFrame {
        width: FRAME,
        height: FRAME,
        pixels: rgba.to_vec(),
    }
    .save_png(path)
    .expect("png");
}

#[test]
fn nested_images_land_where_the_projections_put_them() {
    let cpu = render(
        &Nested::new(false),
        fframes::cpu::CpuRenderingBackend::default(),
    );
    save(&cpu, "cpu");
    assert_landmarks(&cpu, "CPU backend");

    let skia_cpu_ctx = SkiaCpuCtx::new(FRAME as usize, FRAME as usize);
    let skia_cpu = render(
        &Nested::new(false),
        SkiaFFramesRenderer::new_cpu(&skia_cpu_ctx, SkiaPipelineConfig::default()).unwrap(),
    );
    save(&skia_cpu, "skia-cpu");
    assert_landmarks(&skia_cpu, "Skia CPU backend");
    let difference = skia_cpu.mean_difference(&cpu);
    assert!(
        difference < 1.0,
        "Skia CPU differs from CPU by {difference} on average"
    );

    #[cfg(not(target_os = "macos"))]
    {
        let Ok(vulkan) =
            fframes_skia_renderer::vulkan::SkiaVulkanCtx::new(FRAME as usize, FRAME as usize)
        else {
            eprintln!("no Vulkan device, the GPU part is skipped");
            return;
        };
        assert!(
            vulkan.shared_gpu().is_some(),
            "wgpu shares the Vulkan device"
        );
        let video = Nested::new(true);
        let gpu = render(
            &video,
            SkiaFFramesRenderer::new_vulkan(&vulkan, SkiaPipelineConfig::default()).unwrap(),
        );
        save(&gpu, "skia-vulkan");
        assert!(
            video.b_on_gpu.load(Ordering::Relaxed),
            "B stays on the GPU on Skia Vulkan with wgpu"
        );
        assert_landmarks(&gpu, "Skia Vulkan backend");
        let difference = gpu.mean_difference(&cpu);
        assert!(
            difference < 1.0,
            "the GPU differs from the CPU by {difference} on average"
        );
        eprintln!(
            "mean difference to the CPU backend: Skia CPU {:.3}, Skia Vulkan {difference:.3}",
            skia_cpu.mean_difference(&cpu)
        );
    }
}
