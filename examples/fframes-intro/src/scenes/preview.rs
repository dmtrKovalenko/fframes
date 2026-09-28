//! After the benchmark: the native preview window playing this video in real
//! time on the GPU (a capture of `cargo run -- preview`). On the second half
//! the window fills the frame and NO BROWSER AT ALL ANYWHERE runs over it.

use fframes::{
    Color, Duration, FFramesContext, FFramesSyncedVideoFrame, Frame, Scene, ShaderUniforms, Svgr,
    SyncVideoFrameInput,
};

use crate::beat::*;
use crate::shaders::SHADERS;
use crate::ui::*;

beat_scene!(PreviewScene, Some(128.0), Some(160.0));

/// Beat inside the scene where the window goes full frame and the words start.
const TAKEOVER: f32 = 16.0;

/// A takeover row: words with the beat each one lands on, baseline, color and
/// drift direction.
type Row = (&'static [(&'static str, f32)], f32, &'static str, f32);

const ROWS: &[Row] = &[
    (&[("NO", 16.0), ("BROWSER", 17.0)], 380.0, BONE, -1.0),
    (&[("AT", 18.0), ("ALL", 19.0)], 690.0, ORANGE, 1.0),
    (&[("ANYWHERE", 20.0)], 1000.0, BONE, -1.0),
];

/// The captured window (1600x896), placed on the right.
const WIN: (f32, f32, f32, f32) = (590.0, 262.0, 1180.0, 660.8);

const KEYS: &[(&str, &str)] = &[
    ("SPACE", "PLAY / PAUSE"),
    ("H  L", "SEEK A SECOND"),
    ("J  K", "STEP A FRAME"),
];

impl Scene for PreviewScene {
    fn duration(&self) -> Duration<'_> {
        Self::frames()
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let lb = Self::lb(&frame);
        let grid = SHADERS.grid.draw(
            &frame,
            ShaderUniforms::new()
                .float("uSpeed", 1.5)
                .float("uHorizon", 0.7)
                .float("uBright", 0.3)
                .color("uInk", Color::hex("#6f6a63"))
                .color("uHot", Color::hex(ORANGE)),
        );
        let capture = frame
            .get_synced_video_frame(
                ctx,
                "preview_capture.mp4",
                &SyncVideoFrameInput {
                    start_from: 0.0,
                    looping: true,
                    editor_fallback_image: None,
                },
            )
            .map(|f| f.into_image());

        // the window grows from its place on the right to the full frame
        let full = expo_out(prog(lb, TAKEOVER, TAKEOVER + 0.7));
        let (x, y, w, h) = (
            lerp(WIN.0, 0.0, full),
            lerp(WIN.1, 0.0, full),
            lerp(WIN.2, 1920.0, full),
            lerp(WIN.3, 1075.2, full),
        );
        let radius = 14.0 * (1.0 - full);
        let enter = snap(lb - 0.5);
        let s = 0.9 + 0.1 * enter;
        let t = format!(
            "translate({} {}) scale({s}) translate({} {})",
            x + w / 2.0,
            y + h / 2.0 + (1.0 - enter) * 120.0,
            -(x + w / 2.0),
            -(y + h / 2.0)
        );
        let window = capture
            .map(|img| {
                fframes::svgr!(
                    <g>
                        <clipPath id="preview-win"><rect x={x} y={y} width={w} height={h} rx={radius} /></clipPath>
                        <rect x={x} y={y + 18.0} width={w} height={h} rx={radius} fill="#000000" opacity={0.55 * (1.0 - full)} filter="url(#preview-shadow)" />
                        <g clip-path="url(#preview-win)">
                            <image href={img.href()} x={x} y={y} width={w} height={h} />
                        </g>
                        <rect x={x} y={y} width={w} height={h} rx={radius} fill="none" stroke="#3d3a35" stroke-width="1.5" opacity={1.0 - full} />
                    </g>
                )
            })
            .unwrap_or_default();

