//! An svgr UI on the screen of a turning 3D monitor. The UI shows a 3D render of its own:
//! a cube with an svgr logo on its faces. So the frame nests four levels:
//!
//! ```text
//! logo svgr -> cube (3D) -> UI svgr -> monitor (3D) -> frame svgr
//! ```
//!
//! `ctx.render_svgr` renders each svgr level with the backend of the video. On the Skia
//! Vulkan backend with the `wgpu` feature, no level leaves the GPU.

// Colors are hex numbers, as in three.js: `0xff4a00`.
#![allow(clippy::unreadable_literal)]

use std::f32::consts::TAU;

use fframes::three_d::*;
use fframes::{AudioMap, Color, Duration, FFramesContext, Frame, Svgr, Video, svgr};

fframes::include_media_dir!(pub struct SvgIn3dMedia, "examples/svg-in-3d/media");

pub const WIDTH: usize = 1920;
pub const HEIGHT: usize = 1080;
pub const FPS: usize = 30;
const SECONDS: f32 = 8.0;

/// The size of the UI texture: the 16:10 screen at 1280 x 800.
const UI: (u32, u32) = (1280, 800);
/// The size of the cube render in the UI.
const CUBE: u32 = 480;

/// The monitor, the cube and their renderers. Built once, posed in each frame.
pub struct Monitor {
    scene: Scene,
    body: NodeId,
    screen: NodeId,
    renderer: Renderer,
    cube_scene: Scene,
    cube: NodeId,
    cube_renderer: Renderer,
}

impl std::fmt::Debug for Monitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Monitor").finish_non_exhaustive()
    }
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Monitor {
    pub fn new() -> Self {
        let mut scene = Scene::new().environment(Environment::studio());
        let frame = Material::metal(0x2a2d33, 0.35).clearcoat(0.6);
        let body = scene.add(
            Node::mesh(
                Geometry::rounded_box(1.84, 1.2, 0.09, 0.035, 4),
                frame.clone(),
            )
            .at(0.0, 1.32, 0.0),
        );
        // The UI shows as it is: an unlit screen with the UI texture, set in each frame.
        let screen = scene.add_to(
            body,
            Node::mesh(Geometry::plane(1.68, 1.05), Material::unlit(0xffffff)).at(0.0, 0.0, 0.0455),
        );
        scene.add(
            Node::mesh(Geometry::cylinder(0.05, 0.06, 0.62, 24), frame.clone()).at(0.0, 0.5, -0.06),
        );
        scene.add(
            Node::mesh(Geometry::rounded_box(0.8, 0.04, 0.42, 0.02, 3), frame).at(0.0, 0.2, -0.06),
        );
        scene.add(
            Node::mesh(Geometry::floor(20.0, 20.0), Material::shadow_catcher(0.55))
                .at(0.0, 0.18, 0.0)
                .shadows(false, true),
        );
        scene.add(Node::light(Light::sun(
            Rgb::WHITE,
            2.5,
            vec3(2.5, 6.0, 4.0),
        )));

        let mut cube_scene = Scene::new().environment(Environment::studio());
        let cube = cube_scene.add(Node::mesh(
            Geometry::cuboid(1.0, 1.0, 1.0),
            Material::standard(0xffffff).roughness(0.35),
        ));
        cube_scene.add(Node::light(Light::sun(
            Rgb::WHITE,
            2.0,
            vec3(2.0, 3.0, 4.0),
        )));

        Self {
            scene,
            body,
            screen,
            renderer: Renderer::new(WIDTH as u32, HEIGHT as u32).tone_mapping(ToneMapping::Neutral),
            cube_scene,
            cube,
            cube_renderer: Renderer::new(CUBE, CUBE).tone_mapping(ToneMapping::Neutral),
        }
    }

    /// Renders the 3D on the CPU, also with a GPU.
    pub fn on_cpu(mut self) -> Self {
        self.renderer = self.renderer.cpu();
        self.cube_renderer = self.cube_renderer.cpu();
        self
    }

    /// The cube with `logo` on its faces at `t` seconds.
    fn cube(
        &self,
        logo: &fframes::media::ImageData<'_>,
        t: f32,
    ) -> fframes::media::ImageData<'static> {
        let mut scene = self.cube_scene.clone();
        scene[self.cube].rotation = euler(t * 0.7, t * 1.1, 0.25);
        if let Some(material) = scene[self.cube].material_mut() {
            material.map = Some(Texture::from_image(logo).into());
        }
        let camera = Camera::perspective(35.0).at(0.0, 0.0, 3.4);
        self.cube_renderer.render(&scene, &camera)
    }

    /// The monitor with `ui` on its screen at `t` seconds.
    fn monitor(
        &self,
        ui: &fframes::media::ImageData<'_>,
        t: f32,
    ) -> fframes::media::ImageData<'static> {
        let mut scene = self.scene.clone();
        // turns far enough that the text on the screen is seen at a low angle
        scene[self.body].rotation = euler(0.0, (t / SECONDS * TAU).sin() * deg(52.0), 0.0);
        if let Some(material) = scene[self.screen].material_mut() {
            material.map = Some(Texture::from_image(ui).into());
        }
        let camera =
            Camera::perspective(30.0).orbit(vec3(0.0, 1.1, 0.0), 4.6, 12.0 * (t * 0.4).sin(), 9.0);
        self.renderer.render(&scene, &camera)
    }
}

