use crate::svg_waves::frequencies_to_path;
use fframes::{include_media_dir, AudioMap, FFramesContext, Frame, Video, VisualizeFrameInput};

include_media_dir!(pub struct AudioAnnounceMedia, "examples/audio-announce/media");

#[derive(Debug)]
pub struct AudioAnnounce<'a> {
    /// Font used for text. Defaults to inlined JetBrains Mono.
    pub font: Option<&'a str>,
    pub media: &'a AudioAnnounceMedia,
}

impl AudioAnnounce<'_> {
    fn render_glowing_subtitles(&self, mut frame: Frame, ctx: &FFramesContext) -> fframes::Svgr {
        let phrase = ctx
            .get_subtitles("subtitles.vtt")
            .and_then(|subtitles| frame.get_subtitle_phrase(subtitles))
            .unwrap_or_default();

        fframes::svgr!(
            <filter id="glow" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB">
                <feFlood flood-opacity="0" result="BackgroundImageFix" />
                <feColorMatrix in="SourceAlpha" type="matrix" values="0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 127 0" result="hardAlpha" />
                <feOffset />
                <feGaussianBlur stdDeviation="18.3" />
                <feComposite in2="hardAlpha" operator="out" />
                <feColorMatrix type="matrix" values="0 0 0 0 0.654173 0 0 0 0 0.116095 0 0 0 0 0.56808 0 0 0 1 0" />
                <feBlend mode="normal" in2="BackgroundImageFix" result="effect1_dropShadow_8_18" />
                <feBlend mode="normal" in="SourceGraphic" in2="effect1_dropShadow_8_18" result="shape" />
            </filter>

            <g filter="url(#glow)" stroke="#FF208B" stroke-width="1">
                {frame.text_break_lines(
                    ctx,
                    phrase,
                    &fframes::BreakLinesOpts {
                      width: 1400,
                      line_height: 1.2,
                      x: "420",
                      y: "170",
                      font_size: 120,
                      font_family: self.font.unwrap_or("JetBrains Mono"),
                      align: fframes::TextAlign::Left,
                      dominant_baseline: "middle",
                      fill: "white",
                      font_weight: 400,
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
        AudioMap::from([("audio.wav", Second(0.)..Eof)])
    }

    fn render_frame(&self, frame: Frame, ctx: &FFramesContext) -> fframes::Svgr {
        const AVATAR_SIZE: usize = 300;
        const AVATAR_X: usize = 80;
        const AVATAR_Y: usize = 85;

        let visualisation = frame.visualize_audio_frame(VisualizeFrameInput {
            // safe to unwrap because used in the `audio` method
            audio: ctx.get_audio("audio.wav").expect("audio.wav not found"),
            sample_size: fframes::SampleSize::S512,
            smooth_level: 4,
            window: Some(fframes::WindowFunction::Hann),
        });

        fframes::svgr!(
           <svg
            xmlns="http://www.w3.org/2000/svg"
            width={Self::WIDTH}
            height={Self::HEIGHT}
          >
            <image
                x="0"
                y="0"
                width={Self::WIDTH}
                height={Self::HEIGHT}
                href={self.media.background_png.href()}
            />

            {self.render_glowing_subtitles(frame, ctx)}
            <image
                filter="url(#glow)"
                id="avatar"
                x={AVATAR_X}
                y={AVATAR_Y}
                width={AVATAR_SIZE}
                height={AVATAR_SIZE}
                href={self.media.avatar_png.href()}
            />

             <g opacity="0.5" stroke="#fff" stroke-width="6">
                 // Rendering actual waves paths. All these values are tuned imperatively
                 // to achieve a good look no science behind them.
                 <path
                   fill="#4C20A8"
                   d={frequencies_to_path(100, 1200., visualisation.iter().copied().step_by(2))}
                 />
                 <path
                   fill="mediumpurple"
                   d={frequencies_to_path(100, 400., visualisation.iter().copied().step_by(4))}
                 />
                 <path
                   fill="#db2777"
                   stroke="#fff"
                   d={frequencies_to_path(100, 1200., visualisation.iter().copied().step_by(6))}
                 />
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
