use std::sync::Arc;

use fframes::{Scene, Transform, Video, animation::Easing, svgr};
use hello_world_example::HelloWorldVideo;
use marketing_example::MarketingVideo;
use podcast_example::PodcastVideo;
use tiktok_example::GooseVideo;

#[derive(Debug)]
pub struct BetaExamples<'a> {
    pub hello_world_video: Arc<HelloWorldVideo<'a>>,
    pub marketing_video: Arc<MarketingVideo<'a>>,
    pub podcast_video: Arc<PodcastVideo<'a>>,
    pub tiktok_video: Arc<GooseVideo<'a>>,
}

impl Scene for BetaExamples<'_> {
    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Frames(500)
    }

    fn render_frame<'a>(
        &'a self,
        frame: fframes::Frame,
        ctx: &fframes::FFramesContext<'a, '_>,
    ) -> fframes::Svgr<'a> {
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
             x={frame.animate(&fframes::timeline!(
               at 0., animate 200. => 960., Easing::Spring { mass: 1.0 , stiffness: 100., damping: 16. }
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
                transform={
                    frame.animate(
                        &fframes::timeline!(
                            at MARKETING_TS, animate Transform::translate(0, 0.) => Transform::translate(0, -180.),
                            Easing::Spring { mass: 1.0, stiffness: 140., damping: 16. },

                            at PODCAST_TS, animate Transform::translate(0, -180.) => Transform::translate(0, -380.),
                            Easing::Spring { mass: 1.0, stiffness: 140., damping: 16. },

                            at TIKTOK_TS, animate Transform::translate(0, -380.) => Transform::translate(0, -580.),
                            Easing::Spring { mass: 1.0, stiffness: 140., damping: 16. }
                        )
                    )
                }
             >
               <text
                 x={frame.animate(&fframes::timeline!(
                   at 0., animate 1700. => 960., Easing::Spring{ mass: 1.0 , stiffness: 100., damping: 16. }
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
            <g transform={
                Transform {
                    translate_x: if frame.seconds() >= TIKTOK_TS { VIDEO_OFFSET_X + 1188. } else { VIDEO_OFFSET_X } as f64,
                    translate_y: VIDEO_OFFSET_Y.into(),
                    rotate: (if frame.seconds() >= TIKTOK_TS { 90. } else { 0. }).into(),
                    scale: VIDEO_SCALE.into(),
                    skew_x: 0.,
                    skew_y: 0.,
                }
            }>
              {
                match frame.seconds() {
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
