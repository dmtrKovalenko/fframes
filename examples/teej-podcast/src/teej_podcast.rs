use crate::Chapter;
use crate::{constants::*, create_transition_keyframes_for_chapters};
use fframes::animation::KeyFramesAnimation;
use fframes::{
    AudioMap, FFramesContext, FFramesSyncedVideoFrame, FontQuery, Frame, TextOverflow, Transform,
    Video,
};
use std::borrow::Cow;
use std::sync::OnceLock;

#[derive(Debug)]
pub struct TeejPodcast<'a> {
    chapters: &'a [Chapter<'a>],
    chapters_animation: KeyFramesAnimation<f32>,
    /// Chapter titles shortened to their rectangle, see [`Self::fit_titles`].
    fitted_titles: OnceLock<Vec<String>>,
}

impl TeejPodcast<'_> {
    pub fn new<'a>(chapters: &'a [Chapter<'a>]) -> TeejPodcast<'a> {
        let chapters_animation = create_transition_keyframes_for_chapters(chapters);
        TeejPodcast {
            chapters,
            chapters_animation,
            fitted_titles: OnceLock::new(),
        }
    }

    /// Every chapter title cut with an ellipsis to the width left after the
    /// "mm:ss" prefix, or `None` while the font can not be measured.
    fn fit_titles<'a>(
        &'a self,
        frame: &mut Frame,
        ctx: &FFramesContext<'a, '_>,
    ) -> Option<Vec<String>> {
        let font = FontQuery {
            family: CHAPTER_FONT_FAMILY,
            size: CHAPTER_FONT_SIZE,
            weight: CHAPTER_FONT_WEIGHT,
            ..Default::default()
        };
        let separator_width = frame.text_width(ctx, font, "  ")?;
        let max_width = (CHAPTER_WIDTH - CHAPTER_TEXT_PADDING * 2) as usize;

        self.chapters
            .iter()
            .map(|chapter| {
                let prefix_width = frame.text_width(ctx, font, chapter.start)? + separator_width;
                frame
                    .text_fit(
                        ctx,
                        font,
                        chapter.title,
                        max_width.saturating_sub(prefix_width),
                        TextOverflow::Ellipsis,
                    )
                    .map(Cow::into_owned)
            })
            .collect()
    }
}

