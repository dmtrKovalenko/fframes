//! A small captions example: one media track, four presets and a typed word highlight.
use fframes::captions::{CaptionMode, CaptionRect, Captions};
use fframes::{AudioMap, Duration, FFramesContext, Frame, Svgr, Video, include_media_dir, svgr};

include_media_dir!(pub struct CaptionMedia, "examples/captions/media");

#[derive(serde::Deserialize)]
#[serde(tag = "effect", rename_all = "kebab-case")]
enum Highlight {
    Underline { color: String },
    Circle { color: String },
}

pub struct CaptionVideo {
    modes: [Captions; 4],
}

impl Default for CaptionVideo {
    fn default() -> Self {
        Self {
            modes: [
                (CaptionMode::Highlight, 90.0, 230.0),
                (CaptionMode::Karaoke, 1010.0, 230.0),
                (CaptionMode::Reveal, 90.0, 660.0),
                (CaptionMode::Word, 1010.0, 660.0),
            ]
            .map(|(mode, x, y)| {
                Captions::from_media("speech.json", CaptionRect::new(x, y, 820.0, 240.0))
                    .font("DM Sans", 58)
                    .font_weight(500)
                    .mode(mode)
                    .active_fill("#77e6cf")
                    .upcoming_fill("#64748b")
                    .outline("#0c1423", 2.0)
            }),
        }
    }
}

impl Video for CaptionVideo {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(8.0)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        // Native, embedded and browser loading use the same cached metadata.
        // Each type decodes only on first access; no registration or per-frame JSON parse.
        let highlight = self.modes[0].highlight_with(&mut frame, ctx, |word, text| {
            let decoration = match word.word.metadata.typed::<Highlight>() {
                Some(Highlight::Underline { color }) => svgr!(
                    <path d={format!("M 0 {} H {}", word.height() - 3.0, word.width())}
                        stroke={color.as_str()} stroke-width="4" stroke-linecap="round" />
                ),
                Some(Highlight::Circle { color }) => svgr!(
                    <ellipse cx={word.width() / 2.0} cy={word.height() / 2.0}
                        rx={word.width() / 2.0 + 10.0} ry={word.height() / 2.0 + 5.0}
                        fill="none" stroke={color.as_str()} stroke-width="3" />
                ),
                None => Svgr::empty(),
            };
            // The scope is borrowed and text is owned: neither needs cloning.
            svgr!(<g>{decoration}{text}</g>)
        });
        let karaoke = self.modes[1].render(&mut frame, ctx);
        let reveal = self.modes[2].render(&mut frame, ctx);
        let word = self.modes[3].render(&mut frame, ctx);
        svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080">
                <rect width="1920" height="1080" fill="#0c1423" />
                <path d="M 960 180 V 980 M 70 570 H 1850" stroke="#253248" stroke-width="2" />
                <g font-family="DM Sans" font-weight="500" fill="#f4f7fb">
                    <text x="90" y="110" font-size="54">"One transcript, four treatments"</text>
                    <g font-size="26" fill="#91a3b9">
                        <text x="90" y="210">"HIGHLIGHT + TYPED WORD EFFECTS"</text>
                        <text x="1010" y="210">"KARAOKE"</text>
                        <text x="90" y="640">"REVEAL"</text>
                        <text x="1010" y="640">"WORD"</text>
                    </g>
                </g>
                {highlight}{karaoke}{reveal}{word}
            </svg>
        )
    }
}
