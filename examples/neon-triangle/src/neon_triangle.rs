use std::f32::consts::TAU;

use fframes::{
    AudioMap, Color, FFramesContext, Frame, Shader, ShaderUniforms, Svgr, Transform, Video,
    animation::Easing, include_media_dir,
};

include_media_dir!(pub struct NeonTriangleMedia, "examples/neon-triangle/media");

pub const TRIANGLE_SKSL: &str = include_str!("shaders/triangle.sksl");

/// A minimal shader test clip: one neon triangle rotating on pure black at a
/// constant speed. Smooth glow gradients and thin saturated lines on black
/// make banding, chroma bleeding and motion artifacts easy to spot.
#[derive(Debug)]
pub struct NeonTriangleVideo<'a> {
    pub media: &'a NeonTriangleMedia,
    triangle: Shader,
}

impl<'a> NeonTriangleVideo<'a> {
    pub fn new(media: &'a NeonTriangleMedia) -> Self {
        Self {
            media,
            triangle: Shader::sksl(TRIANGLE_SKSL),
        }
    }
}

impl Video for NeonTriangleVideo<'_> {
    const FPS: usize = 60;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Seconds(6.)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        // Constant angular speed (one full turn every 4 s, a slow roll on top)
        // so any stutter or smearing is a defect, not easing.
        let seconds = frame.seconds();
        let yaw = seconds * TAU / 4.0;
        let roll = seconds * TAU / 12.0;
        let triangle = self.triangle.draw(
            &frame,
            ShaderUniforms::new()
                .float("uYaw", yaw)
                .float("uRoll", roll)
                .color("uColor", Color::hex("#ff2bd6")),
        );

        // The title switches on like a neon tube: a few quick flickers, then steady.
        let title = frame.animate(&fframes::timeline!(
            at 0.20 => 0.24, animate 0.0_f32 => 1.0, Easing::Linear,
            at 0.30 => 0.33, animate 1.0 => 0.15, Easing::Linear,
            at 0.40 => 0.43, animate 0.15 => 1.0, Easing::Linear,
            at 0.56 => 0.58, animate 1.0 => 0.4, Easing::Linear,
            at 0.66 => 0.80, animate 0.4 => 1.0, Easing::Linear,
        ));
        let subtitle = frame.animate(&fframes::timeline!(
            at 0.9 => 1.5, animate 0.0_f32 => 1.0, Easing::EaseOut
        ));
        let subtitle_slide = frame.animate(&fframes::timeline!(
            at 0.9, animate Transform::translate(-40, 0) => Transform::translate(0, 0),
                Easing::Spring { mass: 1.0, stiffness: 140.0, damping: 20.0 },
        ));

        let readout = format!(
            "yaw {:03.0}°  roll {:03.0}°  frame {:03}",
            yaw.to_degrees() % 360.0,
            roll.to_degrees() % 360.0,
            frame.index
        );

        fframes::svgr!(
            <svg
                xmlns="http://www.w3.org/2000/svg"
                viewBox="0 0 1920 1080"
                width={Self::WIDTH}
                height={Self::HEIGHT}
            >
                <defs>
                    // White text with a tight and a wide magenta bloom.
                    <filter id="neon" x="-20%" y="-50%" width="140%" height="200%">
                        <feGaussianBlur in="SourceAlpha" stdDeviation="5" result="tight" />
                        <feGaussianBlur in="SourceAlpha" stdDeviation="18" result="wide" />
                        <feMerge result="blur">
                            <feMergeNode in="tight" />
                            <feMergeNode in="wide" />
                        </feMerge>
                        <feFlood flood-color="#ff2bd6" result="tint" />
                        <feComposite in="tint" in2="blur" operator="in" result="glow" />
                        <feMerge>
                            <feMergeNode in="glow" />
                            <feMergeNode in="glow" />
                            <feMergeNode in="SourceGraphic" />
                        </feMerge>
                    </filter>
                </defs>

                // The shader covers only the right side; iResolution follows the element.
                <image href={triangle.href()} x="820" y="0" width="1100" height="1080" />

                <g opacity={title} filter="url(#neon)" font-family="DM Sans" font-weight="500" fill="#ffffff">
                    <text x="140" y="480" font-size="190" letter-spacing="6">"NEON"</text>
                    <text x="146" y="620" font-size="104" letter-spacing="14">"TRIANGLE"</text>
                </g>

                <g opacity={subtitle} transform={subtitle_slide} font-family="JetBrains Mono">
                    <text x="148" y="720" font-size="30" fill="#f5d0fe">"one SkSL shader, drawn by Skia on the GPU"</text>
                </g>

                <text x="148" y="1000" font-family="JetBrains Mono" font-size="24" fill="#a78bfa" opacity={subtitle}>
                    {readout}
                </text>
            </svg>
        )
    }
}
