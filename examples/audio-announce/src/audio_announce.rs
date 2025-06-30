use crate::svg_waves::frequencies_to_path;
use fframes::{
    AudioMap, FFramesContext, FFramesSyncedVideoFrame, Frame, FrameConvertOptions, Svgr, Video,
    VisualizeFrameInput, include_media_dir, media::ResizeVideoFrame,
};

include_media_dir!(pub struct AudioAnnounceMedia, "examples/audio-announce/media");

#[derive(Debug)]
pub struct AudioAnnounce<'a> {
    /// Font used for text. Defaults to inlined JetBrains Mono.
    pub font: Option<&'a str>,
    pub media: &'a AudioAnnounceMedia,
}

impl AudioAnnounce<'_> {
    fn render_glowing_subtitles<'a>(
        &'a self,
        mut frame: Frame,
        ctx: &FFramesContext<'a, '_>,
    ) -> fframes::Svgr<'a> {
        let phrase = ctx
            .get_subtitles("subtitles.vtt")
            .and_then(|subtitles| frame.get_subtitle_phrase(subtitles))
            .unwrap_or_default();

        fframes::svgr!(
            // Glow effect filter for subtitles
            <filter id="glow" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB">
                <feFlood flood-opacity="0" result="BackgroundImageFix" />
                <feColorMatrix
                    in="SourceAlpha"
                    type="matrix"
                    values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 127 0"
                    result="hardAlpha"
                />
                <feOffset />
                <feGaussianBlur stdDeviation="18.3" />
                <feComposite in2="hardAlpha" operator="out" />
                <feColorMatrix
                    type="matrix"
                    values="0 0 0 0 0.654173 0 0 0 0 0.116095 0 0 0 0 0.56808 0 0 0 1 0"
                />
                <feBlend mode="normal" in2="BackgroundImageFix" result="effect1_dropShadow_8_18" />
                <feBlend mode="normal" in="SourceGraphic" in2="effect1_dropShadow_8_18" result="shape" />
            </filter>

            // Glowing subtitle text with pink stroke
            <g filter="url(#glow)" stroke="#FF208B" stroke-width="1">
                {frame.text_break_lines(
                    ctx,
                    phrase,
                    fframes::BreakLinesOpts {
                        width: 1400,
                        line_height: 1.2,
                        x: 450,
                        y: 170,
                        font: fframes::FontQuery {
                            size: 120,
                            weight: 400,
                            family: self.font.unwrap_or("JetBrains Mono"),
                            ..Default::default()
                        },
                        align: fframes::TextAlign::Left,
                        dominant_baseline: "middle",
                        fill: "white",
                        ..Default::default()
                    },
                ).unwrap_or_default()}
            </g>
        )
    }
}

impl Video for AudioAnnounce<'_> {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Auto
    }

    fn audio(&self) -> AudioMap {
        use fframes::AudioTimestamp::*;
        AudioMap::from([("audio.mp3", Second(0.)..Eof)])
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> fframes::Svgr<'a> {
        const AVATAR_SIZE: u32 = 300;
        const AVATAR_X: usize = 80;
        const AVATAR_Y: usize = 100;

        let video_frame = {
            frame.get_synced_video_frame(
                ctx,
                "transparent.mov",
                &fframes::SyncVideoFrameInput {
                    looping: true,
                    ..Default::default()
                },
            )
        };

        let visualisation = frame.visualize_audio_frame(VisualizeFrameInput {
            smooth_level: 4,
            // safe to unwrap because used in the `audio` method
            audio: ctx.get_audio("audio.mp3").expect("audio.mp3 not found"),
            sample_size: fframes::SampleSize::S512,
            window: Some(fframes::WindowFunction::Hann),
        });

        fframes::svgr!(
            <svg
                xmlns="http://www.w3.org/2000/svg"
                width={Self::WIDTH}
                height={Self::HEIGHT}
            >
                // Background image
                <image
                    x="0"
                    y="0"
                    width={Self::WIDTH}
                    height={Self::HEIGHT}
                    href="background.png"
                />

                // Subtitle text with glow effect
                {self.render_glowing_subtitles(frame, ctx)}

                // Avatar/speaker video (conditional)
                {if let Some(video_frame) = video_frame {
                    let image = video_frame.into_resized_image(&FrameConvertOptions {
                        resize: ResizeVideoFrame {
                            width: AVATAR_SIZE,
                            height: AVATAR_SIZE,
                        }
                    }).unwrap().href();

                    fframes::svgr!(
                        <image
                            filter="url(#glow)"
                            id="avatar"
                            x={AVATAR_X}
                            y={AVATAR_Y}
                            width={AVATAR_SIZE}
                            height={AVATAR_SIZE}
                            href={image}
                        />
                    )
                } else {
                    Svgr::default()
                }}

                // Audio visualization wave layers
                <g opacity="0.5" stroke="#fff" stroke-width="6">
                    // Deep purple wave (base layer)
                    <path
                        fill="#4C20A8"
                        d={frequencies_to_path(100, 1200., visualisation.iter().copied().step_by(2))}
                    />
                    // Medium purple wave (mid layer)
                    <path
                        fill="mediumpurple"
                        d={frequencies_to_path(100, 400., visualisation.iter().copied().step_by(4))}
                    />
                    // Pink wave with stroke (top layer)
                    <path
                        fill="#db2777"
                        stroke="#fff"
                        d={frequencies_to_path(100, 1200., visualisation.iter().copied().step_by(6))}
                    />
                    // Orange wave (reversed, left side)
                    <path
                        fill="#ea580c"
                        transform="translate(-1600, 0)"
                        d={frequencies_to_path(100, 1200., visualisation.into_iter().rev().step_by(7))}
                    />
                </g>
            </svg>
        )
    }
}
