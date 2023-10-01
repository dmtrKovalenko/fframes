pub use fframes::{animation, AnimateRuntimeInput, AudioMap, Frame, Svgr, Video};
use fframes::{include_media_dir, FFramesContext, VisualizeFrameInput};

const SPRING: animation::Easing = animation::Easing::Spring2(1.85, 130., 16.);

include_media_dir!(pub struct MarketingMedia, "examples/marketing/media");

fframes::lazy_static::lazy_static! {
    static ref SPRING_RUNTIME: animation::AnimationRuntime = animation::AnimationRuntime::from(&SPRING);
}

struct SpectrumValue<'a> {
    /// position represents how points are displayed on the screen, while the real array position represents stacking context.
    position: usize,
    spectrum_index: usize,
    from_color: &'a str,
    to_color: &'a str,
}

#[derive(Debug)]
pub struct MarketingVideo<'a> {
    pub audio_track: &'a str,
    pub media: &'a MarketingMedia,
}

impl MarketingVideo<'_> {
    fn render_ferris(&self, frame: &Frame) -> Svgr {
        if !(2.3f32..5.0f32).contains(&frame.get_current_second()) {
            return Svgr::default();
        }

        fframes::svgr!(
          <svg
            viewBox="0 0 1200 800"
            width="400"
            height="400"
            x="1456"
            y={frame.animate(fframes::timeline!(
                on 2.3, val 1400. => 770., animation::Easing::Spring2(1.85, 130.0, 16.0),
                on 4.8, val 770. => 1400., animation::Easing::Spring2(1.85, 130.0, 16.0)
            ))}
            xmlns="http://www.w3.org/2000/svg"
            fill-rule="evenodd"
            clip-rule="evenodd"
            stroke-linejoin="round"
            stroke-miterlimit="1.41421"
         >
            <g id="Layer-1">
                <g transform="matrix(1,0,0,1,597.344,637.02)">
                    <path
                        d="M0,-279.559C-121.238,-279.559 -231.39,-264.983 -312.939,-241.23L-312.939,-38.329C-231.39,-14.575 -121.238,0 0,0C138.76,0 262.987,-19.092 346.431,-49.186L346.431,-230.37C262.987,-260.465 138.76,-279.559 0,-279.559"
                        style="fill:rgb(165,43,0);fill-rule:nonzero;" />
                </g>
                <g transform="matrix(1,0,0,1,1068.75,575.642)">
                    <path
                        d="M0,-53.32L-14.211,-82.761C-14.138,-83.879 -14.08,-84.998 -14.08,-86.121C-14.08,-119.496 -48.786,-150.256 -107.177,-174.883L-107.177,2.643C-79.932,-8.849 -57.829,-21.674 -42.021,-35.482C-46.673,-16.775 -62.585,21.071 -75.271,47.686C-96.121,85.752 -103.671,118.889 -102.703,120.53C-102.086,121.563 -94.973,110.59 -84.484,92.809C-60.074,58.028 -13.82,-8.373 -4.575,-25.287C5.897,-44.461 0,-53.32 0,-53.32"
                        style="fill:rgb(165,43,0);fill-rule:nonzero;" />
                </g>
                <g transform="matrix(1,0,0,1,149.064,591.421)">
                    <path
                        d="M0,-99.954C0,-93.526 1.293,-87.194 3.788,-80.985L-4.723,-65.835C-4.723,-65.835 -11.541,-56.989 0.465,-38.327C11.055,-21.872 64.1,42.54 92.097,76.271C104.123,93.564 112.276,104.216 112.99,103.187C114.114,101.554 105.514,69.087 81.631,32.046C70.487,12.151 57.177,-14.206 49.189,-33.675C71.492,-19.559 100.672,-6.755 135.341,4.265L135.341,-204.17C51.797,-177.622 0,-140.737 0,-99.954"
                        style="fill:rgb(165,43,0);fill-rule:nonzero;" />
                </g>
                <g transform="matrix(-65.8097,-752.207,-752.207,65.8097,621.707,796.312)">
                    <path
                        d="M0.991,-0.034L0.933,0.008C0.933,0.014 0.933,0.02 0.933,0.026L0.99,0.069C0.996,0.073 0.999,0.08 0.998,0.087C0.997,0.094 0.992,0.1 0.986,0.103L0.92,0.133C0.919,0.139 0.918,0.145 0.916,0.15L0.964,0.203C0.968,0.208 0.97,0.216 0.968,0.222C0.965,0.229 0.96,0.234 0.953,0.236L0.882,0.254C0.88,0.259 0.877,0.264 0.875,0.27L0.91,0.33C0.914,0.336 0.914,0.344 0.91,0.35C0.907,0.356 0.9,0.36 0.893,0.361L0.82,0.365C0.817,0.369 0.813,0.374 0.81,0.379L0.832,0.445C0.835,0.452 0.833,0.459 0.828,0.465C0.824,0.47 0.816,0.473 0.809,0.472L0.737,0.462C0.733,0.466 0.729,0.47 0.724,0.474L0.733,0.544C0.734,0.551 0.731,0.558 0.725,0.562C0.719,0.566 0.711,0.568 0.704,0.565L0.636,0.542C0.631,0.546 0.626,0.549 0.621,0.552L0.615,0.621C0.615,0.629 0.61,0.635 0.604,0.638C0.597,0.641 0.589,0.641 0.583,0.638L0.521,0.602C0.52,0.603 0.519,0.603 0.518,0.603L0.406,0.729C0.406,0.729 0.394,0.747 0.359,0.725C0.329,0.705 0.206,0.599 0.141,0.543C0.109,0.52 0.089,0.504 0.09,0.502C0.093,0.499 0.149,0.509 0.217,0.554C0.278,0.588 0.371,0.631 0.38,0.619C0.38,0.619 0.396,0.604 0.406,0.575C0.406,0.575 0.406,0.575 0.406,0.575C0.407,0.576 0.407,0.576 0.406,0.575C0.406,0.575 0.091,0.024 0.305,-0.531C0.311,-0.593 0.275,-0.627 0.275,-0.627C0.266,-0.639 0.178,-0.598 0.12,-0.566C0.055,-0.523 0.002,-0.513 0,-0.516C-0.001,-0.518 0.018,-0.533 0.049,-0.555C0.11,-0.608 0.227,-0.707 0.256,-0.726C0.289,-0.748 0.301,-0.73 0.301,-0.73L0.402,-0.615C0.406,-0.614 0.41,-0.613 0.415,-0.613L0.47,-0.658C0.475,-0.663 0.483,-0.664 0.49,-0.662C0.497,-0.66 0.502,-0.655 0.504,-0.648L0.522,-0.58C0.527,-0.578 0.533,-0.576 0.538,-0.574L0.602,-0.608C0.608,-0.612 0.616,-0.612 0.623,-0.608C0.629,-0.605 0.633,-0.599 0.633,-0.592L0.637,-0.522C0.642,-0.519 0.647,-0.515 0.652,-0.512L0.721,-0.534C0.728,-0.536 0.736,-0.535 0.741,-0.531C0.747,-0.526 0.75,-0.519 0.749,-0.512L0.738,-0.443C0.742,-0.439 0.746,-0.435 0.751,-0.431L0.823,-0.439C0.83,-0.44 0.837,-0.437 0.842,-0.432C0.847,-0.426 0.848,-0.419 0.845,-0.412L0.821,-0.347C0.824,-0.342 0.828,-0.337 0.831,-0.332L0.903,-0.327C0.911,-0.327 0.917,-0.322 0.92,-0.316C0.924,-0.31 0.924,-0.302 0.92,-0.296L0.883,-0.236C0.885,-0.231 0.887,-0.226 0.889,-0.22L0.959,-0.202C0.966,-0.2 0.972,-0.195 0.974,-0.188C0.976,-0.181 0.974,-0.174 0.969,-0.168L0.92,-0.116C0.921,-0.111 0.923,-0.105 0.924,-0.099L0.988,-0.068C0.995,-0.065 0.999,-0.059 1,-0.052C1.001,-0.045 0.997,-0.038 0.991,-0.034ZM0.406,0.575C0.406,0.575 0.406,0.575 0.406,0.575C0.406,0.575 0.406,0.575 0.406,0.575Z"
                        style="fill:url(#_Linear1);fill-rule:nonzero;" />
                </g>
                <g transform="matrix(1,0,0,1,450.328,483.629)">
                    <path
                        d="M0,167.33C-1.664,165.91 -2.536,165.068 -2.536,165.068L140.006,153.391C23.733,0 -69.418,122.193 -79.333,135.855L-79.333,167.33L0,167.33Z"
                        style="fill-rule:nonzero;" />
                </g>
                <g transform="matrix(1,0,0,1,747.12,477.333)">
                    <path
                        d="M0,171.974C1.663,170.554 2.536,169.71 2.536,169.71L-134.448,159.687C-18.12,0 69.421,126.835 79.335,140.497L79.335,171.974L0,171.974Z"
                        style="fill-rule:nonzero;" />
                </g>
                <g transform="matrix(-1.53e-05,-267.211,-267.211,1.53e-05,809.465,764.23)">
                    <path
                        d="M1,-0.586C1,-0.586 0.768,-0.528 0.524,-0.165L0.5,-0.064C0.5,-0.064 1.1,0.265 0.424,0.731C0.424,0.731 0.508,0.586 0.405,0.197C0.405,0.197 0.131,0.376 0.14,0.736C0.14,0.736 -0.275,0.391 0.324,-0.135C0.324,-0.135 0.539,-0.691 1,-0.736L1,-0.586Z"
                        style="fill:url(#_Linear2);fill-rule:nonzero;" />
                </g>
                <g transform="matrix(1,0,0,1,677.392,509.61)">
                    <path
                        d="M0,-92.063C0,-92.063 43.486,-139.678 86.974,-92.063C86.974,-92.063 121.144,-28.571 86.974,3.171C86.974,3.171 31.062,47.615 0,3.171C0,3.171 -37.275,-31.75 0,-92.063"
                        style="fill-rule:nonzero;" />
                </g>
                <g
                  transform={format!("matrix(1,0,0,1,{},435.209)",
                    frame.animate(&fframes::timeline!(
                        on 3.4, val 727. => 777., animation::Easing::Linear(0.25),
                        on 4.1, val 777. => 727., animation::Easing::Linear(0.25)
                    ))
                  )}
                >
                    <path
                        d="M0,0.002C0,18.543 -10.93,33.574 -24.408,33.574C-37.885,33.574 -48.814,18.543 -48.814,0.002C-48.814,-18.539 -37.885,-33.572 -24.408,-33.572C-10.93,-33.572 0,-18.539 0,0.002"
                        style="fill:white;fill-rule:nonzero;" />
                </g>
                <g transform="matrix(1,0,0,1,483.3,502.984)">
                    <path
                        d="M0,-98.439C0,-98.439 74.596,-131.467 94.956,-57.748C94.956,-57.748 116.283,28.178 33.697,33.028C33.697,33.028 -71.613,12.745 0,-98.439"
                        style="fill-rule:nonzero;"
                    />
                </g>
                <g
                  transform={format!("matrix(1,0,0,1,{},436.428)",
                    frame.animate(&fframes::timeline!(
                      on 3.4, val 520. => 570., animation::Easing::Linear(0.25),
                      on 4.1, val 570. => 520., animation::Easing::Linear(0.25)
                    ))
                  )}
                >
                    <path
                        d="M0,0C0,19.119 -11.27,34.627 -25.173,34.627C-39.071,34.627 -50.344,19.119 -50.344,0C-50.344,-19.124 -39.071,-34.627 -25.173,-34.627C-11.27,-34.627 0,-19.124 0,0"
                        style="fill:white;fill-rule:nonzero;"
                    />
                </g>
                <g transform="matrix(-1.53e-05,-239.021,-239.021,1.53e-05,402.161,775.388)">
                    <path
                        d="M0.367,0.129C-0.364,-0.441 0.223,-0.711 0.223,-0.711C0.259,-0.391 0.472,-0.164 0.472,-0.164C0.521,-0.548 0.525,-0.77 0.525,-0.77C1.203,-0.256 0.589,0.161 0.589,0.161C0.627,0.265 0.772,0.372 0.906,0.451L1,0.77C0.376,0.403 0.367,0.129 0.367,0.129Z"
                        style="fill:url(#_Linear3);fill-rule:nonzero;" />
                </g>
            </g>

            <defs>
                <linearGradient id="_Linear1" x1="0" y1="0" x2="1" y2="0" gradientUnits="userSpaceOnUse"
                    gradientTransform="matrix(1,0,1.38778e-17,-1,0,-0.000650515)">
                    <stop offset="0" style="stop-color:rgb(247,76,0);stop-opacity:1" />
                    <stop offset="0.33" style="stop-color:rgb(247,76,0);stop-opacity:1" />
                    <stop offset="1" style="stop-color:rgb(244,150,0);stop-opacity:1" />
                </linearGradient>
                <linearGradient id="_Linear2" x1="0" y1="0" x2="1" y2="0" gradientUnits="userSpaceOnUse"
                    gradientTransform="matrix(1,0,0,-1,0,1.23438e-06)">
                    <stop offset="0" style="stop-color:rgb(204,58,0);stop-opacity:1" />
                    <stop offset="0.15" style="stop-color:rgb(204,58,0);stop-opacity:1" />
                    <stop offset="0.74" style="stop-color:rgb(247,76,0);stop-opacity:1" />
                    <stop offset="1" style="stop-color:rgb(247,76,0);stop-opacity:1" />
                </linearGradient>
                <linearGradient id="_Linear3" x1="0" y1="0" x2="1" y2="0" gradientUnits="userSpaceOnUse"
                    gradientTransform="matrix(1,1.32349e-23,1.32349e-23,-1,0,-9.1568e-07)">
                    <stop offset="0" style="stop-color:rgb(204,58,0);stop-opacity:1" />
                    <stop offset="0.15" style="stop-color:rgb(204,58,0);stop-opacity:1" />
                    <stop offset="0.74" style="stop-color:rgb(247,76,0);stop-opacity:1" />
                    <stop offset="1" style="stop-color:rgb(247,76,0);stop-opacity:1" />
                </linearGradient>
            </defs>
        </svg>
        )
    }
}

