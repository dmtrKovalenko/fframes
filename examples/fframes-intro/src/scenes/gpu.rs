//! The first drop on "GPU", then the render pipeline, which has no browser in it.

use fframes::{Duration, FFramesContext, Frame, Scene, Svgr};

use crate::beat::*;
use crate::ui::*;

beat_scene!(GpuScene, Some(48.0), Some(56.0));

impl Scene for GpuScene {
    fn duration(&self) -> Duration<'_> {
        Self::frames()
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let lb = Self::lb(&frame);
        if lb < 2.0 { slam(lb) } else { pipeline(lb - 2.0) }
    }
}

fn slam(lb: f32) -> Svgr<'static> {
    let s = 1.0 + (1.0 - expo_out(lb / 0.35)) * 0.18;
    let shake = (1.0 - prog(lb, 0.0, 0.5)) * 14.0;
    let dx = (hash(lb * 97.0) - 0.5) * shake;
    let dy = (hash(lb * 53.0 + 3.0) - 0.5) * shake;
    let dot = snap(lb - 1.0);
    let t = format!("translate({} {}) translate(150 790) scale({s}) translate(-150 -790)", dx, dy);
    fframes::svgr!(
        <g>
            <rect width="1920" height="1080" fill={ORANGE} />
            <g transform={t}>
                <text x="120" y="880" font-family={DISPLAY} font-size="760" letter-spacing="-40" fill={INK}>"GPU"</text>
                <rect x="1605" y="765" width="115" height="115" fill={INK} transform={format!("translate(0 {})", (1.0 - dot) * -300.0)} opacity={prog(lb, 1.0, 1.05)} />
            </g>
            <g font-family={MONO} font-weight="600" font-size="22" letter-spacing="4" fill={INK}>
                <text x="130" y="170">"SKIA"</text>
                <text x="330" y="170">"METAL"</text>
                <text x="540" y="170">"VULKAN"</text>
                <text x="1790" y="170" text-anchor="end">"NO BROWSER IN THE LOOP"</text>
            </g>
            <rect x="130" y="196" width="1660" height="3" fill={INK} />
        </g>
    )
}

/// The pipeline from svgr! through Skia, the GPU and ffmpeg to an .mp4, with
/// packets moving on the beat.
fn pipeline(l: f32) -> Svgr<'static> {
    let nodes = [
        ("svgr!", "SVG TREE / FRAME"),
        ("SKIA", "GANESH, GPU"),
        ("METAL", "VULKAN ON LINUX"),
        ("FFMPEG", "H.264 · HEVC · VP9"),
        (".MP4", "OUT.MP4"),
    ];
    let title_in = snap(l);
    let exit = expo_in(prog(l, 5.7, 6.0));
    let x0 = 190.0;
    let gap = 330.0;
    let y = 640.0;
    let mut boxes = Vec::new();
    for (i, (name, sub)) in nodes.iter().enumerate() {
        let at = 0.4 + i as f32 * 0.35;
        let s = snap(l - at);
        let x = x0 + i as f32 * gap;
        let hot = i == 2;
        let stroke = if hot { ORANGE } else { "#5a564f" };
        boxes.push(fframes::svgr!(
            <g opacity={prog(l, at, at + 0.1)} transform={format!("translate({x} {})", y + (1.0 - s) * 50.0)}>
                <rect x="0" y="-70" width="250" height="140" fill={if hot { ORANGE } else { "#121110" }} stroke={stroke} stroke-width="2" />
                <text x="125" y="8" text-anchor="middle" font-family={DISPLAY} font-size="40" fill={if hot { INK } else { BONE }}>{*name}</text>
                <text x="125" y="46" text-anchor="middle" font-family={MONO} font-weight="500" font-size="15" letter-spacing="1.5" fill={if hot { INK } else { GREY }}>{*sub}</text>
            </g>
        ));
    }
    // links and packets
    let mut links = Vec::new();
    for i in 0..4 {
        let at = 0.6 + i as f32 * 0.35;
        let p = expo_out(prog(l, at, at + 0.5));
        let xa = x0 + i as f32 * gap + 250.0;
        let len = gap - 250.0;
        links.push(fframes::svgr!(<rect x={xa} y={y - 1.0} width={(len * p).max(0.5)} height="2" fill="#5a564f" />));
        for k in 0..3 {
            let phase = (l * 2.0 + k as f32 / 3.0 + i as f32 * 0.21).fract();
            if l > at + 0.5 {
                let px = xa + phase * len;
                links.push(fframes::svgr!(<rect x={px - 6.0} y={y - 4.0} width="12" height="8" fill={ORANGE} />));
            }
        }
    }
    let fps = "1920×1080 · 60 FPS".to_owned();
    fframes::svgr!(
        <g opacity={1.0 - exit}>
            <g transform={format!("translate(0 {})", (1.0 - title_in) * 60.0)} opacity={prog(l, 0.0, 0.1)}>
                <text x="186" y="330" font-family={DISPLAY} font-size="120" letter-spacing="-4" fill={BONE}>"DIRECTLY ON"</text>
                <text x="186" y="450" font-family={DISPLAY} font-size="120" letter-spacing="-4" fill={ORANGE}>"THE GPU."</text>
            </g>
            {links}
            {boxes}
            {label(190.0, 820.0, "NO HEADLESS BROWSER. NO SCREENSHOTS. NO REACT.".to_owned(), GREY, 22.0, "start")}
            <text x="1730" y="450" text-anchor="end" font-family={MONO} font-weight="600" font-size="30" fill={ORANGE} opacity={prog(l, 2.0, 2.2)}>{fps}</text>
        </g>
    )
}
