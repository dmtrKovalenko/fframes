use crate::{BetaExamples, IphoneScene};
use fframes::{
    AudioMap, AudioTimestamp, FFramesContext, Frame, Scene, Svgr, Video, animation::Easing,
};
use svgr_macro::{self, svgr};

pub struct BetaVideo<'a> {
    pub iphone_scene: IphoneScene,
    pub beta_examples: BetaExamples<'a>,
}

#[derive(Debug)]
struct HeadingScene {}

impl Scene for HeadingScene {
    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Seconds(3.)
    }

    fn render_frame(&self, frame: Frame, _ctx: &FFramesContext) -> Svgr<'_> {
        svgr!(
            <text
                font-family="DM Sans"
                font-weight="700"
                x={frame.animate(&fframes::timeline!(
                    at 0., animate -400. => 260., Easing::Spring { mass: 1.0, stiffness: 100., damping: 16. }
                ))}
                y="40%"
                font-size="150"
                fill="#000"
            >
                "Welcome to the beta"
            </text>

            <text
                font-family="DM Sans"
                font-weight="700"
                fill="#000"
                x={frame.animate(&fframes::timeline!(
                    at 0., animate 1000. => 660., Easing::Spring { mass: 1.0, stiffness: 100., damping: 16. }
                ))}
                y="60%"
                font-size="150"
            >
                "of "
                {if frame.index < 100 {
                    svgr!(<tspan> "fframes" </tspan>)
                } else {
                    svgr!(<tspan dy="10" font-size="190" font-weight="normal" font-family="Bubble Bobble"> <tspan fill="#7450d9"> "ff" </tspan> "rames" </tspan>)
                }}
            </text>
        )
    }
}

impl Video for BetaVideo<'_> {
    const FPS: usize = 60;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Auto
    }

    fn audio(&self) -> AudioMap<'_> {
        use AudioTimestamp::*;

        AudioMap::from([
            ("beta.mp3", (Frame(0)..Eof)),
            ("pop.mp3", (Frame(95)..Eof)),
            ("pop.mp3", (Frame(440)..Eof)),
        ])
    }

    fn define_scenes(&self) -> fframes::Scenes<'_> {
        let vec: Vec<&dyn Scene> = vec![
            &HeadingScene {},
            &crate::code_demo::CodeDemoScene {},
            &crate::rendering::RenderingScene {},
            &self.iphone_scene,
            &crate::github::GithubScene {},
            &self.beta_examples,
            &crate::end::EndScene {},
        ];

        fframes::Scenes::from(vec)
    }

    fn render_frame<'a>(&self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        svgr!(
            <svg
                xmlns="http://www.w3.org/2000/svg"
                width={Self::WIDTH}
                height={Self::HEIGHT}
            >
                <image
                    width={Self::WIDTH}
                    height={Self::HEIGHT}
                    x="0"
                    y="0"
                    fill="#fff"
                    href={ctx.get_image("background.png").expect("Do not use .expect() om media in the real code").href()}
                />

                {ctx.render_scenes(&frame)}
            </svg>
        )
    }
}