const PRETTY_SPECTRUM: [SpectrumValue; 7] = [
    SpectrumValue {
        position: 6,
        spectrum_index: 6,
        from_color: "#ec4899",
        to_color: "#f43f5e",
    },
    SpectrumValue {
        position: 0,
        spectrum_index: 3,
        from_color: "#d946ef",
        to_color: "#9333ea",
    },
    SpectrumValue {
        position: 5,
        spectrum_index: 5,
        from_color: "#facc15",
        to_color: "#f97316",
    },
    SpectrumValue {
        position: 1,
        spectrum_index: 7,
        from_color: "#38bdf8",
        to_color: "#6366f1",
    },
    SpectrumValue {
        position: 4,
        spectrum_index: 4,
        from_color: "#4ade80",
        to_color: "#06b6d4",
    },
    SpectrumValue {
        position: 2,
        spectrum_index: 2,
        from_color: "#22d3ee",
        to_color: "#0ea5e9",
    },
    SpectrumValue {
        position: 3,
        spectrum_index: 1,
        from_color: "#fff",
        to_color: "#fff",
    },
];

const BAR_SIZE: usize = 96;
const BAR_SIZE_F32: f32 = BAR_SIZE as f32;
const BAR_MARGIN: usize = 20;
const BAR_WIDTH_WITH_MARGIN: usize = BAR_SIZE + BAR_MARGIN;

