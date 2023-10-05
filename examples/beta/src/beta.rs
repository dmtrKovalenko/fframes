use std::sync::Arc;

use fframes::{animation, AudioMap, AudioTimestamp, FFramesContext, Frame, Scene, Svgr, Video};
use hello_world_example::HelloWorldVideo;
use marketing_example::MarketingVideo;
use podcast_example::PodcastVideo;
use svgr_macro::{self, svgr};
use tiktok_example::GooseVideo;

pub struct BetaVideo {
    pub minutes: u32,
    pub hours: u32,
    pub hello_world_video: Arc<HelloWorldVideo<'static>>,
    pub marketing_video: Arc<MarketingVideo<'static>>,
    pub podcast_video: Arc<PodcastVideo<'static>>,
    pub tiktok_video: Arc<GooseVideo<'static>>,
}

#[derive(Debug)]
struct HeadingScene {}

impl Scene for HeadingScene {
    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(3.)
    }

    fn render_frame(&self, frame: Frame, _ctx: &FFramesContext) -> Svgr {
        svgr!(
          <text
            font-family="DM Sans"
            font-weight="700"
            x={frame.animate(fframes::timeline!(
              on 0., val -400. => 260., &animation::Easing::Spring2(1.0 , 100., 16.)
            ))}
            y="40%"
            font-size="150"
            fill="#000"
          >
           "Welcome to the beta"
          </text>

          <text
            font-family="DM Sans"
            font-weight="700"
            fill="#000"
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

    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Auto
    }

    fn audio(&self) -> AudioMap {
        use AudioTimestamp::*;

        AudioMap::from([
            ("beta.mp3", (Frame(0)..Eof)),
            ("pop.mp3", (Frame(95)..Eof)),
            ("pop.mp3", (Frame(440)..Eof)),
        ])
    }

    fn define_scenes(&self) -> fframes::Scenes {
        let vec: Vec<Box<dyn Scene>> = vec![
            Box::new(HeadingScene {}),
            Box::new(crate::code_demo::CodeDemoScene {}),
            Box::new(crate::rendering::RenderingScene {}),
            Box::new(crate::iphone::IphoneScene {
                hours: self.hours,
                minutes: self.minutes,
            }),
            Box::new(crate::github::GithubScene {}),
            Box::new(crate::examples::ExamplesScene {
                hello_world_video: self.hello_world_video.clone(),
                marketing_video: self.marketing_video.clone(),
                podcast_video: self.podcast_video.clone(),
                tiktok_video: self.tiktok_video.clone(),
            }),
            Box::new(crate::end::EndScene {}),
        ];

        fframes::Scenes::from(vec)
    }

    fn render_frame(&self, frame: Frame, ctx: &FFramesContext) -> Svgr {
        svgr!(
           <svg
            xmlns="http://www.w3.org/2000/svg"
            width={Self::WIDTH}
            height={Self::HEIGHT}
          >
            <image
              width={Self::WIDTH}
              height={Self::HEIGHT}
              x="0"
              y="0"
              href={ctx.get_image_href("background.png").expect("Do not use .expect() om media in the real code")}
              fill="#fff"
            />

            {ctx.render_scenes(&frame)}
          </svg>
        )
    }
}
