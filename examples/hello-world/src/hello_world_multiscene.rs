use fframes::{animation, AudioMap, FFramesContext, Frame, Scene, Svgr};
pub use fframes::{Color, Video};

pub struct HelloWorldMultiSceneVideo {}

#[derive(Debug)]
struct SceneOne {}

impl Scene for SceneOne {
    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(15.)
    }

    fn render_frame(&self, _frame: Frame, _ctx: &FFramesContext) -> Svgr {
        fframes::svgr!(
            <text font-family="Dm Sans" x="100" y="300" font-size="150"> "hello scene 1" </text>
            <g id="g1" transform="scale(1)">
                <rect id="rect1" x="0" y="0" width="120" height="120" fill="green" />
            </g>
            <g id="g1" transform="rotate(45)" transform-origin="top left">
                <rect id="rect1" x="0" y="0" width="120" height="120" fill="green" />
            </g>
        )
    }
}

#[derive(Debug)]
struct SceneTwo {}

impl Scene for SceneTwo {
    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(15.)
    }

    fn render_frame(&self, frame: Frame, _ctx: &FFramesContext) -> fframes::Svgr {
        fframes::svgr!(
          <text
            x="100"
            font-size="150"
            font-family="DM Sans"
            y={frame.animate(fframes::timeline!(on 0., val 300. => 320., animation::Easing::Linear(0.2)))}
          >
            "Hello Scene 2"
          </text>
        )
    }
}

impl Video for HelloWorldMultiSceneVideo {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Auto
    }

    fn audio(&self) -> AudioMap {
        AudioMap::none()
    }

    fn define_scenes(&self) -> fframes::Scenes {
        let vec: Vec<&dyn Scene> = vec![&SceneOne {}, &SceneTwo {}];

        fframes::Scenes::from(vec)
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        const BACKGROUND_EASING: animation::Easing = animation::Easing::Linear(5.);

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

            {ctx.render_scenes(&frame)}

            <text font-weight="500" font-family="JetBrains Mono" x="100" y="440" font-size="74" fill="#4b5563">
              {format!(
                  "This frame index: {}, second: {:.2}",
                  frame.index,
                  frame.get_current_second()
              )}
            </text>
          </svg>
        )
    }
}
