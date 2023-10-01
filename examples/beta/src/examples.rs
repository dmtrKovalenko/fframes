use fframes::{animation, svgr, Scene, Video};
use hello_world_example::HelloWorldVideo;
use marketing_example::MarketingVideo;
use podcast_example::PodcastVideo;
use tiktok_example::GooseVideo;

#[derive(Debug)]
pub struct ExamplesScene<'a> {
    pub hello_world_video: HelloWorldVideo<'a>,
    pub marketing_video: MarketingVideo<'a>,
    pub podcast_video: PodcastVideo<'a>,
    pub tiktok_video: GooseVideo<'a>,
}

impl Scene for ExamplesScene<'_> {
    fn duration(&self) -> fframes::Duration {
        fframes::Duration::Frames(500)
    }

    fn render_frame(&self, frame: fframes::Frame, ctx: &fframes::FFramesContext) -> fframes::Svgr {
        const VIDEO_SCALE: f32 = 0.62;
        const VIDEO_OFFSET_Y: i32 = 320;
        const VIDEO_OFFSET_X: f32 = (1920. - (1920. * VIDEO_SCALE)) / 2.;
        const VIDEO_CORNDER_RADIUS: i32 = 50;

        const MARKETING_TS: f32 = 2.6;
        const PODCAST_TS: f32 = 4.8;
        const TIKTOK_TS: f32 = 6.8;

        svgr!(
          <defs>
            <linearGradient id="text">
            <stop stop-color="#4338ca"/>
            <stop offset="1" stop-color="#a21caf"/>
            </linearGradient>

            <clipPath id="text-clip">
              <rect fill="red" width="100%" height="135" y="115" />
            </clipPath>

            <filter id="shadow" width="200%" height="200%" color-interpolation-filters="sRGB">
            <feDropShadow stdDeviation="40" flood-opacity="0.3"/>
             </filter>

            <clipPath id="preview-clip">
              <rect
                x={VIDEO_OFFSET_X} y={VIDEO_OFFSET_Y}
                height={1080. * VIDEO_SCALE} width={1920. * VIDEO_SCALE}
                rx={VIDEO_CORNDER_RADIUS} ry={VIDEO_CORNDER_RADIUS}
              />
            </clipPath>
          </defs>

          <text
            font-family="DM Sans"
            font-weight="700"
            text-anchor="middle"
            x={frame.animate(fframes::timeline!(
              on 0., val 200. => 960., &animation::Easing::Spring2(1.0 , 100., 16.)
            ))}
            y="8%"
            font-size="60"
            fill="url(#text)"
          >
           "to help you get started"
          </text>

          <g clip-path="url(#text-clip)">
          <g
            font-family="DM Sans"
            font-weight="700"
            fill="#000"
            text-anchor="middle"
            font-size="120"
            transform={format!("translate(0, {})",
              frame.animate(
                &fframes::timeline!(
                  on MARKETING_TS, val 0. => -180., &animation::Easing::Spring2(1.0 , 140., 16.),
                  on PODCAST_TS, val -200. => -380., &animation::Easing::Spring2(1.0 , 140., 16.),
                  on TIKTOK_TS, val -400. => -580., &animation::Easing::Spring2(1.0 , 140., 16.)
                )
              )
            )}
          >
            <text
              x={frame.animate(fframes::timeline!(
                on 0., val 1700. => 960., &animation::Easing::Spring2(1.0 , 100., 16.)
              ))}
              y="220"
            >
             "\"Hello world\" video"
             </text>
             <text
              x="960"
              y="394"
              >
              "Marketing Pitch Video"
              </text>
             <text
              x="960"
              y="594"
              >
              "Podcast Visualization"
              </text>
             <text
              x="960"
              y="794"
              >
              "TikTok-like video"
              </text>
          </g>
          </g>

          <rect
              filter="url(#shadow)"
              width={1920. * VIDEO_SCALE} height={1080. * VIDEO_SCALE}
              x={VIDEO_OFFSET_X} y={VIDEO_OFFSET_Y}
              rx={VIDEO_CORNDER_RADIUS}
              ry={VIDEO_CORNDER_RADIUS}
              stroke="#4338ca"
              stroke-width="8"
          />

          <g clip-path="url(#preview-clip)">
            <g transform={format!("translate({} {VIDEO_OFFSET_Y}) scale({VIDEO_SCALE} {VIDEO_SCALE}) rotate({})",
              if frame.get_current_second() >= TIKTOK_TS { VIDEO_OFFSET_X + 1188. } else { VIDEO_OFFSET_X },
              if frame.get_current_second() >= TIKTOK_TS { 90 } else { 0 }
            )}>
              {
                match frame.get_current_second() {
                  second if second < MARKETING_TS => self.hello_world_video.render_frame(frame, ctx),
                  second if second < PODCAST_TS => self.marketing_video.render_frame(frame, ctx),
                  second if second < TIKTOK_TS => self.podcast_video.render_frame(frame.into_global(), ctx),
                  _ => self.tiktok_video.render_frame(frame.into_global(), ctx),
                }
              }
            </g>
          </g>
        )
    }
}
