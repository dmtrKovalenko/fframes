use fframes::{
    AudioMap, Color, FFramesContext, Frame, Shader, ShaderUniforms, Svgr, Transform, Video,
    animation::Easing, include_media_dir,
};

include_media_dir!(pub struct ShadersMedia, "examples/shaders/media");

pub const AURORA_SKSL: &str = include_str!("shaders/aurora.sksl");
pub const TORUS_SHADERTOY: &str = include_str!("shaders/torus.glsl");

/// GPU shader layers composed with regular SVG: an SkSL background, a
/// Shadertoy raymarcher clipped into a card, and text drawn on top.
#[derive(Debug)]
pub struct ShadersVideo<'a> {
    pub media: &'a ShadersMedia,
    aurora: Shader,
    torus: Shader,
}

impl<'a> ShadersVideo<'a> {
    pub fn new(media: &'a ShadersMedia) -> Self {
        Self {
            media,
            aurora: Shader::sksl(AURORA_SKSL),
            torus: Shader::shadertoy(TORUS_SHADERTOY),
        }
    }
}

impl Video for ShadersVideo<'_> {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Seconds(8.)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let background = self.aurora.draw(
            &frame,
            ShaderUniforms::new()
                .color(
                    "uColorA",
                    frame.animate(&fframes::timeline!(
                        at 0. => 8., animate Color::hex("#22d3ee") => Color::hex("#a855f7"), Easing::EaseInOut
                    )),
                )
                .color("uColorB", Color::hex("#f472b6"))
                .float("uSpeed", 0.6),
        );
        let torus = self.torus.draw(&frame, ShaderUniforms::new());

        let card_slide = frame.animate(&fframes::timeline!(
            at 0.3, animate Transform::translate(0, 140) => Transform::translate(0, 0),
                Easing::Spring { mass: 1.0, stiffness: 180.0, damping: 20.0 },
        ));
        let card_opacity = frame.animate(&fframes::timeline!(
            at 0.3 => 0.9, animate 0.0_f32 => 1.0, Easing::EaseOut
        ));
        let title_opacity = frame.animate(&fframes::timeline!(
            at 0.6 => 1.4, animate 0.0_f32 => 1.0, Easing::EaseOut
        ));

        fframes::svgr!(
            <svg
                xmlns="http://www.w3.org/2000/svg"
                viewBox="0 0 1920 1080"
                width={Self::WIDTH}
                height={Self::HEIGHT}
            >
                <defs>
                    <clipPath id="card">
                        <rect x="1040" y="160" width="760" height="760" rx="40" />
                    </clipPath>
                </defs>

                <image href={background.href()} x="0" y="0" width="1920" height="1080" />

                <g transform={card_slide} opacity={card_opacity}>
                    <g clip-path="url(#card)">
                        <image href={torus.href()} x="1040" y="160" width="760" height="760" />
                    </g>
                    <rect
                        x="1040" y="160" width="760" height="760" rx="40"
                        fill="none" stroke="#ffffff" stroke-opacity="0.2" stroke-width="2"
                    />
                </g>

                <g opacity={title_opacity}>
                    <text x="120" y="500" font-family="DM Sans" font-weight="500" font-size="128" fill="#ffffff">
                        "GPU shaders"
                    </text>
                    <text x="126" y="580" font-family="DM Sans" font-weight="500" font-size="40" fill="#f5f3ff" fill-opacity="0.8">
                        "SkSL and Shadertoy, drawn by Skia"
                    </text>
                </g>
            </svg>
        )
    }
}
