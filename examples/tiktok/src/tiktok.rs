use fframes::{
    AudioMap, FFramesContext, Frame, SampleSize, Svgr, Video, VisualizeFrameInput,
    center_spectrum_low_frequencies, svgr,
};
pub use tiktok_media::GooseMedia;

const BAR_SIZE: usize = 30;
const BAR_PADDING: usize = 20;
const SPECTRUM_WIDTH: usize = 16 * (BAR_SIZE + BAR_PADDING) - BAR_PADDING;

#[derive(Debug)]
pub struct GooseVideo<'a> {
    pub media: &'a GooseMedia,
}

impl Video for GooseVideo<'_> {
    const FPS: usize = 60;
    const WIDTH: usize = 1080;
    const HEIGHT: usize = 1920;

    fn duration(&self) -> fframes::Duration {
        fframes::Duration::FromAudio("thought.mp3")
    }

    fn audio(&self) -> AudioMap {
        use fframes::AudioTimestamp::*;

        AudioMap::from([("thought.mp3", (Frame(0)..Eof))])
    }

    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let audio_visualization = frame.visualize_audio_frame(VisualizeFrameInput {
            audio: &self.media.thought_mp3,
            sample_size: SampleSize::S32,
            smooth_level: 4,
            window: None,
        });

        let audio_visualization = center_spectrum_low_frequencies(audio_visualization.as_slice());

        let svgr = svgr!(
            <svg width="1080" height="1920" fill="none" xmlns="http://www.w3.org/2000/svg">
                <g clip-path="url(#a)">
                    <path fill="#000" d="M0 0h1080v1920H0z"/>
                    <g style="mix-blend-mode:hard-light" filter="url(#b)">
                    <ellipse cx="449.658" cy="652.599" rx="311.126" ry="316.462" transform="rotate(70 449.658 652.599)" fill="#B011E8"/>
                    </g>
                 <g filter="url(#c)"><ellipse rx="208.812" ry="211.997" transform="matrix(.00844 .99996 -.99984 .01802 686.793 1443.34)" fill="#B310FF"/></g><g style="mix-blend-mode:lighten" opacity=".5" filter="url(#d)"><ellipse rx="176.773" ry="179.55" transform="matrix(.00844 .99996 -.99984 .01802 406.049 927.243)" fill="#F90"/></g><g style="mix-blend-mode:hard-light" filter="url(#e)"><ellipse cx="565.155" cy="1157.97" rx="379.483" ry="380.775" transform="rotate(70 565.155 1157.97)" fill="#454ACF"/></g></g>
                 <defs>
                   <filter id="b" x="-66.271" y="140.76" width="1031.86" height="1023.68" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB"><feFlood flood-opacity="0" result="BackgroundImageFix"/><feBlend in="SourceGraphic" in2="BackgroundImageFix" result="shape"/><feGaussianBlur stdDeviation="100" result="effect1_foregroundBlur_3_2"/></filter><filter id="c" x="194.823" y="954.499" width="983.941" height="977.681" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB"><feFlood flood-opacity="0" result="BackgroundImageFix"/><feBlend in="SourceGraphic" in2="BackgroundImageFix" result="shape"/><feGaussianBlur stdDeviation="140" result="effect1_foregroundBlur_3_2"/></filter><filter id="d" x="26.522" y="550.447" width="759.054" height="753" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB"><feFlood flood-opacity="0" result="BackgroundImageFix"/><feBlend in="SourceGraphic" in2="BackgroundImageFix" result="shape"/><feGaussianBlur stdDeviation="100" result="effect1_foregroundBlur_3_2"/></filter><filter id="e" x="-15.573" y="578.228" width="1161.46" height="1159.48" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB"><feFlood flood-opacity="0" result="BackgroundImageFix"/><feBlend in="SourceGraphic" in2="BackgroundImageFix" result="shape"/><feGaussianBlur stdDeviation="100" result="effect1_foregroundBlur_3_2"/></filter><clipPath id="a"><path fill="#fff" d="M0 0h1080v1920H0z"/></clipPath>
                 </defs>
                 {
                     audio_visualization.iter().enumerate().map(|(i, value)| {
                         let height= (value * 2000.).clamp(30., 400.);
                         svgr!(
                             <rect
                                 x={i * (BAR_SIZE + BAR_PADDING) + (Self::WIDTH - SPECTRUM_WIDTH) / 2}
                                 y={300. - height / 2. }
                                 width={BAR_SIZE}
                                 rx="15"
                                 fill="white"
                                 height={height}
                             />
                         )
                     })
                     .collect::<Vec<_>>()
                 }

                {frame.text_break_lines(
                    ctx,
                    frame.get_subtitle_phrase(&self.media.thought_vtt).unwrap_or(""),
                    fframes::BreakLinesOpts {
                      width: 1000,
                      line_height: 1.2,
                      x: "40",
                      y: "34%",
                      font_size: 100,
                      font_family: "JetBrains Mono",
                      align: fframes::TextAlign::Center,
                      fill: "white",
                      font_weight: 400,
                      ..Default::default()
                    },
                ).unwrap_or_default()}

                <image
                  width="950"
                  height="950"
                  href={self.media.goose2_png.href()}
                  y={1920 - 950}
                  x={1080 / 2 - 400}
                />
        </svg>
        );
        svgr
    }
}
