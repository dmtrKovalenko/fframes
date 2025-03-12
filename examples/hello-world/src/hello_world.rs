use fframes::{AudioMap, Color, FFramesContext, Frame, Video, include_media_dir};

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

    fn render_frame(&self, frame: Frame, ctx: &FFramesContext) -> fframes::Svgr {
        const BACKGROUND_EASING: fframes::animation::Easing =
            fframes::animation::Easing::Linear(5.);

        fframes::svgr!(
           <svg
            xmlns="http://www.w3.org/2000/svg"
            width={ctx.current_video_size.width}
            height={ctx.current_video_size.height}
          >
            <rect
              width={ctx.current_video_size.width}
              height={ctx.current_video_size.height}
              x="0"
              y="0"
              fill={
                frame.animate(fframes::timeline!(
                  on 0.; Color::hex("#fff") => Color::hex("#f8fafc"), &BACKGROUND_EASING,
                  on 5.: Color::hex("#f8fafc") => Color::hex("#fff7ed"), &BACKGROUND_EASING,
                  on 10.: Color::hex("#fff7ed") => Color::hex("#fef2f2"), &BACKGROUND_EASING,
                  on 15.: Color::hex("#fef2f2") => Color::hex("#f7fee7"), &BACKGROUND_EASING,
                  on 20.: Color::hex("#f7fee7") => Color::hex("#ecfdf5"), &BACKGROUND_EASING,
                  on 25.: Color::hex("#ecfdf5") => Color::hex("#faf5ff"), &BACKGROUND_EASING
                ))
              }
            />

            <text font-family="DM Sans Medium" x="100" y="300" font-size="150">
              "Hello " {self.slug}
            </text>

            <text font-weight="500" font-family="JetBrains Mono" x="100" y="440" font-size="74" fill="#4b5563">
              {format!("This frame index: {}, second: {:.2}", frame.index, frame.get_current_second())}
            </text>
          </svg>
        )
    }
}
