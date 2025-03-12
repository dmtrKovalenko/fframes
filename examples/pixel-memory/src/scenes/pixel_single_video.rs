use fframes::{
    AudioMap, FFramesSyncedVideoFrame, Scene, Svgr, SyncVideoFrameInput, animation::Easing,
    timeline,
};

#[derive(Debug)]
pub struct PixelSingleVideoScene<'a> {
    pub video: &'a str,
}

impl Scene for PixelSingleVideoScene<'_> {
    fn duration(&self) -> fframes::Duration {
        fframes::Duration::FromAudio(self.video)
    }

    fn audio(&self) -> AudioMap {
        use fframes::AudioTimestamp::*;
        AudioMap::from([(self.video, Second(0.)..Eof)])
    }

    fn render_frame<'a>(
        &'a self,
        frame: fframes::Frame,
        ctx: &fframes::FFramesContext<'a, '_>,
    ) -> fframes::Svgr<'a> {
        let Some(video_frame) = frame.get_synced_video_frame(
            ctx,
            self.video,
            &SyncVideoFrameInput {
                start_from: 0.,
                looping: false,
                editor_fallback_image: ctx.get_image("10.jpg"),
            },
        ) else {
            return Svgr::empty();
        };

        let video_duration = video_frame.stream_duration_in_seconds();
        let image = video_frame.into_image();

        let opacity = frame.animate(&timeline!(
            at 0. => 0.5, 0. => 1., Easing::EaseIn,
            at video_duration - 0.5 => video_duration, 1. => 0., Easing::EaseOut,
        ));

        let scale_factor = image.metadata.height as f64 / 1080.;
        let scaled_width = image.metadata.width as f64 / scale_factor;
        let center_x = (1920.0 - scaled_width) / 2.0;

        if image.metadata.height > image.metadata.width {
            fframes::svgr!(
                <g>
                    <defs>
                        <filter id="blur-effect">
                            <feGaussianBlur in="SourceGraphic" stdDeviation="30" />
                        </filter>
                        <mask id="edge-mask">
                            <rect x="0" y="0" width="1920" height="1080" fill="white" />
                            <rect
                                x={center_x}
                                y="0"
                                width={scaled_width}
                                height="1080"
                                fill="black"
                            />
                        </mask>
                    </defs>

                    <image
                        id="main-image"
                        x={center_x}
                        y="0"
                        width={scaled_width}
                        height="1080"
                        href={image.href()}
                        opacity={opacity}
                    />

                    <image
                        href={image.href()}
                        x="0"
                        y="0"
                        width="1920"
                        height="1080"
                        preserveAspectRatio="xMidYMid slice"
                        opacity={opacity * 0.7}
                        filter="url(#blur-effect)"
                        mask="url(#edge-mask)"
                    />
                </g>
            )
        } else {
            fframes::svgr!(
                <image
                    x={center_x}
                    y="0"
                    width={scaled_width}
                    height="1080"
                    href={image.href()}
                    opacity={opacity}
                />
            )
        }
    }
}