impl Video for TeejPodcast<'_> {
    const FPS: usize = 24;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Auto
    }

    fn audio(&self) -> AudioMap<'_> {
        use fframes::AudioTimestamp::*;

        AudioMap::from([
            ("left.mp4", Second(0.)..Eof),
            ("right.mp4", Second(0.)..Eof),
        ])
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> fframes::Svgr<'a> {
        let mut frame = frame;
        let left_frame = frame
            .get_synced_video_frame(
                ctx,
                "left.mp4",
                &fframes::SyncVideoFrameInput {
                    start_from: 0.,
                    looping: false,
                    editor_fallback_image: ctx.get_image("left.jpg"),
                },
            )
            .map(|fr| fr.into_image());

        let right_frame = frame
            .get_synced_video_frame(
                ctx,
                "right.mp4",
                &fframes::SyncVideoFrameInput {
                    start_from: 0.,
                    looping: false,
                    editor_fallback_image: ctx.get_image("right.jpg"),
                },
            )
            .map(|fr| fr.into_image());

        // The chapter whose time range contains the current frame; the last
        // chapter stays active until the end.
        let t = frame.seconds() as u64;
        let active_index = self
            .chapters
            .windows(2)
            .position(|w| t >= w[0].start_seconds && t < w[1].start_seconds)
            .unwrap_or(self.chapters.len().saturating_sub(1));

        // Chapter titles are cut with an ellipsis so they stay inside their
        // rectangle after the "mm:ss" prefix. They never change, so they are
        // measured once; until the fonts can be measured (the editor loads
        // them lazily) the full titles are shown.
        let titles = match self.fitted_titles.get() {
            Some(titles) => Cow::Borrowed(titles),
            None => match self.fit_titles(&mut frame, ctx) {
                Some(titles) => Cow::Borrowed(self.fitted_titles.get_or_init(|| titles)),
                None => Cow::Owned(
                    self.chapters
                        .iter()
                        .map(|chapter| chapter.title.to_owned())
                        .collect(),
                ),
            },
        };

        fframes::svgr!(
            <svg
                xmlns="http://www.w3.org/2000/svg"
                width={Self::WIDTH}
                height={Self::HEIGHT}
            >
                <rect x="0" y="0" width="1920" height="1080" fill="#1A191B" />
                <defs>
                    <clipPath id="clip-left-frame">
                        <rect id="left-video-frame"
                            x={FRAME_PADDING}
                            y={FRAME_PADDING}
                            width={FRAME_WIDTH}
                            height={FRAME_HEIGHT}
                        />
                    </clipPath>
                    <clipPath id="clip-right-frame">
                        <rect
                            id="right-video-frame"
                            x={FRAME_WIDTH + FRAME_PADDING * 2}
                            y={FRAME_PADDING}
                            width={FRAME_WIDTH}
                            height={FRAME_HEIGHT}
                        />
                    </clipPath>
                </defs>

                // Reuse the same element used for clipping the image to add a border
                <use href="#left-video-frame" fill="none" stroke="#000" stroke-width="10" />
                <image
                    id="avatar"
                    x="-600"
                    y="0"
                    width="1920"
                    stroke-width="6"
                    stroke="#fff"
                    clip-path="url(#clip-left-frame)"
                    preserveAspectRatio="xMidYMid meet"
                    href={
                        match left_frame {
                            Some(ref frame) => frame,
                            _ => ctx.get_image("left.jpg").expect("left.jpg not found")
                        }.href()
                    }
                />

                <use href="#right-video-frame" fill="none" stroke="#000" stroke-width="10" />
                <image
                    id="avatar"
                    x="8"
                    y="0"
                    width="1920"
                    clip-path="url(#clip-right-frame)"
                    preserveAspectRatio="xMidYMid meet"
                    href={
                        match right_frame {
                            Some(ref frame) => frame,
                            _ => ctx.get_image("right.jpg").expect("left.jpg not found")
                        }.href()
                    }
                />

                <text
                    x="1400"
                    y="40"
                    alignment-baseline="text-before-edge"
                    font-size="75"
                    fill="#fff"
                    font-family="JetBrains Mono"
                >
                    "2D: Rust"
                </text>

                <g>
                    // The chapters rendered as rectangles
                    {self.chapters.iter().enumerate().map(|(i, _)| fframes::svgr!(
                        <rect
                            x={CHAPTERS_START_X}
                            y={Chapter::get_y_position(i)}
                            width={CHAPTER_WIDTH}
                            height={CHAPTER_HEIGHT}
                            fill="#404040"
                            rx="5"
                            ry="5"
                        />
                    )).collect::<Vec<_>>()}

                    // Highlighter renders on top of the rectangles but under the text
                    <rect
                        x={CHAPTERS_START_X}
                        y={CHAPTERS_START_Y}
                        width={CHAPTER_WIDTH}
                        height={CHAPTER_HEIGHT}
                        fill="#ff6900"
                        rx="5"
                        ry="5"
                        transform={Transform::translate(0, frame.animate(&self.chapters_animation))}
                    />

                    // The text is rendered on top of either highlighter or rectangle
                    {self.chapters.iter().zip(titles.iter()).enumerate().map(|(i, (chapter, title))| {
                        let text_color = if i == active_index { "#000" } else { "#fff" };
                        fframes::svgr!(
                        <text
                            x={CHAPTERS_START_X + CHAPTER_TEXT_PADDING}
                            y={Chapter::get_y_position(i) + CHAPTER_HEIGHT / 2 + 5}
                            dominant-baseline="middle"
                            fill={text_color}
                            font-family={CHAPTER_FONT_FAMILY}
                            font-weight={CHAPTER_FONT_WEIGHT}
                            font-size={CHAPTER_FONT_SIZE}
                        >
                            <tspan>{chapter.start}</tspan>
                            <tspan>{"  "}</tspan>
                            <tspan>{title.clone()}</tspan>
                        </text>
                    )}).collect::<Vec<_>>()}
                </g>
            </svg>
        )
    }
}
