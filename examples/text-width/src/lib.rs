//! Boxes sized by `text_width` and a paragraph wrapped by `text_break_lines`: each box should
//! hug its glyphs, and no wrapped line should cross the paragraph's right edge.
use fframes::{
    AudioMap, BreakLinesOpts, Duration, FFramesContext, FontQuery, Frame, Svgr, TextAlign, Video,
    include_media_dir, svgr,
};

include_media_dir!(pub struct TextWidthMedia, "examples/arm/media");

const FAMILY: &str = "Inter 18pt";
const PARAGRAPH_WIDTH: usize = 760;

pub struct TextWidthVideo;

fn font(size: usize) -> FontQuery<'static> {
    FontQuery {
        family: FAMILY,
        size,
        weight: 800,
        ..Default::default()
    }
}

fn measured<'a>(
    frame: &mut Frame,
    ctx: &FFramesContext<'a, '_>,
    text: &'a str,
    size: usize,
    y: f64,
) -> Svgr<'a> {
    let width = frame.text_width(ctx, font(size), text).unwrap_or(0);
    svgr!(<g>
        <rect x="120" y={y - size as f64 * 0.8} width={width} height={size as f64} fill="#1f3a5f" />
        <text x="120" y={y} font-family={FAMILY} font-weight="800" font-size={size} fill="#f4f7fb">{text}</text>
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
        let kerned = measured(&mut frame, ctx, "AVAVAVAV WAVE", 100, 260.0);
        let greek = measured(
            &mut frame,
            ctx,
            "δεν είναι ανατολική ρωμαϊκή αυτοκρατορία",
            46,
            420.0,
        );
        let latin = measured(
            &mut frame,
            ctx,
            "The Byzantine Greeks call Eastern Rome",
            46,
            520.0,
        );
        let paragraph = frame.text_break_lines(
            ctx,
            "Η Ανατολική Ρωμαϊκή Αυτοκρατορία ονομάστηκε αργότερα Βυζάντιο. Later historians named the Eastern Roman Empire Byzantium.",
            BreakLinesOpts {
                font: font(40),
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
                <text x="120" y="120" font-family={FAMILY} font-weight="800" font-size="40" fill="#91a3b9">"text_width boxes and a wrapped paragraph"</text>
                {kerned}{greek}{latin}
                <path d={format!("M {} 600 V 1000", 120 + PARAGRAPH_WIDTH)} stroke="#e76f51" stroke-width="2" />
                {paragraph}
            </svg>
        )
    }
}
