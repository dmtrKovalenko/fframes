use fframes::{
    animation::{self, AnimationRuntime},
    AudioMap, AudioTimestamp, Svgr,
};
pub use fframes::{
    audio_data, audio_window_functions, fframes_context, frame::Frame, subtitles, video::Video,
};
use lazy_static::lazy_static;
use svgr_macro::{self, svgr};

const SPRING: animation::Easing = animation::Easing::Spring2(1.85, 130., 16.);

lazy_static! {
    static ref SPRING_RUNTIME: AnimationRuntime = AnimationRuntime::from_easing(&SPRING);
}

#[derive(Debug)]
pub struct GooseVideo {
    pub audio_track: &'static str,
}

impl Video for GooseVideo {
    const FPS: usize = 60;
    const WIDTH: usize = 1080;
    const HEIGHT: usize = 1920;
    // const DURATION: fframes::Duration = fframes::Duration::FromAudio("thought.mp3");
    const DURATION: fframes::Duration = fframes::Duration::Seconds(400);

    fn audio(&self) -> AudioMap {
        use AudioTimestamp::{Eof, Second};

        AudioMap::from([("thought.mp3", (Second(0), Eof))])
    }

    fn render_frame(&self, frame: Frame, ctx: &fframes_context::FFramesContext) -> Svgr {
        let audio_visualization = frame.visualize_audio_frame(audio_data::VisualizeFrameInput {
            audio: ctx.get_audio_data(self.audio_track),
            sample_size: audio_data::SampleSize::S64,
            ctx,
            smooth_level: 4,
            window: None,
        });

        svgr!(
            <svg width="1080" height="1920" fill="none" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
                <g clip-path="url(#a)">
                    <path fill="#000" d="M0 0h1080v1920H0z"/>
                    <g style="mix-blend-mode:hard-light" filter="url(#b)">
                    <ellipse cx="449.658" cy="652.599" rx="311.126" ry="316.462" transform="rotate(70 449.658 652.599)" fill="#B011E8"/>
                    </g>
                 <g filter="url(#c)"><ellipse rx="208.812" ry="211.997" transform="matrix(.00844 .99996 -.99984 .01802 686.793 1443.34)" fill="#B310FF"/></g><g style="mix-blend-mode:lighten" opacity=".5" filter="url(#d)"><ellipse rx="176.773" ry="179.55" transform="matrix(.00844 .99996 -.99984 .01802 406.049 927.243)" fill="#F90"/></g><g style="mix-blend-mode:hard-light" filter="url(#e)"><ellipse cx="565.155" cy="1157.97" rx="379.483" ry="380.775" transform="rotate(70 565.155 1157.97)" fill="#454ACF"/></g></g>
                 <defs>
                   <filter id="b" x="-66.271" y="140.76" width="1031.86" height="1023.68" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB"><feFlood flood-opacity="0" result="BackgroundImageFix"/><feBlend in="SourceGraphic" in2="BackgroundImageFix" result="shape"/><feGaussianBlur stdDeviation="100" result="effect1_foregroundBlur_3_2"/></filter><filter id="c" x="194.823" y="954.499" width="983.941" height="977.681" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB"><feFlood flood-opacity="0" result="BackgroundImageFix"/><feBlend in="SourceGraphic" in2="BackgroundImageFix" result="shape"/><feGaussianBlur stdDeviation="140" result="effect1_foregroundBlur_3_2"/></filter><filter id="d" x="26.522" y="550.447" width="759.054" height="753.593" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB"><feFlood flood-opacity="0" result="BackgroundImageFix"/><feBlend in="SourceGraphic" in2="BackgroundImageFix" result="shape"/><feGaussianBlur stdDeviation="100" result="effect1_foregroundBlur_3_2"/></filter><filter id="e" x="-15.573" y="578.228" width="1161.46" height="1159.48" filterUnits="userSpaceOnUse" color-interpolation-filters="sRGB"><feFlood flood-opacity="0" result="BackgroundImageFix"/><feBlend in="SourceGraphic" in2="BackgroundImageFix" result="shape"/><feGaussianBlur stdDeviation="100" result="effect1_foregroundBlur_3_2"/></filter><clipPath id="a"><path fill="#fff" d="M0 0h1080v1920H0z"/></clipPath>
                 </defs>

                    <text
                        font-size="100"
                        y="15%"
                        x="50%"
                        text-anchor="middle"
                        fill="white"
                        font-family="JetBrains Mono"
                    >
                     ".mp3 and .mp4"
                    </text>



                    <text
                      font-size="90"
                      y="25%"
                      x="50%"
                      text-anchor="middle"
                      fill="white"
                      font-family="JetBrains Mono"
                    >
                      "are not the codecs"
                    </text>

                    <image
              width="1200"
              height="1200"
              xlink:href={ctx.get_image_link("goose.png")}

              y="800"
              x={1080 / 2 - 650}
            />

            {
                audio_visualization.iter().enumerate().skip(1).take(19).map(|(i, value)| {
                    let height= (value * 10.).clamp(30., 400.);
                    svgr!(
                        <rect
                            x={i * 50 + 20}
                            y={800. - height / 2. }
                            width="30"
                            rx="15"
                            fill="white"
                            height={height}
                        />
                    )
                })
                .collect::<Vec<_>>()
            }
        </svg>
        )
    }
}
