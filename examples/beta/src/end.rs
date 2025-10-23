use fframes::{Rotate, Scene, Transform, animation, svgr};

#[derive(Debug)]
pub struct EndScene {}

impl Scene for EndScene {
    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Seconds(2.)
    }

    fn render_frame(
        &self,
        frame: fframes::Frame,
        _ctx: &fframes::FFramesContext,
    ) -> fframes::Svgr<'_> {
        svgr!(
          <linearGradient id="text">
            <stop stop-color="#4338ca"/>
            <stop offset="1" stop-color="#a21caf"/>
          </linearGradient>

          <text
            font-size="190"
            font-family="Bubble Bobble"
            x="50%"
            y="50%"
            text-anchor="middle"
            opacity={frame.animate(&fframes::timeline!(
              at 0.1 => 0.2, animate 0. => 1., animation::Easing::Linear
            ))}
            fill="black"
          >
            <tspan fill="#7450d9">
              "ff"
            </tspan>
            "rames"
          </text>

          <text font-size="80" x="1220" y="605" fill="url(#text)" font-family="Chalkboard SE"
            opacity={
              frame.animate(
                &fframes::timeline!(
                  at 0.6 => 0.8, animate 0. => 1., animation::Easing::Linear
                )
              )
            }
            transform={
                frame.animate(
                    &fframes::timeline!(
                        at 0.6 => 0.8,
                        animate Transform::rotate(Rotate { angle: -60., origin: Some((1300., 590.)) })
                             => Transform::rotate(Rotate { angle: -25., origin: Some((1300., 590.)) }),
                        animation::Easing::Linear
                    )
                )
            }
          >
            "beta"
          </text>
        )
    }
}
