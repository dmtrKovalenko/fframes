use crate::{animation, Frame};

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
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"1920\" height=\"1080\"><image width=\"900\" height=\"900\" xlink:href=\"code.png\" y=\"10\"></image><rect x=\"12.2\"></rect></svg>"
        .to_owned()
    );
}
