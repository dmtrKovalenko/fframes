use std::sync::Arc;

use fframes::{AudioMap, AudioTimestamp, Scene, Svgr};
pub use fframes::{FFramesContext, Frame, Video};

pub struct LowPolyVideo {}

mod owl;
mod pelican;
mod popuga;
mod spektacled_owl;

impl Video for LowPolyVideo {
    const FPS: usize = 60;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Auto
    }

    fn audio(&self) -> AudioMap {
        AudioMap::from([(
            "owl.mp3",
            (AudioTimestamp::Frame(0)..AudioTimestamp::Second(10.)),
        )])
    }

    fn define_scenes(&self) -> fframes::Scenes {
        let vec: Vec<Arc<dyn Scene>> = vec![Arc::new(owl::Owl {})];

        fframes::Scenes::from(vec)
    }

    fn render_frame(&self, frame: Frame, ctx: &FFramesContext) -> Svgr {
        fframes::svgr!(
           <svg
            xmlns="http://www.w3.org/2000/svg"
            width={Self::WIDTH}
            height={Self::HEIGHT}
          >
            <defs>
              <pattern id="scratch-pattern" patternUnits="userSpaceOnUse" width="230" height="177">
                <image href={ctx.get_image_href("white_noise.png").expect("missing white_noise.png")} x="0" y="0" width="230" height="177" />
              </pattern>
            </defs>

            <rect
              width={Self::WIDTH}
              height={Self::HEIGHT}
              x="0"
              y="0"
              fill="#000"
            />

            {ctx.render_scenes(&frame)}
          </svg>
        )
    }
}
