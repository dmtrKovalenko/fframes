use fframes::{
    AudioMap, Color, FFramesContext, Frame, Svgr, Transform, animation::Easing, include_media_dir,
};

use crate::SPRING_SNAPPY;

include_media_dir!(pub struct MotionGraphicsMedia, "examples/motion-graphics/media");

#[derive(Debug)]
pub struct MotionGraphicsVideo<'a> {
    pub media: &'a MotionGraphicsMedia,
}

impl fframes::Video for MotionGraphicsVideo<'_> {
    const FPS: usize = 60;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Seconds(4.0)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        // "I FIXED" — punches in with scale, holds, then moves up
        let line1_scale = frame.animate(&fframes::timeline!(
            at 0.2 => 0.6, animate 1.4_f32 => 1.0, SPRING_SNAPPY,
        ));
        let line1_opacity = frame.animate(&fframes::timeline!(
            at 0.2 => 0.5, animate 0.0_f32 => 1.0, Easing::EaseOut,
        ));
        let line1_y = frame.animate(&fframes::timeline!(
            at 0.2 => 0.7, animate 540.0_f32 => 540.0, Easing::Linear,
            at 0.7 => 1.1, animate 540.0_f32 => 280.0, Easing::EaseInOut,
        ));

        // "CLAUDE CODE" — smooth glide up from below
        let line2_opacity = frame.animate(&fframes::timeline!(
            at 0.8 => 1.4, animate 0.0_f32 => 1.0, Easing::EaseInOut,
        ));
        let line2_y = frame.animate(&fframes::timeline!(
            at 0.8 => 1.5, animate 720.0_f32 => 620.0, Easing::EaseInOut,
        ));

        // Accent line expands from center
        let accent_width = frame.animate(&fframes::timeline!(
            at 1.0 => 1.6, animate 0.0_f32 => 1000.0, Easing::EaseInOut,
        ));

        // "(for real)" — slides up from below into a clip box
        let line3_y = frame.animate(&fframes::timeline!(
            at 1.3 => 1.8, animate 890.0_f32 => 810.0, Easing::EaseOut,
        ));
        let line3_opacity = frame.animate(&fframes::timeline!(
            at 1.3 => 1.8, animate 0.0_f32 => 0.6, Easing::EaseOut,
        ));

        // Fade out
        let fade_out = frame.animate(&fframes::timeline!(
            at 3.2 => 3.8, animate 1.0_f32 => 0.0, Easing::EaseIn,
        ));

        fframes::svgr!(
            <svg
                xmlns="http://www.w3.org/2000/svg"
                viewBox="0 0 1920 1080"
                width={Self::WIDTH}
                height={Self::HEIGHT}
            >
                <rect width={Self::WIDTH} height={Self::HEIGHT} fill="black" />

                <g opacity={fade_out}>
                    // "I FIXED" — scale punch in, then spring up
                    <text
                        x="960"
                        y={line1_y}
                        opacity={line1_opacity}
                        font-family="Helvetica Neue"
                        font-size="340"
                        font-weight="700"
                        fill="white"
                        text-anchor="middle"
                        dominant-baseline="central"
                        letter-spacing="20"
                        transform={Transform {
                            translate_x: 960.0 * (1.0 - line1_scale as f64),
                            translate_y: line1_y as f64 * (1.0 - line1_scale as f64),
                            scale: fframes::Scale { x: line1_scale as f64, y: line1_scale as f64 },
                            ..Default::default()
                        }}
                    >
                        "I FIXED"
                    </text>

                    // "CLAUDE CODE" — springs up from below
                    <text
                        x="960"
                        y={line2_y}
                        opacity={line2_opacity}
                        font-family="Bebas Neue"
                        font-size="280"
                        fill="white"
                        text-anchor="middle"
                        dominant-baseline="central"
                        letter-spacing="14"
                    >
                        "CLAUDE CODE"
                    </text>

                    // Accent line
                    <rect
                        x={960.0 - accent_width / 2.0}
                        y="455"
                        width={accent_width}
                        height="3"
                        fill="white"
                        opacity="0.25"
                        rx="2"
                    />

                    // "(for real)" — slides up with fade
                    <text
                        x="960"
                        y={line3_y}
                        opacity={line3_opacity}
                        font-family="Helvetica Neue"
                        font-size="60"
                        font-weight="300"
                        font-style="italic"
                        fill="white"
                        text-anchor="middle"
                        letter-spacing="6"
                    >
                        "(for real)"
                    </text>
                </g>
            </svg>
        )
    }
}
