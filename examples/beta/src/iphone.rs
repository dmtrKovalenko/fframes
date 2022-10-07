use fframes::{animation, svgr, Scene, audio_data};

#[derive(Debug)]
pub struct IphoneScene {}

impl Scene for IphoneScene {
    fn duration(&self) -> fframes::video::Duration {
        fframes::video::Duration::Seconds(10)
    }

    fn render_frame(
        &self,
        frame: fframes::frame::Frame,
        ctx: &fframes::FFramesContext,
    ) -> fframes::Svgr {
        let dynamic_island_width = frame.animate(&fframes::timeline!(
            on 0.0, val 120. => 180., &animation::Easing::Spring2(0.6 , 300., 26.)
        ));

        let audio_visualization = frame.visualize_audio_frame(audio_data::VisualizeFrameInput {
            smooth_level: 3,
            ctx,
            audio: ctx.get_audio_data("beta.mp3"),
            sample_size: audio_data::SampleSize::S16,
            window: None,
        });

        svgr!(
         <clipPath id="iphoneUi">
           <rect width="380" height="820" rx="60" x="780" y="150"/>
         </clipPath>
          <g clip-path="url(#iphoneUi)">
            <image
               xlink:href={ctx.get_image_link("camera_ui.png")}
               width="370"
               height="819"
               rx="60"
               x="790"
               y="154"
            />

            <image
              xlink:href={ctx.get_image_link("qr.png")}
              width="300"
              height="300"
              x="830"
              y={1080 / 2 - 150}
            />
          </g>

          <image xlink:href={ctx.get_image_link("iphone_frame.png")} x="30%" y="10%" width="800" />

          <rect
            x="915"
            y="170"
            width={dynamic_island_width}
            transform={format!("translate(-{}, 0)",( dynamic_island_width - 120. )/ 2.)}
            height="40"
            rx="20"
            fill="#000"
          />

          <text font-family="Bubble Bobble" fill="#6366f1" font-size="23" x="895" y="197">
          "ff"
          </text>
          {
            audio_visualization
            .iter()
            .enumerate()
            .skip(1)
            .take(6)
            .map(|(i, freq)| {
              let db = (10.0 * libm::log10f(*freq)).max(10.);

              svgr!(
                <rect
                  x={1025. + i as f64 * 3.5 + i as f64}
                  y={190. - db / 2.}
                  width="3"
                  height={db - 3.}
                  ry="1"
                  fill="#6366f1"
                />
              )
            })
            .collect::<Vec<_>>()
          }

        )
    }
}
