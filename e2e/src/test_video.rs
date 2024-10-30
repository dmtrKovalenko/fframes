pub use fframes::Video;
use fframes::{animation, AudioMap, Color, FFramesContext, Frame};

#[derive(Debug)]
pub struct TestVideo {
    pub slug: String,
}

impl Video for TestVideo {
    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(15.)
    }

    fn audio(&self) -> AudioMap {
        AudioMap::none()
    }

    fn render_frame(&self, frame: Frame, ctx: &FFramesContext) -> fframes::Svgr {
        const BACKGROUND_EASING: animation::Easing = animation::Easing::Linear(5.);

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
                  on 10., val Color::hex("#fff7ed") => Color::hex("#fef2f2"), &BACKGROUND_EASING
                ))
              }
            />

            <svg xmlns="http://www.w3.org/2000/svg" x="40" y="40" width="100" height="100" viewBox="0 0 297.5 297.5">
              <path style="fill:#bce6ec"
                d="m42.655 154.48 100.36 100.36c-12.37 9.4-27.39 14.49-43.19 14.49-19.14 0-37.13-7.45-50.67-20.99-13.53-13.53-20.99-31.53-20.99-50.67 0-15.8 5.1-30.81 14.49-43.19zM157.445 155.05c9.11 12.26 14.04 27.07 14.04 42.62 0 15.81-5.1 30.82-14.49 43.2L56.635 140.5c12.37-9.39 27.39-14.48 43.19-14.48 19.14 0 37.13 7.45 50.67 20.99a74.06 74.06 0 0 1 6.9 7.97c.01.03.03.05.05.07z" />
              <path style="fill:#ffd63f"
                d="M269.325 67.21v71.66h-94.39V67.21c0-26.02 21.18-47.19 47.2-47.19s47.19 21.17 47.19 47.19z" />
              <path style="fill:#0099ef"
                d="M269.325 158.63v71.66c0 26.02-21.17 47.19-47.19 47.19-20.59 0-38.14-13.26-44.57-31.68 8.9-14.32 13.68-30.85 13.68-48.13 0-13.72-3-26.99-8.7-39.04h86.78z" />
              <path
                d="M289.095 67.21v163.08c0 36.92-30.04 66.96-66.96 66.96-25.04 0-46.91-13.82-58.4-34.23-17.18 16.81-39.83 26.08-63.91 26.08-24.42 0-47.38-9.51-64.65-26.78-17.26-17.27-26.77-40.23-26.77-64.65s9.51-47.37 26.77-64.64c17.27-17.27 40.23-26.78 64.65-26.78 20.27 0 39.51 6.57 55.35 18.67V67.21c0-36.92 30.04-66.96 66.96-66.96s66.96 30.04 66.96 66.96zm-19.77 163.08v-71.66h-86.78c5.7 12.05 8.7 25.32 8.7 39.04 0 17.28-4.78 33.81-13.68 48.13 6.43 18.42 23.98 31.68 44.57 31.68 26.02 0 47.19-21.17 47.19-47.19zm0-91.42V67.21c0-26.02-21.17-47.19-47.19-47.19s-47.2 21.17-47.2 47.19v71.66h94.39zm-97.84 58.8c0-15.55-4.93-30.36-14.04-42.62a.18.18 0 0 1-.05-.07 74.06 74.06 0 0 0-6.9-7.97c-13.54-13.54-31.53-20.99-50.67-20.99-15.8 0-30.82 5.09-43.19 14.48l100.36 100.37c9.39-12.38 14.49-27.39 14.49-43.2zm-28.47 57.17L42.655 154.48c-9.39 12.38-14.49 27.39-14.49 43.19 0 19.14 7.46 37.14 20.99 50.67 13.54 13.54 31.53 20.99 50.67 20.99 15.8 0 30.82-5.09 43.19-14.49z" />
            </svg>

            <text font-weight="500" font-family="JetBrains Mono" x="100" y="240" font-size="34" fill="#4b5563">
              {format!("{} {}, second: {:.2}", self.slug, frame.index, frame.get_current_second())}
            </text>
          </svg>
        )
    }
}
