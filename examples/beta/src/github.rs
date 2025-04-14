use fframes::{Overlap, Scene, Transform, animation::Easing, svgr};

#[derive(Debug)]
pub struct GithubScene {}

impl Scene for GithubScene {
    fn overlap(&self) -> fframes::Overlap {
        Overlap::Previous(0.3)
    }

    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(5.)
    }

    fn render_frame(&self, frame: fframes::Frame, ctx: &fframes::FFramesContext) -> fframes::Svgr {
        let github_image = if let Some(github_image) = ctx.get_image("github_screenshot.png") {
            github_image.href()
        } else {
            return fframes::Svgr::default();
        };

        svgr!(
            <image
                width="1920"
                href={github_image}
                transform={
                    Transform::translate(
                        frame.animate(&fframes::timeline!(
                            at 0.0, animate 1920. => 0., Easing::Spring { mass: 0.3, stiffness: 90., damping: 26. }
                        )),
                        frame.animate(&fframes::timeline!(
                            at 0.8 => 6.3, animate 0. => -1700., Easing::Linear
                        ))
                    )
                }
            />
        )
    }
}
