#![allow(dead_code)]
use fframes::{include_media_dir, AudioMap, Scene, Svgr};
pub use fframes::{FFramesContext, Frame, Video};

include_media_dir!(pub struct LowPolyMedia, "examples/low-poly-art/media");

pub struct LowPolyVideo<'a> {
    pub scene: &'a dyn Scene,
    pub media: &'a LowPolyMedia,
}

pub mod owl;
pub mod pelican;
pub mod popuga;
pub mod spektacled_owl;

impl Video for LowPolyVideo<'_> {
    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Auto
    }

    fn audio(&self) -> AudioMap {
        AudioMap::none()
    }

    fn define_scenes(&self) -> fframes::Scenes {
        fframes::Scenes::from(vec![self.scene])
    }

    fn render_frame<'a>(&self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        fframes::svgr!(
           <svg
            xmlns="http://www.w3.org/2000/svg"
            width={ctx.width}
            height={ctx.height}
          >
            <defs>
              <pattern id="scratch-pattern" patternUnits="userSpaceOnUse" width="230" height="177">
                <image href={ctx.get_image("white_noise.png").expect("missing white_noise.png").href()} x="0" y="0" width="230" height="177" />
              </pattern>
            </defs>

            <rect
              width={ctx.width}
              height={ctx.height}
              x="0"
              y="0"
              fill="#000"
            />

            {ctx.render_scenes(&frame)}
          </svg>
        )
    }
}
