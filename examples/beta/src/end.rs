use fframes::{Scene, animation, svgr};

#[derive(Debug)]
pub struct EndScene {}

impl Scene for EndScene {
    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(2.)
    }

    fn render_frame(&self, frame: fframes::Frame, _ctx: &fframes::FFramesContext) -> fframes::Svgr {
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
              on 0.1, val 0. => 1., animation::Easing::Linear(0.1)
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
                fframes::timeline!(
                  on 0.6, val 0. => 1., animation::Easing::Linear(0.2)
                )
              )
            }
            transform={format!("rotate({} 1300 590)",
              frame.animate(
                &fframes::timeline!(
                  on 0.6, val -60. => -25., animation::Easing::Linear(0.2)
                )
              )
            )}
          >
            "beta"
          </text>
        )
    }
}
