use fframes::{include_media_dir, AudioMap, Color, FFramesContext, Frame, Video};

include_media_dir!(pub struct HelloWorldMedia, "examples/hello-world/media");

#[derive(Debug)]
pub struct HelloWorldVideo<'a> {
    pub slug: &'a str,
    pub media: &'a HelloWorldMedia,
}

impl Video for HelloWorldVideo<'_> {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(30.)
    }

    fn audio(&self) -> AudioMap {
        AudioMap::none()
    }

    fn render_frame(&self, frame: Frame, _ctx: &FFramesContext) -> fframes::Svgr {
        const BACKGROUND_EASING: fframes::animation::Easing =
            fframes::animation::Easing::Linear(5.);

        fframes::svgr!(
           <svg
            xmlns="http://www.w3.org/2000/svg"
            width={Self::WIDTH}
            height={Self::HEIGHT}
          >
            <rect
            width={Self::WIDTH}
            height={Self::HEIGHT}
            x="0"
            y="0"
            fill={
              frame.animate(fframes::keyframes!(
                initial_state Color::hex("#fff"),
                repeats 3,
                looping_delay 0.0,
                animate_to Color::hex("#f8fafc"), delay 0., &BACKGROUND_EASING,
                animate_to Color::hex("#fff7ed"), delay 5., &BACKGROUND_EASING,
                animate_to Color::hex("#fef2f2"), delay 5., &BACKGROUND_EASING,
                animate_to Color::hex("#f7fee7"), delay 5., &BACKGROUND_EASING,
                animate_to Color::hex("#ecfdf5"), delay 5., &BACKGROUND_EASING,
                animate_to Color::hex("#faf5ff"), delay 5., &BACKGROUND_EASING
              ))
            }
          />

            <text font-family="DM Sans" x="100" y="300" font-size="150">
              "Hello " {self.slug}
            </text>

            <text font-weight="500" font-family="JetBrains Mono" x="100" y="440" font-size="74" fill="#4b5563">
              {format!("This frame index: {}, second: {:.2}", frame.index, frame.get_current_second())}
            </text>
          </svg>
        )
    }
}
