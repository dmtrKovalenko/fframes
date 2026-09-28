//! Workload "podcast" (fframes side). Twin of ../remotion/src/Podcast.tsx: two real video
//! feeds (`get_synced_video_frame`) with rounded corners (`clipPath`) and drop shadows
//! (`feDropShadow`), blurred moving gradient blobs (`feGaussianBlur`), word-by-word captions
//! laid out with `text_width`, a progress bar and a spring-animated title card.
//!
//! ```sh
//! target/release/podcast metal out/podcast.mp4 medium        # 1920x1080, 30 fps, 20 s
//! PODCAST_SECONDS=5 target/release/podcast metal out/podcast4k.mp4 medium 2   # 3840x2160, 5 s
//! ```

#[path = "../shared.rs"]
mod shared;

use fframes::animation::{AnimationRuntime, Easing};
use fframes::{
    AnimateRuntimeInput, AudioMap, Color, Duration, FFramesContext, FFramesSyncedVideoFrame, FontQuery,
    Frame, Svgr, SyncVideoFrameInput, Video,
};
use shared::*;

struct Blob {
    color: &'static str,
    r: f64,
    x: f64,
    y: f64,
}
const BLOBS: [Blob; 4] = [
    Blob { color: "#7c3aed", r: 360.0, x: -520.0, y: -220.0 },
    Blob { color: "#db2777", r: 300.0, x: 480.0, y: 200.0 },
    Blob { color: "#0ea5e9", r: 340.0, x: -200.0, y: 260.0 },
    Blob { color: "#f59e0b", r: 260.0, x: 380.0, y: -260.0 },
];
const CARDS: [(&str, f64, &str, &str); 2] =
    [("left30.mp4", 80.0, "Host", "card-clip-0"), ("right30.mp4", 980.0, "Guest", "card-clip-1")];
const CARD_Y: f64 = 200.0;
const CARD_W: f64 = 860.0;
const CARD_H: f64 = 484.0;

struct Podcast {
    frames: usize,
    words: Vec<&'static str>,
    title_spring: AnimationRuntime,
    card_spring: AnimationRuntime,
}

fn fmt(s: f64) -> String {
    format!("{:02}:{:02}", (s / 60.0).floor() as u64, (s % 60.0).floor() as u64)
}

impl Video for Podcast {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(self.frames)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let mut frame = frame;
        let fps = Self::FPS as f64;
        let f = frame.index as f64;
        let t = f / fps;
        let spring = |frame: &Frame, on: f64, rt: &AnimationRuntime| -> f64 {
            frame.animate_runtime(AnimateRuntimeInput { on_second: on as f32, from: 0.0_f32, to: 1.0, animation_runtime: rt })
                as f64
        };

