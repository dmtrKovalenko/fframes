use fframes::{animation, AudioMap, AudioTimestamp, Color, Scene, Svgr};
pub use fframes::{audio_data, fframes_context, frame, video::Video};
use svgr_macro::{self, svgr};

pub struct BetaVideo {}

#[derive(Debug)]
struct HeadingScene {}

impl Scene for HeadingScene {
    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(3)
    }

    fn render_frame(&self, frame: frame::Frame, _ctx: &fframes_context::FFramesContext) -> Svgr {
        svgr!(
          <text
            font-family="DM Sans"
            font-weight="700"
            x={frame.animate(fframes::timeline!(
              on 0., val -400. => 260., &animation::Easing::Spring2(1.0 , 100., 16.)
            ))}
            y="40%"
            font-size="150"
          >
           "r12 to the beta"
          </text>

          <text
            font-family="DM Sans"
            font-weight="700"
            x={frame.animate(fframes::timeline!(
              on 0., val 1000. => 660., &animation::Easing::Spring2(1.0 , 100., 16.)
            ))}
            y="60%"
            font-size="150"
          > "of "
          {if frame.index < 100 {
            svgr!(<tspan> "fframes" </tspan>)
          } else {
            svgr!(<tspan dy="10" font-size="190" font-weight="normal" font-family="Bubble Bobble"> <tspan fill="#7450d9"> "ff" </tspan> "rames" </tspan>)
          }
        }
          </text>
        )
    }
}

impl Video for BetaVideo {
    const FPS: usize = 60;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn audio(&self) -> AudioMap {
        AudioMap::from([
            (
                "beta.mp3",
                (AudioTimestamp::Second(0), AudioTimestamp::Second(35)),
            ),
            ("pop.mp3", (AudioTimestamp::Frame(95), AudioTimestamp::Eof)),
            ("pop.mp3", (AudioTimestamp::Frame(440), AudioTimestamp::Eof)),
        ])
    }

    fn define_scenes(&self) -> fframes::Scenes {
        let vec: Vec<Box<dyn Scene>> = vec![
            Box::new(HeadingScene {}),
            Box::new(crate::code_demo::CodeDemoScene {}),
            Box::new(crate::rendering::RenderingScene {}),
            Box::new(crate::iphone::IphoneScene {}),
        ];

        fframes::Scenes::from(vec)
    }

    fn render_frame(&self, frame: frame::Frame, _ctx: &fframes_context::FFramesContext) -> Svgr {
        svgr!(
           <svg
            xmlns="http://www.w3.org/2000/svg"
            xmlns:xlink="http://www.w3.org/1999/xlink"
            width={Self::WIDTH}
            height={Self::HEIGHT}
          >
            <rect
              width={Self::WIDTH}
              height={Self::HEIGHT}
              x="0"
              y="0"
             fill="#fff"
            />
                        <image
              width={Self::WIDTH}
              height={Self::HEIGHT}
              x="0"
              y="0"
              xlink:href={_ctx.get_image_link("background.png")}
             fill="#fff"
            />

            {_ctx.render_scenes(&frame)}

          </svg>
        )
    }
}
