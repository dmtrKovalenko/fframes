use fframes::{
    AudioMap, FFramesContext, FFramesSyncedVideoFrame, Frame, Svgr, SyncVideoFrameInput, Video,
};

#[derive(Debug)]
pub struct AnamorphicVideo;

impl Video for AnamorphicVideo {
    const FPS: usize = 30;
    const WIDTH: usize = 1280;
    const HEIGHT: usize = 720;

    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Seconds(1.)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let clip = frame
            .get_synced_video_frame(ctx, "anamorphic.mp4", &SyncVideoFrameInput::default())
            .map_or_else(Svgr::empty, |video| {
                let height = Self::HEIGHT as f32;
                let width = video.width() as f32 * height / video.height() as f32;
                let image = video.into_image();
                fframes::svgr!(
                    <image
                        href={image.href()}
                        x={(Self::WIDTH as f32 - width) / 2.}
                        width={width}
                        height={height}
                    />
                )
            });

        fframes::svgr!(
            <svg
                xmlns="http://www.w3.org/2000/svg"
                viewBox="0 0 1280 720"
                width={ctx.current_video_size.width}
                height={ctx.current_video_size.height}
            >
                <rect width="1280" height="720" fill="#000" />
                {clip}
                <circle cx="640" cy="360" r="280" fill="none" stroke="#f43f5e" stroke-width="4" />
            </svg>
        )
    }
}