        // animated blurred gradient background
        let blobs: Vec<Svgr> = BLOBS
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let i = i as f64;
                let cx = 960.0 + b.x + 220.0 * (t * 0.35 + i * 1.7).sin();
                let cy = 540.0 + b.y + 160.0 * (t * 0.28 + i * 2.3).cos();
                fframes::svgr!(<circle cx={cx} cy={cy} r={b.r} fill={b.color} opacity="0.75" />)
            })
            .collect();

        // title card
        let title = spring(&frame, 0.0, &self.title_spring);
        let title_tf = around(390.0, 106.0, 0.0, (1.0 - title) * -60.0, &format!("scale({})", 0.8 + 0.2 * title));

        // video feeds
        let cards: Vec<Svgr> = CARDS
            .iter()
            .enumerate()
            .map(|(i, &(src, x, label, clip))| {
                let p = spring(&frame, (8.0 + 6.0 * i as f64) / fps, &self.card_spring);
                let video = frame.get_synced_video_frame(
                    ctx,
                    src,
                    &SyncVideoFrameInput { start_from: 0.0, looping: false, editor_fallback_image: None },
                );
                let image = match video {
                    Some(v) => {
                        let img = v.into_image();
                        fframes::svgr!(
                            <image href={img.href()} x={x} y={CARD_Y} width={CARD_W} height={CARD_H}
                                preserveAspectRatio="xMidYMid slice" clip-path={format!("url(#{clip})")} />
                        )
                    }
                    None => Svgr::empty(),
                };
                let tag_top = CARD_Y + CARD_H - 64.0;
                fframes::svgr!(
                    <g opacity={clamp01(p)} transform={format!("translate(0 {})", (1.0 - p) * 80.0)}>
                        <rect x={x} y={CARD_Y} width={CARD_W} height={CARD_H} rx="28" fill="#000" filter="url(#card-shadow)" />
                        {image}
                        <rect x={x + 20.0} y={tag_top} width="150" height="44" rx="22" fill="#000" fill-opacity="0.55" />
                        <text x={x + 42.0} y={baseline(tag_top, 44.0, 22.0, DM_SANS.0, DM_SANS.1)}
                            font-family="DM Sans" font-weight="500" font-size="22" fill="#fff">
                            {label}
                        </text>
                    </g>
                )
            })
            .collect();

        // word-by-word captions
        let k_cur = (((t - CAPTION_START) / WORD_SECONDS).floor().max(0.0) as usize).min(self.words.len() - 1);
        let line = k_cur / WORDS_PER_LINE;
        let line_words = &self.words[line * WORDS_PER_LINE..((line + 1) * WORDS_PER_LINE).min(self.words.len())];
        let font = FontQuery { family: "DM Sans", size: 54, weight: 500, ..Default::default() };
        let widths: Vec<f64> =
            line_words.iter().map(|w| frame.text_width(ctx, font, *w).unwrap_or(0) as f64).collect();
        let box_w = widths.iter().sum::<f64>() + 16.0 * (line_words.len() as f64 - 1.0) + 60.0;
        let box_x = 960.0 - box_w / 2.0;
        let word_baseline = baseline(760.0, 90.0, 54.0, DM_SANS.0, DM_SANS.1);
        let mut wx = box_x + 30.0;
        let words: Vec<Svgr> = line_words
            .iter()
            .zip(&widths)
            .enumerate()
            .map(|(j, (&word, &w))| {
                let k = line * WORDS_PER_LINE + j;
                let start = CAPTION_START + k as f64 * WORD_SECONDS;
                let a = if t < start { 0.0 } else { ease_out_cubic((t - start) / 0.15) };
                let active = k == k_cur && t >= start;
                let tf = around(wx + w / 2.0, 805.0, 0.0, (1.0 - a) * 16.0, if active { "scale(1.1)" } else { "scale(1)" });
                let node = fframes::svgr!(
                    <text x={wx} y={word_baseline} opacity={a} transform={tf} font-family="DM Sans" font-weight="500"
                        font-size="54" fill={if active { "#facc15" } else { "#fff" }}>
                        {word}
                    </text>
                );
                wx += w + 16.0;
                node
            })
            .collect();

        let progress_w = (1760.0 * f / (self.frames as f64 - 1.0)).max(10.0);
        let time = format!("{} / {}", fmt(t), fmt(self.frames as f64 / fps));

        fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width={Self::WIDTH} height={Self::HEIGHT}>
                <defs>
                    <filter id="bg-blur" filterUnits="userSpaceOnUse" x="-600" y="-600" width="3120" height="2280">
                        <feGaussianBlur stdDeviation="90" />
                    </filter>
                    <filter id="title-shadow" filterUnits="userSpaceOnUse" x="20" y="4" width="740" height="264">
                        <feDropShadow dx="0" dy="20" stdDeviation="20" flood-color="#000" flood-opacity="0.35" />
                    </filter>
                    <filter id="card-shadow" x="-15%" y="-25%" width="130%" height="160%">
                        <feDropShadow dx="0" dy="30" stdDeviation="30" flood-color="#000" flood-opacity="0.5" />
                    </filter>
                    <clipPath id="card-clip-0"><rect x="80" y={CARD_Y} width={CARD_W} height={CARD_H} rx="28" /></clipPath>
                    <clipPath id="card-clip-1"><rect x="980" y={CARD_Y} width={CARD_W} height={CARD_H} rx="28" /></clipPath>
                    <linearGradient id="accent" x1="0" y1="0" x2="1" y2="0">
                        <stop offset="0" stop-color="#7c3aed" />
                        <stop offset="1" stop-color="#db2777" />
                    </linearGradient>
                </defs>
                <rect width={Self::WIDTH} height={Self::HEIGHT} fill="#0b0a16" />
                <g filter="url(#bg-blur)">{blobs}</g>

                <g opacity={clamp01(title)} transform={title_tf}>
                    <rect x="80" y="64" width="620" height="84" rx="42" fill="url(#accent)" filter="url(#title-shadow)" />
                    <text x="116" y={baseline(64.0, 84.0, 36.0, DM_SANS.0, DM_SANS.1)} font-family="DM Sans"
                        font-weight="500" font-size="36" fill="#fff">
                        "The Render Loop · Episode 42"
                    </text>
                </g>

                {cards}

                <rect x={box_x} y="760" width={box_w} height="90" rx="20" fill="#000" fill-opacity="0.45" />
                {words}

                <text x="1840" y={baseline(950.0, 40.0, 26.0, DM_SANS.0, DM_SANS.1)} text-anchor="end"
                    font-family="DM Sans" font-weight="500" font-size="26" fill="#fff" fill-opacity="0.8">
                    {time}
                </text>
                <rect x="80" y="1000" width="1760" height="10" rx="5" fill="#fff" fill-opacity="0.15" />
                <rect x="80" y="1000" width={progress_w} height="10" rx="5" fill="url(#accent)" />
            </svg>
        )
    }
}

fn main() {
    let seconds: usize = std::env::var("PODCAST_SECONDS").ok().and_then(|v| v.parse().ok()).unwrap_or(20);
    let spring = |stiffness, damping| AnimationRuntime::new(3.0, &Easing::Spring { mass: 1.0, stiffness, damping });
    let video = Podcast {
        frames: seconds * 30,
        words: CAPTION_TEXT.split(' ').collect(),
        title_spring: spring(120.0, 14.0),
        card_spring: spring(100.0, 15.0),
    };
    shared::run("podcast", &video, video.frames);
}
