use crate::ConferenceMedia;
use fframes::{
    self, EstimateTextWidthOptions, FFramesContext, FontQuery, FontStretch, FontStyle, Frame,
    Scene, Svgr, Transform,
    animation::{self, Easing},
};

#[derive(Debug, Clone)]
pub struct SpeakerScene<'a> {
    pub speaker_name: &'a str,
    pub media: &'a ConferenceMedia,
    pub talk_title: &'a str,
    pub talk_description: &'a str,
    pub avatar: Option<&'a str>,
}

impl Scene for SpeakerScene<'_> {
    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Seconds(4.)
    }

    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let tilt_angle = frame.animate(&fframes::timeline!(
          at 0.3, animate 0.4 => 1.2, animation::Easing::Linear,
          at 2.3 => 4.1, animate 1.2 => 0.8, animation::Easing::Linear
        ));

        const TITLE_Y: usize = 350;
        let title_opts = fframes::BreakLinesOpts {
            width: 970,
            line_height: 1.2,
            x: 90,
            y: TITLE_Y,
            font: FontQuery {
                size: if self.talk_title.len() < 50 || self.talk_description.len() <= 300 {
                    70
                } else {
                    56
                },
                family: "Inter 18pt",
                weight: 800,
                ..Default::default()
            },
            align: fframes::TextAlign::Left,
            fill: "white",
            ..Default::default()
        };

        let title_structure = frame
            .text_break_lines_structure(ctx, self.talk_title, title_opts)
            .expect("failed to wrap the title");

        let speaker_name_width = fframes::estimate_text_width(
            ctx,
            self.speaker_name,
            EstimateTextWidthOptions {
                font_family: "Inter 24pt",
                font_size: 54,
                font_weight: 700,
                font_style: FontStyle::Normal,
                font_stretch: FontStretch::Normal,
            },
        )
        .expect("Failed to calculate speaker name text width")
            + 32;

        let abstract_y = TITLE_Y + title_structure.occupied_height();
        let abstract_opts = fframes::BreakLinesOpts {
            width: 1000,
            line_height: 1.2,
            x: 90,
            y: abstract_y,
            font: fframes::FontQuery {
                size: if self.talk_description.len() > 600 {
                    32
                } else if self.talk_description.len() > 300 {
                    36
                } else {
                    48
                },
                weight: 400,
                family: "Inter 24pt",
                ..Default::default()
            },
            align: fframes::TextAlign::Left,
            fill: "white",
            ..Default::default()
        };

        let abstract_text_structure = frame
            .text_break_lines_structure(ctx, self.talk_description, abstract_opts)
            .expect("failed to wrap the abstract text");

        fframes::svgr!(
            <image
                width={1920}
                // height={1080}
                x="0"
                y="0"
                href={self.media.background_2025_png.href()}
            />

            {title_structure.as_svgr(title_opts)}

            <svg
                x="1000"
                y="250"
                width="140"
                height="134"
                viewBox="0 0 140 134"
                fill="none"
                xmlns="http://www.w3.org/2000/svg"
            >
              <path
                d="M-4.94719e-06 2H134.8C136.567 2 138 3.43269 138 5.2V134"
                stroke="#FFFBFB"
                stroke-width="3"
                stroke-dasharray="300"
                stroke-dashoffset={frame.animate(&fframes::timeline!(
                  at 0., animate 300. => 0., &Easing::Spring  {mass: 4., stiffness: 80., damping: 55.}
                ))}
              />
            </svg>

            <svg
                x="40"
                y={abstract_y + abstract_text_structure.occupied_height() - 110}
                width="140"
                height="134"
                viewBox="0 0 140 134"
                fill="none"
                xmlns="http://www.w3.org/2000/svg"
            >
              <path
                d="M137.2 132H5.19999C3.43268 132 2 130.567 2 128.8V0"
                stroke="#FFFBFB"
                stroke-width="3"
                stroke-dasharray="300"
                stroke-dashoffset={frame.animate(&fframes::timeline!(
                  at 0., animate 300. => 0., &Easing::Spring  {mass: 4., stiffness: 80., damping: 55.}
                ))}
              />
            </svg>

            {abstract_text_structure.as_svgr(abstract_opts).clone()}

            <pattern
                id="speaker"
                x="0%"
                y="0%"
                height="100%"
                width="100%"
                viewBox="0 0 480 480"
            >
               <image
                 x="0%"
                 y="0%"
                 width="480"
                 height="480"
                 href={
                    self
                      .avatar
                      .as_ref()
                      .and_then(|avatar| ctx.get_image(avatar))
                      .unwrap_or(&self.media.speaker_placeholder_png)
                      .href()
                 }
               />
            </pattern>
            <filter id="shadow">
              <feDropShadow
                dx="3"
                dy="3"
                stdDeviation="8"
                flood-color="#000"
                flood-opacity="0.5"
              />
            </filter>

            <g transform={Transform::skew(tilt_angle, -tilt_angle + 0.4)}>
              <rect
                filter="url(#shadow)"
                rx="60"
                ry="60"
                x="1200"
                y="162"
                width="661"
                height="770"
                fill="#525764"
              />
              <circle
                r="220"
                cx="1540"
                cy="470"
                fill="url(#speaker)"
              />
              <rect
                x={1534-speaker_name_width/2}
                y={820-95/2}
                width={speaker_name_width}
                height="90"
                rx="20"
                ry="20"
                fill="white"
              />
              <text
                dominant-baseline="middle"
                fill="#D54000"
                text-anchor="middle"
                x="1534"
                y="820"
                font-weight="700"
                font-family="Inter 24pt"
                font-size="54"
              >
                {self.speaker_name}
              </text>
            </g>
        )
    }
}
