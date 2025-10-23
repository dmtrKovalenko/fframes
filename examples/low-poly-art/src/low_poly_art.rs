use fframes::{AudioMap, Scene, Svgr, include_media_dir};
pub use fframes::{FFramesContext, Frame, Video};

pub mod owl;
#[allow(dead_code)]
pub mod pelican;
#[allow(dead_code)]
pub mod popuga;
#[allow(dead_code)]
pub mod spektacled_owl;

include_media_dir!(pub struct LowPolyMedia, "examples/low-poly-art/media");

pub struct LowPolyVideo<'a> {
    pub scene: &'a dyn Scene,
    pub media: &'a LowPolyMedia,
}

impl Video for LowPolyVideo<'_> {
    const FPS: usize = 60;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Auto
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn define_scenes(&self) -> fframes::Scenes<'_> {
        fframes::Scenes::from(vec![self.scene])
    }

    fn render_frame<'a>(&self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        fframes::svgr!(
            <svg
                xmlns="http://www.w3.org/2000/svg"
                width={Self::WIDTH}
                height={Self::HEIGHT}
            >

                <defs>
                    // Noise texture pattern for visual effects
                    <pattern id="scratch-pattern" patternUnits="userSpaceOnUse" width="230" height="177">
                        <image
                            href={ctx.get_image("white_noise.png").expect("missing white_noise.png").href()}
                            x="0"
                            y="0"
                            width="230"
                            height="177"
                        />
                    </pattern>
                </defs>

                // Black background canvas
                <rect
                    width={Self::WIDTH}
                    height={Self::HEIGHT}
                    x="0"
                    y="0"
                    fill="#000"
                />

                // Render the current scene (owl, pelican, etc.)
                {ctx.render_scenes(&frame)}
            </svg>
        )
    }
}
