use fframes::{svgr, Scene};

#[derive(Debug)]
pub struct GithubScene {}

impl Scene for GithubScene {
    fn overlap(&self) -> fframes::Overlap {
        fframes::Overlap::Previous(0.3)
    }

    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(5.)
    }

    fn render_frame(
        &self,
        frame: fframes::frame::Frame,
        ctx: &fframes::FFramesContext,
    ) -> fframes::Svgr {
        svgr!(
            <image
                width="1920"
                href={ctx.get_image_link("github_screenshot.png")}
                transform={format!("translate({} -{})",
                frame.animate(&fframes::timeline!(
                    on 0.0, val 1920. => 0., &fframes::Easing::Spring2(0.3, 90., 26.)
                )),
                frame.animate(&fframes::timeline!(
                    on 0.8, val 0. => 1700., &fframes::Easing::Linear(5.5)
                )))}
            />
        )
    }
}
