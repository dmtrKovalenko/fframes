use crate::bokeh_background::BokehCircle;
use fframes::{
    AnimateRuntimeInput, AudioMap, FFramesContext, Frame, Scene, Scenes, Svgr, Transform, Video,
    animation::{AnimationRuntime, Easing},
    include_media_dir,
    lazy_static::lazy_static,
};
use std::sync::Arc;

include_media_dir!(pub struct PixelMedia, "examples/pixel-memory/media");

#[derive(Debug)]
pub struct PixelVideo<'a> {
    pub music: &'a str,
    pub bokeh_circles: Vec<BokehCircle>,
    pub scenes: Vec<Arc<dyn Scene + 'a>>,
    pub total_duration: f32,
}

impl Video for PixelVideo<'_> {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(self.total_duration)
    }

    fn define_scenes(&self) -> Scenes {
        Scenes::from(self.scenes.as_slice())
    }

    fn audio(&self) -> AudioMap {
        use fframes::AudioTimestamp::*;

        AudioMap::from([(self.music, Second(0.)..Eof)])
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let deviation1 = frame.animate_loop(&fframes::timeline!(
            at 0.0, animate 0.0 => 20.0, Easing::Linear,
            at 2.8, animate 20.0 => 0.0, Easing::Linear,
            at 5.3, animate 0.0 => -20.0, Easing::Linear,
            at 7.9, animate-20.0 => 0.0, Easing::Linear,
            at 10.4,animate 0.0 => 20.0, Easing::Linear,
            at 13.1,animate 20.0 => -20.0, Easing::Linear,
            at 15.7,animate -20.0 => 0.0, Easing::Linear,
            at 18.2,animate 0.0 => 20.0, Easing::Linear,
            at 20.9,animate 20.0 => 0.0, Easing::Linear,
            at 23.5,animate 0.0 => -20.0, Easing::Linear,
            at 26.1 => 28.7, animate -20.0 => 0.0, Easing::Linear,
        ));

        let deviation2 = frame.animate_loop(&fframes::timeline!(
            at 0.0,animate 0.0 => 20.0, Easing::Linear,
            at 2.3,animate 20.0 => 0.0, Easing::Linear,
            at 4.9,animate 0.0 => -20.0, Easing::Linear,
            at 7.2,animate -20.0 => 0.0, Easing::Linear,
            at 9.8,animate 0.0 => 20.0, Easing::Linear,
            at 12.1,animate  20.0 => -20.0, Easing::Linear,
            at 14.7,animate  -20.0 => 0.0, Easing::Linear,
            at 17.3,animate  0.0 => 20.0, Easing::Linear,
            at 19.6,animate  20.0 => 0.0, Easing::Linear,
            at 22.2,animate  0.0 => -20.0, Easing::Linear,
            at 24.5,animate  -20.0 => 0.0, Easing::Linear,
            at 27.1,animate  0.0 => 20.0, Easing::Linear,
            at 29.8 => 32.0, animate 20.0 => 0.0, Easing::Linear,
        ));

        fframes::svgr!(
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width={ctx.current_video_size.width}
            height={ctx.current_video_size.height}
          >
            <defs>
              <linearGradient id="bg-gradient" x1="0%" y1="0%" x2="100%" y2="100%">
                <stop offset="0%" stop-color="#000000" />
                <stop offset="100%" stop-color="#050505" />
              </linearGradient>
              <filter id="bokeh-blur-large" x="-50%" y="-50%" width="200%" height="200%">
                <feGaussianBlur in="SourceGraphic" stdDeviation="8" />
              </filter>
              <filter id="bokeh-blur-medium" x="-50%" y="-50%" width="200%" height="200%">
                <feGaussianBlur in="SourceGraphic" stdDeviation="5" />
              </filter>
              <filter id="bokeh-blur-small" x="-50%" y="-50%" width="200%" height="200%">
                <feGaussianBlur in="SourceGraphic" stdDeviation="2" />
              </filter>
              <radialGradient id="bokeh-gold-bright" cx="50%" cy="50%" r="50%" fx="40%" fy="40%">
                <stop offset="0%" stop-color="#ffffff" />
                <stop offset="70%" stop-color="#f5f0e0" />
                <stop offset="100%" stop-color="#e1c78c" />
              </radialGradient>
              <radialGradient id="bokeh-gold-medium" cx="50%" cy="50%" r="50%" fx="40%" fy="40%">
                <stop offset="0%" stop-color="#f8f5e8" />
                <stop offset="70%" stop-color="#e8dfc0" />
                <stop offset="100%" stop-color="#c0b090" />
              </radialGradient>
              <radialGradient id="bokeh-gold-soft" cx="50%" cy="50%" r="50%" fx="40%" fy="40%">
                <stop offset="0%" stop-color="#f0ebd8" />
                <stop offset="70%" stop-color="#d8cba8" />
                <stop offset="100%" stop-color="#a89870" />
              </radialGradient>
            </defs>

            <rect width="1920" height="1080" fill="url(#bg-gradient)" />

            <g id="circles">
              {self.bokeh_circles.iter().enumerate().map(|(index, circle)| {
                let deviation = if index % 2 == 0 { deviation1 } else { deviation2 };
                let (heart_x, heart_y) = circle.final_heart_point;

                lazy_static! {
                  static ref HEART_RUNTIME: AnimationRuntime = AnimationRuntime::new(3.0, &Easing::EaseInOut);
                };

                fframes::svgr!(
                  <circle
                    cx={
                      frame.animate_runtime(
                        AnimateRuntimeInput {
                          on_second: (self.total_duration - 5.0).max(0.0),
                          from: circle.cx,
                          to: heart_x,
                          animation_runtime: &HEART_RUNTIME
                        }
                      )
                    }
                    cy={
                      frame.animate_runtime(
                        AnimateRuntimeInput {
                        on_second: (self.total_duration - 5.0).max(0.0),
                        from: circle.cy,
                          to: heart_y,
                          animation_runtime: &HEART_RUNTIME
                        }
                      )
                    }
                    r={circle.r}
                    opacity={circle.opacity}
                    fill={circle.fill.as_str()}
                    filter={circle.filter.as_str()}
                    transform={
                      Transform {
                        translate_x: deviation * circle.amplitude_factor_x,
                        translate_y: deviation * circle.amplitude_factor_y,
                        ..Default::default()
                      }
                    }
                  />
                )
              }).collect::<Svgr>()}
            </g>

           {ctx.render_scenes(&frame)}
          </svg>
        )
    }
}