        let keys: Vec<Svgr> = KEYS
            .iter()
            .enumerate()
            .map(|(i, (key, what))| {
                let at = 3.0 + i as f32 * 0.5;
                let k = snap(lb - at);
                let ky = 560.0 + i as f32 * 78.0;
                fframes::svgr!(
                    <g opacity={prog(lb, at, at + 0.1)} transform={format!("translate({} 0)", (1.0 - k) * -60.0)}>
                        <rect x="150" y={ky - 38.0} width="120" height="54" rx="8" fill="#161514" stroke={BONE} stroke-opacity="0.5" stroke-width="1.5" />
                        <text x="210" y={ky - 3.0} text-anchor="middle" font-family={MONO} font-weight="600" font-size="20" fill={BONE}>{*key}</text>
                        <text x="290" y={ky - 3.0} font-family={MONO} font-weight="500" font-size="18" letter-spacing="2" fill={GREY}>{*what}</text>
                    </g>
                )
            })
            .collect();
        let cmd = prog(lb, 2.0, 2.4);
        // the side panel and title leave as the window takes over
        let side = 1.0 - prog(lb, TAKEOVER - 0.2, TAKEOVER + 0.1);
        let exit = expo_in(prog(lb, 31.5, 32.0));

        fframes::svgr!(
            <g opacity={1.0 - exit}>
                <defs>
                    <filter id="preview-shadow" x="-10%" y="-10%" width="120%" height="130%">
                        <feGaussianBlur stdDeviation="24" />
                    </filter>
                </defs>
                <image href={grid.href()} x="0" y="0" width="1920" height="1080" />
                <g opacity={side}>
                    {Slam::new(150.0, 205.0, "WATCH IT", DISPLAY, 110.0, BONE).draw(lb, 0.0, 150.0)}
                    {Slam::new(820.0, 205.0, "LIVE.", DISPLAY, 110.0, ORANGE).draw(lb - 1.0, 0.0, 150.0)}
                </g>
                <g transform={t} opacity={prog(lb, 0.5, 0.6)}>{window}</g>
                <g opacity={cmd * side}>
                    <text x="150" y="330" font-family={MONO} font-weight="500" font-size="20" letter-spacing="3" fill={GREY}>"NATIVE PREVIEW WINDOW"</text>
                    <text x="150" y="380" font-family={MONO} font-weight="600" font-size="26" fill={ORANGE}>"$"</text>
                    <text x="180" y="380" font-family={MONO} font-weight="500" font-size="26" fill={BONE}>"cargo run -- preview"</text>
                    <text x="150" y="440" font-family={MONO} font-weight="500" font-size="20" letter-spacing="2" fill={BONE}>"GPU · WITH SOUND"</text>
                    <text x="150" y="472" font-family={MONO} font-weight="500" font-size="20" letter-spacing="2" fill={BONE}>"NO BROWSER, NO BUNDLER"</text>
                </g>
                <g opacity={side}>
                    {keys}
                    {callout(WIN.0 + 720.0, WIN.1 + 12.0, WIN.0 + 720.0, 170.0, "REAL-TIME, FRAME BY FRAME".to_owned(), ORANGE, prog(lb, 6.0, 6.8))}
                    {callout(WIN.0 + 354.0, WIN.1 + 630.0, 560.0, 975.0, "PLAY · SEEK · LOOP · SCRUB".to_owned(), BONE, prog(lb, 8.0, 8.8))}
                </g>
                {takeover(lb)}
            </g>
        )
    }
}

/// NO BROWSER / AT ALL / ANYWHERE: rows larger than the frame that slam in
/// word by word and drift in opposite directions over the dimmed window.
fn takeover(lb: f32) -> Svgr<'static> {
    let l = lb - TAKEOVER;
    if l < 0.0 {
        return Svgr::empty();
    }
    let dim = prog(l, 0.0, 0.5) * 0.6;
    let rows: Vec<Svgr> = ROWS
        .iter()
        .map(|(words, y, color, dir)| {
            let first = words[0].1 - TAKEOVER;
            let shown: Vec<&str> = words
                .iter()
                .filter(|(_, at)| lb >= *at)
                .map(|(w, _)| *w)
                .collect();
            if shown.is_empty() {
                return Svgr::empty();
            }
            let drift = dir * (l - first).max(0.0) * 34.0;
            let x = 60.0 + drift + if *dir > 0.0 { -140.0 } else { 0.0 };
            let text = shown.join(" ");
            fframes::svgr!(
                <g>{Slam::new(x, *y, text, DISPLAY, 330.0, color).draw(l - first, 0.0, 220.0)}</g>
            )
        })
        .collect();
    fframes::svgr!(
        <g>
            <rect width="1920" height="1080" fill="#050505" opacity={dim} />
            {rows}
        </g>
    )
}
