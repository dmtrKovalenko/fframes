use fframes::{
    AudioMap, Color, FFramesContext, Frame, Transform, Video, animation::Easing, include_media_dir,
};

include_media_dir!(pub struct HelloWorldMedia, "examples/hello-world/media");

#[derive(Debug)]
pub struct HelloWorldVideo<'a> {
    pub slug: &'a str,
    pub media: &'a HelloWorldMedia,
}

impl Video for HelloWorldVideo<'_> {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(30.)
    }

    fn audio(&self) -> AudioMap {
        AudioMap::none()
    }

    fn render_frame(&self, frame: Frame, ctx: &FFramesContext) -> fframes::Svgr {
        fframes::svgr!(
            <svg
                xmlns="http://www.w3.org/2000/svg"
                viewBox="0 0 1920 1080"
                width={ctx.current_video_size.width}
                height={ctx.current_video_size.height}
            >
                <rect
                    width={ctx.current_video_size.width}
                    height={ctx.current_video_size.height}
                    x="0"
                    y="0"
                    fill={
                        frame.animate(&fframes::timeline!(
                            at 0., animate Color::hex("#fff") => Color::hex("#f8fafc"), Easing::Linear,
                            at 5., animate Color::hex("#f8fafc") => Color::hex("#fff7ed"), Easing::Linear,
                            at 10., animate Color::hex("#fff7ed") => Color::hex("#fef2f2"), Easing::Linear,
                            at 15., animate Color::hex("#fef2f2") => Color::hex("#f7fee7"), Easing::Linear,
                            at 20., animate Color::hex("#f7fee7") => Color::hex("#ecfdf5"), Easing::Linear,
                            at 25. => 30., animate Color::hex("#ecfdf5") => Color::hex("#faf5ff"), Easing::Linear
                        ))
                    }
                />

                <text font-family="DM Sans" x="100" y="300" font-size="150" fill="#000">
                    "Hello " {self.slug}
                </text>

                <rect
                    x="400"
                    y="400"
                    width="200"
                    height="200"
                    fill="blue"
                    transform={frame.animate(
                        fframes::timeline!(
                            at 0., animate Transform::translate(0, 0) => Transform::translate(200,480), Easing::Linear,
                            at 2. => 6.0, animate Transform::translate(200, 480) => Transform::translate(750,-400), Easing::Linear,
                            // move it the bottom most corner
                            at 6.0 => 10.0, animate Transform::translate(750, -400) => Transform::translate(1310, 480), Easing::Linear,
                        )
                    )}
                />

                <text
                    font-weight="500"
                    font-family="JetBrains Mono"
                    x="100"
                    y="440"
                    font-size="74"
                    fill="#4b5563"
                >
                    {format!("This frame index: {}, second: {:.2}", frame.index, frame.seconds())}
                </text>
            </svg>
        )
    }
}
