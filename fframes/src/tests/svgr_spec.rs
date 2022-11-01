use crate::{animation, AnimateRuntimeInput, Frame};

mod fframes {
    pub use crate::*;
}

use svgr_macro::svgr;
pub struct Ctx;

impl Ctx {
    pub fn get_image_link(&self, a: &str) -> String {
        a.to_owned()
    }
}

#[test]
pub fn macro_animations() {
    let frame = Frame {
        global_index: 50,
        fps: 50,
        index: 75,
    };
    let ctx = Ctx {};

    assert_eq!(
        svgr!(
           <svg
            xmlns="http://www.w3.org/2000/svg"
            xmlns:xlink="http://www.w3.org/1999/xlink"
            width="1920"
            height="1080"
          >
            <image
              width="900"
              height="900"
              xlink:href={ctx.get_image_link("code.png")}
              y="10"
            />

            <rect
                x={frame.animate(fframes::timeline!(
                  on 0., val 10.0 => 12.2, animation::Easing::Linear(0.2),
                  on 10., val 10.0 => 12.2, animation::Easing::Linear(0.2),
                  on 12., val 10.0 => 12.2, animation::Easing::Linear(0.2)
                ))}
            />
          </svg>
        )
        .value,
        r"".to_string()
    );
}

#[test]
pub fn macro_frame_animate_runtime() {
    let frame = Frame {
        fps: 50,
        index: 75,
        global_index: 75,
    };

    assert_eq!(
        svgr!(
          <rect
            transform-origin="center center"
            x={frame.animate_runtime(
              AnimateRuntimeInput {
                on: 16.0,
                from: 100.,
                to: 944.,
                animation_runtime: &crate::AnimationRuntime::Static(0.),
              }
            )}
          />
        )
        .value,
        r"".to_string()
    );
}
