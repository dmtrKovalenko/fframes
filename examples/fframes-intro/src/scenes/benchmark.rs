//! The drop after the riser. Remotion and fframes render the same 100,000
//! text nodes; the race runs as a time-lapse of the measured wall-clock times.

use fframes::{Color, Duration, FFramesContext, Frame, Scene, ShaderUniforms, Svgr};

use crate::beat::*;
use crate::facts::*;
use crate::shaders::SHADERS;
use crate::ui::*;

beat_scene!(BenchmarkScene, Some(96.0), Some(128.0));

const RACE_START: f32 = 2.0;
/// The slower renderer crosses the line after this many beats.
const RACE_BEATS: f32 = 12.0;
const RESULT_AT: f32 = 16.0;
const TABLE_AT: f32 = 22.0;

impl Scene for BenchmarkScene {
    fn duration(&self) -> Duration<'_> {
        Self::frames()
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let lb = Self::lb(&frame);
        if (RESULT_AT..TABLE_AT).contains(&lb) {
            return result(lb - RESULT_AT);
        }
        let grid = SHADERS.grid.draw(
            &frame,
            ShaderUniforms::new()
                .float("uSpeed", 2.0)
                .float("uHorizon", 0.62)
                .float("uBright", 0.35)
                .color("uInk", Color::hex("#6f6a63"))
                .color("uHot", Color::hex(ORANGE)),
        );
        let body = if lb < RESULT_AT {
            race(lb)
        } else {
            table(lb - TABLE_AT)
        };
        fframes::svgr!(
            <g>
                <image href={grid.href()} x="0" y="0" width="1920" height="1080" />
                {body}
            </g>
        )
    }
}

fn slowest() -> f32 {
    BENCH_REMOTION_S.max(BENCH_FFRAMES_S).max(0.001)
}

fn race(lb: f32) -> Svgr<'static> {
    let exit = expo_in(prog(lb, 15.6, 16.0));
    // seconds of the real run shown at this beat
    let real = prog(lb, RACE_START, RACE_START + RACE_BEATS) * slowest();
    let speedup = slowest() / (RACE_BEATS * BEAT);
    let lanes = [
        (
            BENCH_REMOTION_LABEL,
            "CHROME + REACT, SCREENSHOT PER FRAME",
            BENCH_REMOTION_S,
            GREY,
            470.0,
        ),
        (
            BENCH_FFRAMES_LABEL,
            "RUST, SVG TREE → GPU",
            BENCH_FFRAMES_S,
            ORANGE,
            700.0,
        ),
    ];
    let rows: Vec<Svgr> = lanes
        .iter()
        .enumerate()
        .map(|(i, (name, sub, secs, color, y))| {
            let appear = snap(lb - 0.6 - i as f32 * 0.3);
            let p = (real / secs.max(0.001)).min(1.0);
            let done = p >= 1.0;
            let shown = real.min(*secs);
            let bar_w = (1620.0 * p).max(0.5);
            let done_at = RACE_START + RACE_BEATS * secs / slowest();
            let stamp = snap(lb - done_at);
            let flash = if done { (-(lb - done_at) * 6.0).exp() } else { 0.0 };
            let frames_done = (30.0 * p).floor() as u32;
            fframes::svgr!(
                <g opacity={prog(lb, 0.6 + i as f32 * 0.3, 0.7 + i as f32 * 0.3)} transform={format!("translate({} 0)", (1.0 - appear) * -120.0)}>
                    <text x="150" y={*y} font-family={DISPLAY} font-size="58" letter-spacing="-2" fill={if i == 1 { ORANGE } else { BONE }}>{*name}</text>
                    <text x="150" y={*y + 34.0} font-family={MONO} font-weight="500" font-size="18" letter-spacing="2.5" fill={GREY}>{*sub}</text>
                    <rect x="150" y={*y + 56.0} width="1620" height="64" fill="#141312" stroke="#2f2c29" stroke-width="1.5" />
                    <rect x="150" y={*y + 56.0} width={bar_w} height="64" fill={*color} />
                    <rect x="150" y={*y + 56.0} width="1620" height="64" fill={BONE} opacity={flash * 0.6} />
                    <text x="1770" y={*y} text-anchor="end" font-family={MONO} font-weight="600" font-size="52" fill={if done { *color } else { BONE }}>{format!("{shown:.3}s")}</text>
                    <text x="170" y={*y + 100.0} font-family={MONO} font-weight="600" font-size="22" letter-spacing="2" fill={INK} opacity={if bar_w > 260.0 { 1.0 } else { 0.0 }}>{format!("{frames_done} / 30 FRAMES")}</text>
                    <g opacity={if done { 1.0 } else { 0.0 }} transform={format!("translate(0 {})", (1.0 - stamp) * 20.0)}>
                        <text x="1750" y={*y + 100.0} text-anchor="end" font-family={MONO} font-weight="700" font-size="24" letter-spacing="4" fill={INK}>"DONE"</text>
                    </g>
                </g>
            )
        })
        .collect();
    fframes::svgr!(
        <g opacity={1.0 - exit}>
            {Slam::new(150.0, 250.0, "100,000 TEXT NODES", DISPLAY, 120.0, BONE).draw(lb, 0.0, 140.0)}
            {label(154.0, 310.0, "30 FRAMES · 1000×1000 · PNG · SAME LAYOUT, SAME FONT".to_owned(), GREY, 20.0, "start")}
            {rows}
            <g opacity={prog(lb, RACE_START, RACE_START + 0.2)}>
                {label(1770.0, 310.0, format!("TIME-LAPSE ×{speedup:.1} · MEASURED WALL CLOCK"), ORANGE, 20.0, "end")}
            </g>
        </g>
    )
}

