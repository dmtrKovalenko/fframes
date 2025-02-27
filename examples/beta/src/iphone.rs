use fframes::{SampleSize, Scene, Svgr, VisualizeFrameInput, animation, media::ImageData, svgr};

#[derive(Debug)]
pub struct IphoneScene {
    pub hours: u32,
    pub minutes: u32,
}

fn render_discord_message<'a>(
    x: usize,
    y: usize,
    name: &'a str,
    content: &'a str,
    image: Option<&ImageData>,
) -> Svgr<'a> {
    match image {
        None => Svgr::default(),
        Some(image) => {
            svgr!(
              <g>
                <image x={x} y={y} width="80" height="80" href={image.href()} />

                <text font-weight="500" x={x + 80} y={y + 34} fill="white" font-size="18">
                  {name}
                </text>

                <text font-weight="500" x={x + 80} y={y + 60} fill="#cbd5e1" font-size="15">
                  {content}
                </text>
              </g>
            )
        }
    }
}

impl Scene for IphoneScene {
    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Seconds(8.)
    }

    fn render_frame(&self, frame: fframes::Frame, ctx: &fframes::FFramesContext) -> fframes::Svgr {
        const SEND_MESSAGE_TS: f32 = 4.8;
        const EXPAND_ISLAND_TS: f32 = 6.2;

        let dynamic_island_width = frame.animate(&fframes::timeline!(
            on 0.0, val 120. => 180., &animation::Easing::Spring{ mass: 1.6 , stiffness: 400., damping: 26. },
            on EXPAND_ISLAND_TS, val 180. => 348., &animation::Easing::Spring{ mass: 1.6 , stiffness: 300., damping: 26. }
        ));

        let dynamic_island_height = frame.animate(&fframes::timeline!(
           on EXPAND_ISLAND_TS, val 40. => 80., &animation::Easing::Spring{ mass: 1.6 , stiffness: 300., damping: 26. }
        ));

        let audio_visualization = frame.visualize_audio_frame(VisualizeFrameInput {
            smooth_level: 3,
            audio: ctx.get_audio("beta.mp3").expect("missing beta.mp3"),
            sample_size: SampleSize::S16,
            window: None,
        });

        svgr!(
           <clipPath id="iphoneUi">
             <rect width="380" height="820" rx="60" x="780" y="150"/>
           </clipPath>

            <g clip-path="url(#iphoneUi)">
              <rect
                 width="370"
                 height="819"
                 rx="60"
                 x="790"
                 y="154"
                 fill="url(#pattern1)"
             />
              <image
                 href={ctx.get_image("camera_ui.png").unwrap().href()}
                 width="370"
                 height="819"
                 rx="60"
                 x="790"
                 y="154"
              />

              <image
                href={ctx.get_image("qr.png").unwrap().href()}
                width="300"
                height="300"
                x="830"
                y={1080 / 2 - 150}
              />

              <g fill="none" opacity={
                frame.animate(fframes::timeline!(
                  on 1.1, val 0. => 1., &animation::Easing::Linear(0.3)
                ))
              }>
                <path d="M832 439V415C832 402.297 842.297 392 855 392H882" stroke="#EBBD1D" stroke-width="2"/>
                <path d="M1086 392H1110C1122.7 392 1133 402.297 1133 415V442" stroke="#EBBD1D" stroke-width="2"/>
                <path d="M1133 650V674C1133 686.703 1122.7 697 1110 697H1083" stroke="#EBBD1D" stroke-width="2"/>
                <path d="M879 697H855C842.297 697 832 686.703 832 674V647" stroke="#EBBD1D" stroke-width="2"/>

                <rect x="884" y="715" fill="#EBBD1D" width="200" height="34" rx="16"
                  transform-origin="center center"
                  transform={format!("scale({scale}, {scale})",
                    scale=frame.animate(&fframes::timeline!(
                      on 2.5, val 1. => 1.04, &animation::Easing::Linear(0.2)
                    ))
                  )}
                />
                <text font-size="16" font-weight="500" x="905" y="737" fill="#000"
                  transform-origin="center center"
                  transform={format!("scale({scale}, {scale})",
                    scale=frame.animate(&fframes::timeline!(
                      on 2.5, val 1. => 1.04, &animation::Easing::Linear(0.2)
                    ))
                  )}
                >
                  "fframes discord invite"
                </text>
              </g>
            </g>

            <g clip-path="url(#iphoneUi)">
              <g
                transform={format!("translate(0 {})", frame.animate(&fframes::timeline!(
                    on 3.0, val 1800. => 0., &animation::Easing::Spring { mass: 0.4 , stiffness: 70., damping: 16. }
                  )))
                }
              >
                <rect x="790" y="154" width="370" height="819" fill="#292841" />
                <image
                   href={ctx.get_image("discord_ui.png").expect("Expects are not fine").href()}
                   width="370"
                   height="819"
                   rx="60"
                   x="791"
                   y="152"
                />

                <text font-weight="500" x="830" y="196" fill="#9ca3af" font-size="16">
                  {format!("{:02}:{:02}", self.hours, self.minutes)}
                </text>

                {render_discord_message(800, 370, "John Doe", "Hey, How I can export the .webm video?", ctx.get_image("john.png"))}
                {render_discord_message(800, 450, "Dmitriy Kovalenko", "Just change out file extension to .webm", ctx.get_image("dmitriy.png"))}
                {render_discord_message(800, 530, "Linus Torvalds", "https://github.com/torvalds", ctx.get_image("torvalds.png"))}

                <g
                  transform-origin="bottom center"
                  transform={format!("translate(0 {y}) scale({scale})", y=frame.animate(&fframes::timeline!(
                    on SEND_MESSAGE_TS, val 200. => 0., &animation::Easing::Spring{ mass: 1.0 , stiffness: 240., damping: 26. }
                  )), scale=frame.animate(&fframes::timeline!(
                    on SEND_MESSAGE_TS, val 0.4 => 1., &animation::Easing::Spring{ mass: 1.0 , stiffness: 220., damping: 26. }
                  )))}
                  opacity={
                    frame.animate(
                      fframes::timeline!(
                        on SEND_MESSAGE_TS, val 0. => 1., &animation::Easing::Linear(0.2)
                      )
                    )
                  }
                >
                  {render_discord_message(800, 620, "Me", "https://github.com/theawesome", ctx.get_image("me.png"))}
                </g>
              </g>
            </g>

            <image href={ctx.get_image("iphone_frame.png").expect("Do not use expects in real code!").href()} x="30%" y="10%" width="800" />

            <rect
              x="916"
              y="170"
              width={dynamic_island_width}
              transform={format!("translate(-{}, 0)",( dynamic_island_width - 120. )/ 2.)}
              height={dynamic_island_height}
              ry={dynamic_island_height / 2.}
              fill="#000"
            />

            {if frame.index > 8 && frame.get_current_second() < EXPAND_ISLAND_TS {
              svgr!(
                <text font-family="Bubble Bobble" fill="#6366f1" font-size="23" x="895" y="197">
                 "ff"
                </text>
                {
                  audio_visualization
                  .iter()
                  .enumerate()
                  .skip(1)
                  .take(6)
                  .map(|(i, freq)| {
                    let db = (10.0 * libm::log10f(*freq * 500.)).clamp(4., 18.);
                    svgr!(
                      <rect
                        x={1025. + i as f64 * 3.5 + i as f64}
                        y={190. - db / 2.}
                        width="3"
                        height={db}
                        ry="1"
                        fill="#6366f1"
                      />
                    )
                  })
                  .collect::<Vec<_>>()
                }
              )
            } else if frame.get_current_second() > EXPAND_ISLAND_TS + 0.2 {
              svgr!(
                <svg
                  x="816" y="182"
                  height="56" width="56"
                  xmlns="http://www.w3.org/2000/svg"
                  viewBox="0 0 24 24"
                >
                  <g fill="#fff">
                    <path d="M12 2C6.477 2 2 6.477 2 12c0 4.419 2.865 8.166 6.839 9.489.5.09.682-.218.682-.484 0-.236-.009-.866-.014-1.699-2.782.602-3.369-1.34-3.369-1.34-.455-1.157-1.11-1.465-1.11-1.465-.909-.62.069-.608.069-.608 1.004.071 1.532 1.03 1.532 1.03.891 1.529 2.341 1.089 2.91.833.091-.647.349-1.086.635-1.337-2.22-.251-4.555-1.111-4.555-4.943 0-1.091.39-1.984 1.03-2.682-.103-.254-.447-1.27.097-2.646 0 0 .84-.269 2.75 1.025A9.548 9.548 0 0112 6.836c.85.004 1.705.114 2.504.336 1.909-1.294 2.748-1.025 2.748-1.025.546 1.376.202 2.394.1 2.646.64.699 1.026 1.591 1.026 2.682 0 3.841-2.337 4.687-4.565 4.935.359.307.679.917.679 1.852 0 1.335-.012 2.415-.012 2.741 0 .269.18.579.688.481A9.997 9.997 0 0022 12c0-5.523-4.477-10-10-10z"/>
                  </g>
                </svg>

                <text font-weight="500" font-size="18" x="880" y="208" fill="#fff">
                  "New collaboration invite"
                </text>

                <text font-weight="500" font-size="14" x="880" y="230" fill="#cbd5e1">
                  "dmtrKovalenko invites you to fframes"
                </text>
              )
            } else {
              Svgr::default()
            }
          }
        )
    }
}
