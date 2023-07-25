use fframes::{animation, svgr, Scene};

#[derive(Debug)]
pub struct RenderingScene {}

impl Scene for RenderingScene {
    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Frames(140)
    }

    fn render_frame(&self, frame: fframes::Frame, _ctx: &fframes::FFramesContext) -> fframes::Svgr {
        const GPU_SECOND: f32 = 1.1;

        let gpu = frame.get_current_second() > GPU_SECOND;
        let derivation: f32 = rand::random();

        let base_fps = frame.animate(&fframes::timeline!(
          on GPU_SECOND, val 60. => 100., &animation::Easing::Linear(0.8)
        ));

        let fps_counter = (base_fps + (derivation * 4.)) as u8;

        svgr!(
           <text
             font-family="DM Sans"
             font-weight="700"
             x={frame.animate(fframes::timeline!(
               on 0., val -400. => 260., &animation::Easing::Spring2(0.6 , 300., 26.),
               on 0.3, val 260. => 310., &animation::Easing::Linear(1.9)
             ))}
             y="30%"
             font-size="150"
           >
            "With the power of"
           </text>

           <text
             font-family="DM Sans"
             font-weight="700"
             x={frame.animate(fframes::timeline!(
               on 0., val 1900. => 460., &animation::Easing::Spring2(0.6 , 300., 26.),
               on 0.3, val 460. => 410., &animation::Easing::Linear(1.9)
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
             width={frame.animate(fframes::timeline!(
               on 0., val 900. => 1000., &animation::Easing::Linear(0.8),
               on GPU_SECOND, val 1000. => 1350., &animation::Easing::Linear(1.0)
             ))}
           />

        )
    }
}
