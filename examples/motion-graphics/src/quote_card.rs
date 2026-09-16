use std::sync::OnceLock;

use fframes::{
    AudioMap, Color, FFramesContext, FontQuery, Frame, Svgr, Transform, animation::Easing,
};

use crate::{MotionGraphicsMedia, SPRING_SNAPPY};

const MAX_FONT_SIZE: usize = 500;
const MIN_FONT_SIZE: usize = 60;
const HORIZONTAL_PADDING: usize = 200;
const TARGET_WIDTH: usize = 1920 - HORIZONTAL_PADDING * 2;
const LINE_HEIGHT_RATIO: f32 = 0.9;

/// A single giant word/phrase that fills the entire frame.
/// Supports multiple lines separated by `\n`.
/// Meant to render behind a speaker as a background emphasis card.
pub struct QuoteCardVideo<'a> {
    pub media: &'a MotionGraphicsMedia,
    pub text: &'a str,
    resolved_font_sizes: OnceLock<Vec<usize>>,
}

impl std::fmt::Debug for QuoteCardVideo<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuoteCardVideo")
            .field("text", &self.text)
            .finish()
    }
}

impl<'a> QuoteCardVideo<'a> {
    pub fn new(media: &'a MotionGraphicsMedia, text: &'a str) -> Self {
        Self {
            media,
            text,
            resolved_font_sizes: OnceLock::new(),
        }
    }
}

/// Largest font size (down to [`MIN_FONT_SIZE`]) at which `line` fits into
/// [`TARGET_WIDTH`].  `None` when the font can not be measured yet (fonts are
/// resolved lazily in the editor), so the caller must not cache the result.
fn resolve_font_size<'a>(
    frame: &mut Frame,
    ctx: &FFramesContext<'a, '_>,
    line: &'a str,
) -> Option<usize> {
    let mut size = MAX_FONT_SIZE;
    loop {
        let query = FontQuery {
            family: "Bebas Neue",
            size,
            weight: 400,
            ..Default::default()
        };
        match frame.text_width(ctx, query, line)? {
            w if w <= TARGET_WIDTH => break Some(size),
            _ if size <= MIN_FONT_SIZE => break Some(MIN_FONT_SIZE),
            _ => size -= 10,
        }
    }
}

impl fframes::Video for QuoteCardVideo<'_> {
    const FPS: usize = 60;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Seconds(3.0)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let lines: Vec<&str> = self.text.lines().collect();
        // Measuring text is only cached once the fonts could actually be
        // measured; until then every line falls back to the minimum size.
        let font_sizes = match self.resolved_font_sizes.get() {
            Some(sizes) => sizes.clone(),
            None => match lines
                .iter()
                .map(|line| resolve_font_size(&mut frame, ctx, line))
                .collect::<Option<Vec<usize>>>()
            {
                Some(sizes) => self.resolved_font_sizes.get_or_init(|| sizes).clone(),
                None => vec![MIN_FONT_SIZE; lines.len()],
            },
        };

        // Calculate total block height to center vertically
        let total_height: f32 = font_sizes
            .iter()
            .map(|s| *s as f32 * LINE_HEIGHT_RATIO)
            .sum();
        let start_y = 540.0 - total_height / 2.0;

        // Scale punch in (start big, spring down to 1.0)
        let scale = frame.animate(&fframes::timeline!(
            at 0.0 => 0.8, animate 1.6_f32 => 1.0, SPRING_SNAPPY,
        ));

        // Slide up from below
        let slide_y = frame.animate(&fframes::timeline!(
            at 0.0 => 0.6, animate 80.0_f32 => 0.0, SPRING_SNAPPY,
        ));

        // Fade in
        let opacity = frame.animate(&fframes::timeline!(
            at 0.0 => 0.3, animate 0.0_f32 => 1.0, Easing::EaseOut,
        ));

        // Fade out
        let fade_out = frame.animate(&fframes::timeline!(
            at 2.2 => 2.8, animate 1.0_f32 => 0.0, Easing::EaseIn,
        ));

        let mut y_offset = start_y;
        let text_elements: Vec<_> = lines
            .iter()
            .zip(font_sizes.iter())
            .map(|(line, &size)| {
                let line_y = y_offset + size as f32 * LINE_HEIGHT_RATIO;
                y_offset = line_y;
                let y = line_y;

                fframes::svgr!(
                    <text
                        x="960"
                        y={y}
                        font-family="Bebas Neue"
                        font-size={size}
                        fill="white"
                        text-anchor="middle"
                        letter-spacing="20"
                    >
                        {*line}
                    </text>
                )
            })
            .collect();

        fframes::svgr!(
            <svg
                xmlns="http://www.w3.org/2000/svg"
                viewBox="0 0 1920 1080"
                width={Self::WIDTH}
                height={Self::HEIGHT}
            >
                <rect width={Self::WIDTH} height={Self::HEIGHT} fill="black" />

                <g
                    opacity={opacity * fade_out}
                    transform={Transform {
                        translate_x: 960.0 * (1.0 - scale as f64),
                        translate_y: 540.0 * (1.0 - scale as f64) + slide_y as f64,
                        scale: fframes::Scale { x: scale as f64, y: scale as f64 },
                        ..Default::default()
                    }}
                >
                    {text_elements}
                </g>
            </svg>
        )
    }
}
