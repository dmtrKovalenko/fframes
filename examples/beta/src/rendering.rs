use fframes::{
    Scene,
    animation::{self, Easing},
    svgr,
};

#[derive(Debug)]
pub struct RenderingScene {}

impl Scene for RenderingScene {
    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Frames(140)
    }

    fn render_frame(
        &self,
        frame: fframes::Frame,
        _ctx: &fframes::FFramesContext,
    ) -> fframes::Svgr<'_> {
        const GPU_SECOND: f32 = 1.1;

        let gpu = frame.seconds() > GPU_SECOND;
        let derivation: f32 = rand::random();

        let base_fps = frame.animate(&fframes::timeline!(
          at GPU_SECOND => GPU_SECOND + 0.8, animate 60. => 100., animation::Easing::Linear
        ));
        let fps_counter = (base_fps + (derivation * 4.)) as u8;

        svgr!(
           <text
             font-family="DM Sans"
             font-weight="700"
             x={frame.animate(&fframes::timeline!(
               at 0., animate -400. => 260., Easing::Spring{ mass: 0.6 , stiffness: 300., damping: 26. },
               at 0.3 => 2.2, animate 260. => 310., Easing::Linear
             ))}
             y="30%"
             font-size="150"
           >
            "With the power of"
           </text>

           <text
             font-family="DM Sans"
             font-weight="700"
             x={frame.animate(&fframes::timeline!(
               at 0., animate 1900. => 460., Easing::Spring{ mass: 0.6 , stiffness: 300., damping: 26. },
               at 0.3 => 2.2, animate 460. => 410., Easing::Linear
             ))}
             y="50%"
             font-size="150"
           >
            <tspan
              font-weight={if gpu { "400" } else { "900"}}
              font-size={if gpu { "170" } else { "150"}}
              font-family={if gpu { "Bubble Bobble" } else { "DM Sans"}}
              fill="#7450d9">
             { if gpu { "  GPU    " } else { "native " } }
            </tspan>
            "     rendering"
           </text>

          <defs>
            <linearGradient id="progress">
            <stop stop-color="#aa83de"/>
            <stop offset="1" stop-color="#00d4ff"/>
            </linearGradient>
          </defs>

          <text y="70%" x="11%" fill="#4b5563" font-family="JetBrains Mono" font-size="45" font-weight="600">
             {format!("Rendering fps: {fps_counter}")}
          </text>
           <rect
             fill="url(#progress)"
             x="10%"
             y="72%"
             rx="12"
             ry="12"
             height="30"
             width={frame.animate(&fframes::timeline!(
               at 0. => 0.8, animate 900. => 1000., Easing::Linear,
               at GPU_SECOND => GPU_SECOND + 1., animate 1000. => 1350., Easing::Linear
             ))}
           />
        )
    }
}
