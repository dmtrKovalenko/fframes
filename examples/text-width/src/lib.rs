//! Boxes sized by `text_width`, a box filled by `text_fit` and a paragraph wrapped by
//! `text_break_lines`: each box should hug its glyphs, and no wrapped line should cross the
//! paragraph's right edge. The right column covers a variable font at two weights and Arabic
//! laid out with a fallback font inside Latin text.
use fframes::{
    AudioMap, BreakLinesOpts, Duration, FFramesContext, FontQuery, Frame, Svgr, TextAlign,
    TextOverflow, Video, include_media_dir, svgr,
};

include_media_dir!(pub struct InterMedia, "examples/arm/media");
mod own {
    // Sofia Sans Semi Condensed (variable weight) and Noto Sans Arabic cut down to "سلام".
    fframes::include_media_dir!(pub struct TextWidthMedia, "examples/text-width/media");
}
pub use own::TextWidthMedia;

const INTER: &str = "Inter 18pt";
const SOFIA: &str = "Sofia Sans Semi Condensed";
const PARAGRAPH_WIDTH: usize = 760;
const FIT_WIDTH: usize = 550;

pub struct TextWidthVideo;

fn font(family: &str, size: usize, weight: u16) -> FontQuery<'_> {
    FontQuery {
        family,
        size,
        weight,
        ..Default::default()
    }
}

fn measured<'a>(
    frame: &mut Frame,
    ctx: &FFramesContext<'a, '_>,
    font: FontQuery<'a>,
    text: &'a str,
    (x, y): (f64, f64),
) -> Svgr<'a> {
    let width = frame.text_width(ctx, font, text).unwrap_or(0);
    let size = font.size as f64;
    svgr!(<g>
        <rect x={x} y={y - size * 0.8} width={width} height={size} fill="#1f3a5f" />
        <text x={x} y={y} font-family={font.family} font-weight={font.weight} font-size={size} fill="#f4f7fb">{text}</text>
    </g>)
}

impl Video for TextWidthVideo {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(1.0)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let rows = [
            (font(INTER, 100, 800), "AVAVAVAV WAVE", (120.0, 260.0)),
            (
                font(INTER, 46, 800),
                "δεν είναι ανατολική ρωμαϊκή αυτοκρατορία",
                (120.0, 420.0),
            ),
            (
                font(INTER, 46, 800),
                "The Byzantine Greeks call Eastern Rome",
                (120.0, 520.0),
            ),
            (font(SOFIA, 64, 100), "Hamburgefonts WAVE", (1040.0, 660.0)),
            (font(SOFIA, 64, 900), "Hamburgefonts WAVE", (1040.0, 760.0)),
            (font(INTER, 64, 800), "AV سلام WAVE", (1040.0, 860.0)),
        ]
        .into_iter()
        .map(|(font, text, at)| measured(&mut frame, ctx, font, text, at))
        .collect::<Vec<_>>();
        let fitted = frame
            .text_fit(
                ctx,
                font(INTER, 64, 800),
                "Eastern Rome, Byzantium",
                FIT_WIDTH,
                TextOverflow::Ellipsis,
            )
            .unwrap_or_default()
            .into_owned();
        let paragraph = frame.text_break_lines(
            ctx,
            "Η Ανατολική Ρωμαϊκή Αυτοκρατορία ονομάστηκε αργότερα Βυζάντιο. Later historians named the Eastern Roman Empire Byzantium.",
            BreakLinesOpts {
                font: font(INTER, 40, 800),
                width: PARAGRAPH_WIDTH,
                line_height: 1.25,
                x: 120,
                y: 660,
                align: TextAlign::Left,
                fill: "#f4f7fb",
                ..Default::default()
            },
        );
        svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080">
                <rect width="1920" height="1080" fill="#0c1423" />
                <text x="120" y="120" font-family={INTER} font-weight="800" font-size="40" fill="#91a3b9">"text_width, text_fit and text_break_lines"</text>
                {rows}
                <rect x="1040" y="209" width={FIT_WIDTH} height="64" fill="none" stroke="#e76f51" stroke-width="2" />
                <text x="1040" y="260" font-family={INTER} font-weight="800" font-size="64" fill="#f4f7fb">{fitted}</text>
                <path d={format!("M {} 600 V 1000", 120 + PARAGRAPH_WIDTH)} stroke="#e76f51" stroke-width="2" />
                {paragraph}
            </svg>
        )
    }
}