/// `m:ss.cc` of `t` seconds.
fn timer(t: f32) -> String {
    let centis = (t * 100.0).round() as u32;
    format!(
        "{}:{:02}.{:02}",
        centis / 6000,
        centis / 100 % 60,
        centis % 100
    )
}

impl Video for Monitor {
    const FPS: usize = FPS;
    const WIDTH: usize = WIDTH;
    const HEIGHT: usize = HEIGHT;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SECONDS)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();

        // level 1: a static svgr logo. It renders once, later frames get the same image.
        let Some(logo) = ctx.render_svgr(
            svgr!(
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">
                    <rect x="0" y="0" width="256" height="256" fill="#2f81f7" />
                    <rect x="14" y="14" width="228" height="228" rx="28" fill="none" stroke="#ffffff" stroke-width="10" />
                    <text x="128" y="168" text-anchor="middle" font-family="DM Sans" font-size="116" fill="#ffffff">"ff"</text>
                </svg>
            ),
            256,
            256,
        ) else {
            return Svgr::empty();
        };

        // level 2: the logo on a turning cube
        let cube = self.cube(&logo, t);

        // level 3: the UI with a live timer and the cube render
        let progress = t / SECONDS;
        let Some(ui) = ctx.render_svgr(
            svgr!(
                <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1280 800" width="1280" height="800">
                    <rect x="0" y="0" width="1280" height="800" fill="#0d1117" />
                    <rect x="0" y="0" width="1280" height="96" fill="#161b22" />
                    <circle cx="52" cy="48" r="12" fill="#ff5f56" />
                    <circle cx="88" cy="48" r="12" fill="#ffbd2e" />
                    <circle cx="124" cy="48" r="12" fill="#27c93f" />
                    <text x="168" y="62" font-family="DM Sans" font-size="38" fill="#e6edf3">"ctx.render_svgr"</text>
                    <text x="1232" y="64" text-anchor="end" font-family="JetBrains Mono" font-size="40" fill="#ff7b72">{timer(t)}</text>
                    <rect x="64" y="148" width="528" height="528" rx="24" fill="#161b22" />
                    <image href={cube.href()} x="88" y="172" width="480" height="480" />
                    <text x="660" y="236" font-family="JetBrains Mono" font-size="34" fill="#7ee787">"svg  -> 3D"</text>
                    <text x="660" y="296" font-family="JetBrains Mono" font-size="34" fill="#79c0ff">"3D   -> svg"</text>
                    <text x="660" y="356" font-family="JetBrains Mono" font-size="34" fill="#d2a8ff">"svg  -> 3D"</text>
                    <text x="660" y="416" font-family="JetBrains Mono" font-size="34" fill="#ffa657">"3D   -> frame"</text>
                    <text x="660" y="520" font-family="DM Sans" font-size="30" fill="#8b949e">"frame " {frame.index.to_string()}</text>
                    <rect x="660" y="600" width="540" height="18" rx="9" fill="#30363d" />
                    <rect x="660" y="600" width={18.0 + 522.0 * progress} height="18" rx="9" fill="#2f81f7" />
                </svg>
            ),
            UI.0,
            UI.1,
        ) else {
            return Svgr::empty();
        };

        // level 4: the UI on the screen of the monitor, in the frame
        let monitor = self.monitor(&ui, t);
        svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1920 1080" width={WIDTH} height={HEIGHT}>
                <defs>
                    <radialGradient id="room" cx="50%" cy="38%" r="75%">
                        <stop offset="0%" stop-color="#3a4150" />
                        <stop offset="100%" stop-color="#0b0d12" />
                    </radialGradient>
                </defs>
                <rect x="0" y="0" width="1920" height="1080" fill="url(#room)" />
                <image href={monitor.href()} x="0" y="0" width="1920" height="1080" />
                <text x="96" y="1010" font-family="DM Sans" font-size="34" fill="#8b949e">"An svgr UI on a 3D screen. The UI holds a 3D render with an svgr logo."</text>
            </svg>
        )
    }
}