/// N× FASTER on an orange field.
fn result(l: f32) -> Svgr<'static> {
    let ratio = BENCH_REMOTION_S / BENCH_FFRAMES_S.max(0.001);
    let n = 1.0 + (ratio - 1.0) * expo_out(prog(l, 0.0, 1.2));
    let s = 1.0 + (1.0 - expo_out(l / 0.35)) * 0.15;
    let word = snap(l - 1.0);
    let exit = expo_in(prog(l, 5.7, 6.0));
    fframes::svgr!(
        <g opacity={1.0 - exit}>
            <rect width="1920" height="1080" fill={ORANGE} />
            <g transform={format!("translate(150 760) scale({s}) translate(-150 -760)")}>
                <text x="130" y="760" font-family={DISPLAY} font-size="480" letter-spacing="-30" fill={INK}>{format!("{n:.2}×")}</text>
            </g>
            <text x="150" y={900.0 + (1.0 - word) * 60.0} font-family={DISPLAY} font-size="120" letter-spacing="-4" fill={INK} opacity={prog(l, 1.0, 1.08)}>"FASTER THAN REMOTION"</text>
            <g font-family={MONO} font-weight="600" font-size="22" letter-spacing="4" fill={INK}>
                <text x="150" y="170">"SAME 100,000 TEXT NODES"</text>
                <text x="1770" y="170" text-anchor="end">"NO VIDEO ENCODING · MEDIAN WALL CLOCK"</text>
            </g>
            <rect x="150" y="196" width="1620" height="3" fill={INK} />
        </g>
    )
}

fn table(l: f32) -> Svgr<'static> {
    let rows = [
        (BENCH_REMOTION_LABEL, BENCH_REMOTION_S, GREY),
        (BENCH_FFRAMES_LABEL, BENCH_FFRAMES_S, ORANGE),
    ];
    let items: Vec<Svgr> = rows
        .iter()
        .enumerate()
        .map(|(i, (name, secs, color))| {
            let at = 0.3 + i as f32 * 0.5;
            let s = snap(l - at);
            let y = 520.0 + i as f32 * 130.0;
            let fps = 30.0 / secs.max(0.001);
            fframes::svgr!(
                <g opacity={prog(l, at, at + 0.08)} transform={format!("translate({} 0)", (1.0 - s) * 80.0)}>
                    <rect x="150" y={y - 70.0} width="1620" height="110" fill="#0f0e0d" fill-opacity="0.85" stroke="#2f2c29" stroke-width="1.5" />
                    <rect x="150" y={y - 70.0} width="6" height="110" fill={*color} />
                    <text x="190" y={y} font-family={DISPLAY} font-size="48" letter-spacing="-1" fill={BONE}>{*name}</text>
                    <text x="1250" y={y} text-anchor="end" font-family={MONO} font-weight="600" font-size="48" fill={*color}>{format!("{secs:.3} s")}</text>
                    <text x="1740" y={y} text-anchor="end" font-family={MONO} font-weight="600" font-size="48" fill={*color}>{format!("{fps:.3} fps")}</text>
                </g>
            )
        })
        .collect();
    let exit = expo_in(prog(l, 9.6, 10.0));
    fframes::svgr!(
        <g opacity={1.0 - exit}>
            {Slam::new(150.0, 300.0, "THE NUMBERS", DISPLAY, 110.0, BONE).draw(l, 0.0, 120.0)}
            {label(1250.0, 400.0, "WALL CLOCK".to_owned(), GREY, 18.0, "end")}
            {label(1740.0, 400.0, "THROUGHPUT".to_owned(), GREY, 18.0, "end")}
            {items}
            {label(154.0, 820.0, "VIDEO ENCODING NOT MEASURED".to_owned(), BONE, 18.0, "start")}
            {label(154.0, 860.0, BENCH_NOTE.to_owned(), GREY, 18.0, "start")}
            {label(154.0, 900.0, "REPRODUCE: RENDER-BENCH/VS-REMOTION IN THE FFRAMES REPO".to_owned(), ORANGE, 18.0, "start")}
        </g>
    )
}
