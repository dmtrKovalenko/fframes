use fframes::{include_media_dir, AudioMap, Color, FFramesContext, Frame, Video};

include_media_dir!(pub struct HelloWorldMedia, "examples/hello-world/media");

#[derive(Debug)]
pub struct HelloWorldVideo<'a> {
    pub slug: &'a str,
    pub media: &'a HelloWorldMedia,
}

impl Video for HelloWorldVideo<'_> {
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
            width={ctx.width}
            height={ctx.height}
          >
            <rect
              width={ctx.width}
              height={ctx.height}
              x="0"
              y="0"
              fill={
                frame.animate(fframes::timeline!(
                  on 0., val Color::hex("#fff") => Color::hex("#f8fafc"), &BACKGROUND_EASING,
                  on 5., val Color::hex("#f8fafc") => Color::hex("#fff7ed"), &BACKGROUND_EASING,
                  on 10., val Color::hex("#fff7ed") => Color::hex("#fef2f2"), &BACKGROUND_EASING,
                  on 15., val Color::hex("#fef2f2") => Color::hex("#f7fee7"), &BACKGROUND_EASING,
                  on 20., val Color::hex("#f7fee7") => Color::hex("#ecfdf5"), &BACKGROUND_EASING,
                  on 25., val Color::hex("#ecfdf5") => Color::hex("#faf5ff"), &BACKGROUND_EASING
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