// viewbox width - space that all bars will take - right margin
const SPECTRUM_LEN: usize = 1920 - BAR_WIDTH_WITH_MARGIN * PRETTY_SPECTRUM.len() - BAR_MARGIN;

impl Video for MarketingVideo<'_> {
    const FPS: usize = 60;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Auto
    }

    fn audio(&self) -> AudioMap {
        use fframes::AudioTimestamp::{Eof, Second};

        AudioMap::from([
            ("marketing.mp3", Second(0.)..Eof),
            ("woosh.mp3", Second(6.)..Eof),
            ("end.mp3", Second(16.)..Eof),
        ])
    }

    fn render_frame(&self, frame: fframes::Frame, _ctx: &FFramesContext) -> Svgr {
        let audio_visualization = frame.visualize_audio_frame(VisualizeFrameInput {
            audio: &self.media.marketing_mp3,
            sample_size: fframes::SampleSize::S16,
            smooth_level: 3,
            window: Some(fframes::WindowFunction::Hamming),
        });

        fframes::svgr!(
          <svg
            xmlns="http://www.w3.org/2000/svg"
            width="1920"
            height="1080"
          >
            <defs>
              <linearGradient id="gradient-test" x1="1" y1="1">
                <stop stop-color="#3a1c71"/>
                <stop offset=".5" stop-color="#d76d77"/>
                <stop offset="1" stop-color="#ffaf7b"/>
              </linearGradient>
              <linearGradient id="cover" gradientUnits="userSpaceOnUse" x1="14.66%" y1="108.81%" x2="85.34%" y2="-8.81%">
                <stop stop-color="#ffa69e"/>
                <stop offset="1" stop-color="#7351d8"/>
              </linearGradient>
              <linearGradient id="anim">
                <stop stop-color="#ec77ab"/>
                <stop offset="1" stop-color="#4F46E5"/>
              </linearGradient>
              <linearGradient id="g1" x1="1" y1="1" x2="0">
                <stop stop-color="#2d3436"/>
                <stop offset=".74" stop-color="#000000"/>
              </linearGradient>
            </defs>


            <rect x="0" y="0" width="100%" height="100%" fill="red" />

            {
              PRETTY_SPECTRUM
              .iter()
              .map(|SpectrumValue { spectrum_index, from_color, to_color, position }|  {
                let val: f32 = audio_visualization[*spectrum_index];

                let bar_height = match val / 20.0 {
                  height if height.is_nan() => BAR_SIZE_F32,
                  height if height < BAR_SIZE_F32 => BAR_SIZE_F32,
                  height if height > 720.0 => 720.0,
                  height => height.to_owned()
                };

                 fframes::svgr!(
                  <filter id={format!("{position}-shadow")} x="-100%" y="-100%" width="300%" height="300%">
                    <feGaussianBlur in="SourceAlpha" stdDeviation="10.4"/>
                    <feOffset dx="0" dy="3" result="offsetblur"/>
                    <feFlood flood-color={to_color}  flood-opacity="0.5" />
                    <feComposite in2="offsetblur" operator="in"/>
                    <feMerge>
                      <feMergeNode/>
                      <feMergeNode in="SourceGraphic"/>
                    </feMerge>
                  </filter>

                  <linearGradient id={format!("{position}-gradient")} y2="1">
                     <stop stop-color={from_color}/>
                     <stop offset="1" stop-color={to_color} />
                  </linearGradient>
                   <rect
                    y={500. - bar_height / 2.0}
                    x={frame.animate_runtime(
                      AnimateRuntimeInput {
                        on_second: 16.0,
                        from: ((SPECTRUM_LEN / 2) + (position * BAR_WIDTH_WITH_MARGIN)) as f32,
                        to: 944.,
                        animation_runtime: &SPRING_RUNTIME,
                      }
                    )}
                    transform-origin="center center"
                    height={bar_height + 8.0}
                    fill="transparent"
                    stroke="white"
                    stroke-width="4"
                    width={BAR_SIZE + 8}
                    rx={BAR_SIZE / 2}
                    ry={BAR_SIZE / 2}
                  />
                  <rect
                    y={500. - bar_height / 2.0}
                    x={frame.animate_runtime(
                      AnimateRuntimeInput {
                        on_second: 16.0,
                        from: ((SPECTRUM_LEN / 2) + (position * BAR_WIDTH_WITH_MARGIN)) as f32,
                        to: 944.,
                        animation_runtime: &SPRING_RUNTIME,
                      }
                    )}
                    transform-origin="center center"
                    fill={format!("url(#{position}-gradient)")}
                    filter={format!("url(#{position}-shadow)")}
                    height={bar_height}
                    width={BAR_SIZE}
                    rx={BAR_SIZE / 2}
                    ry={BAR_SIZE / 2}
                  />
               )
               })
              .collect::<Vec<_>>()
          }

            <svg
              x="200"
              y="290"
              xmlns="http://www.w3.org/2000/svg"
              viewBox="0 0 598.3520004127504 417.989493060112"
              width="298"
              opacity={frame.animate(fframes::timeline!(
                  on 5.8, val 0. => 1., &animation::Easing::Spring2(1.85, 130., 16.),
                  on 9.0, val 1. => 0., &animation::Easing::Spring2(1.85, 130., 16.)
              ))}
            >
              <g stroke-linecap="round" transform="translate(12.76795062351539 11.630295608565234) rotate(0 286.4080495828598 197.36445092149077)">
              <path d="M-2.19 -1.63 C8.29 53.64, -34.21 264.71, 62.09 331.05 C158.39 397.38, 490.59 385.91, 575.58 396.36" stroke="#fff" stroke-width="4.5" fill="none" stroke-dasharray="8 12"></path></g><g transform="translate(12.76795062351539 11.630295608565234) rotate(0 286.4080495828598 197.36445092149077)"><path d="M544.61 406.42 C552.39 404.4, 562.6 396.92, 572.02 395.84" stroke="#fff" stroke-width="4.5" fill="none"></path></g><g transform="translate(12.76795062351539 11.630295608565234) rotate(0 286.4080495828598 197.36445092149077)"><path d="M545.58 385.92 C553.02 389.43, 562.97 387.49, 572.02 395.84" stroke="#fff" stroke-width="4.5" fill="none"></path></g>
            </svg>

            <text
              font-size="64"
              y="87%"
              x="50%"
              text-anchor="middle"
              fill="white"
              font-family="Chalkboard SE"
            >
              {frame.get_subtitle_phrase(&self.media.subtitles_vtt).unwrap_or("")}
            </text>

            <image
              width="900"
              height="900"
              href={self.media.code_png.href()}
              x={frame.animate(fframes::timeline!(
                  on 5.8, val -1000. => 40., &animation::Easing::Spring2(0.85, 80., 16.),
                  on 9.0, val 40. => -1200., &animation::Easing::Spring2(0.85, 80., 16.)
              ))}
              y="10"
            />

            {self.render_ferris(&frame)}

            <circle
              cx={960}
              fill="#fff"
              cy="500"
              r={frame.animate(fframes::timeline!(
                on 16.2, val 0. => 1200.0, &animation::Easing::Linear(0.3)
              ))}
            />

            {if frame.get_current_second() > 16.25  {
              fframes::svgr!(
                <g>
                  <text x="960" y="570" font-family="Bubble Bobble" font-size="154" text-anchor="middle">
                    <tspan fill={if frame.get_current_second() > 18.8 { "#7351d8" } else { "#000" }}>"ff"</tspan>"rames"
                  </text>
                  <text x="960" y="610" font-family="Chalkboard SE" font-size="30" text-anchor="middle">
                    "Write some code. Get video. Enjoy!"
                  </text>

                  <g y="540" transform="scale(2.7) translate(-218, 110)">
                    <path
                     fill="none"
                     stroke="url(#anim)"
                     stroke-width="10"
                     stroke-linecap="round"
                     stroke-linejoin="round"
                     stroke-miterlimit="10"
                     stroke-dashoffset={frame.animate(fframes::timeline!(
                       on 16.3, val -700. => 41.0, &animation::Easing::Linear(2.5)
                     ))}
                     stroke-dasharray="40.4579px, 796.447px"
                     d="M505,55c0-27.6-22.4-50-50-50s-50,22.4-50,50c0-27.6-22.4-50-50-50s-50,22.4-50,50c0-27.6-22.4-50-50-50s-50,22.4-50,50c0-27.6-22.4-50-50-50s-50,22.4-50,50c0-27.6-22.4-50-50-50S5,27.4,5,55"
                    />

                    <path
                      fill="none"
                      stroke="#4F46E5"
                      stroke-width="6"
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      opacity={frame.animate(
                        fframes::timeline!(
                          on 18.6, val 1. => 0.55, &animation::Easing::Linear(0.2)
                        )
                      )}
                      stroke-dashoffset={frame.animate(fframes::timeline!(
                       on 18.7, val -40. => 0.0, &animation::Easing::Linear(0.2)
                      ))}
                      stroke-dasharray={
                        format!("{}, 137px", frame.animate(
                          &fframes::timeline!(on 18.7, val 30.0 => 12.0, &animation::Easing::Linear(0.2)))
                        )
                      }
                      d="M549.7,46.6l-21.8,12.6 M531.9,25.8l-12.6,21.8 M504.2,18.3v25.1 M476.4,25.8L489,47.6 M458.7,46.6l21.8,12.6"
                    />
                  </g>
                </g>
              )
            } else {
              Svgr::default()
            }}
          </svg>
        )
    }
}
