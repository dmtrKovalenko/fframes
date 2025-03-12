use fframes::{BreakLinesOpts, Scene, animation::Easing, timeline};

#[derive(Debug)]
pub struct StartScene {
    pub text: &'static str,
    pub duration: f32,
}

impl Scene for StartScene {
    fn overlap(&self) -> fframes::Overlap {
        fframes::Overlap::Next(self.duration)
    }

    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(3.0)
    }

    fn render_frame<'a>(
        &'a self,
        mut frame: fframes::Frame,
        ctx: &fframes::FFramesContext<'a, '_>,
    ) -> fframes::Svgr<'a> {
        frame
            .text_break_lines(
                ctx,
                &self.text,
                BreakLinesOpts {
                    x: "50%",
                    y: "30%",
                    font_size: 124,
                    font_family: "Space Grotesk",
                    fill: "#fff",
                    text_anchor: "middle",
                    dominant_baseline: "middle",
                    width: 1600,
                    line_height: 1.1,
                    opacity: frame.animate(&timeline!(
                        at (self.duration - 3.0) => self.duration, 1.0 => 0.0, Easing::EaseOut
                    )),
                    ..Default::default()
                },
            )
            .unwrap_or_default()
    }
}
