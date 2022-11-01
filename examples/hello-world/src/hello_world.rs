pub use fframes::{audio_data, fframes_context, frame, video::Video};
use fframes::{AudioMap, Color};

#[derive(Debug)]
pub struct HelloWorldVideo {
    pub slug: String,
}

impl Video for HelloWorldVideo {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;
    const DURATION: fframes::Duration = fframes::Duration::Seconds(30);

    fn audio(&self) -> AudioMap {
        AudioMap::none()
    }

    fn render_frame(
        &self,
        frame: frame::Frame,
        _ctx: &fframes_context::FFramesContext,
    ) -> fframes::Svgr {
        const BACKGROUND_EASING: fframes::Easing = fframes::Easing::Linear(5.);

        fframes::svgr!(
           <svg
            xmlns="http://www.w3.org/2000/svg"
            xmlns:xlink="http://www.w3.org/1999/xlink"
            width={Self::WIDTH}
            height={Self::HEIGHT}
          >
            <rect
              width={Self::WIDTH}
              height={Self::HEIGHT}
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
              {self.slug.as_str()}
            </text>

            <text font-weight="500" font-family="JetBrains Mono" x="100" y="440" font-size="74" fill="#4b5563">
              {format!("This frame index: {}, second: {:.2}", frame.index, frame.get_current_second())}
            </text>
          </svg>
        )
    }
}
