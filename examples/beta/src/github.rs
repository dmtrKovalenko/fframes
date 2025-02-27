use fframes::{Scene, animation, svgr};

#[derive(Debug)]
pub struct GithubScene {}

impl Scene for GithubScene {
    fn overlap(&self) -> fframes::Overlap {
        fframes::Overlap::Previous(0.3)
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
                transform={format!("translate({} -{})",
                frame.animate(&fframes::timeline!(
                    on 0.0, val 1920. => 0., &animation::Easing::Spring { mass: 0.3, stiffness: 90., damping: 26. }
                )),
                frame.animate(&fframes::timeline!(
                    on 0.8, val 0. => 1700., &animation::Easing::Linear(5.5)
                )))}
            />
        )
    }
}
