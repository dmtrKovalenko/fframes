//! The breakdown: the loop an agent runs while it makes a video, with real
//! output of this project's command line on the hi-hat hits. Ends on "this
//! video was made by an agent".

use fframes::{Color, Duration, FFramesContext, Frame, Scene, ShaderUniforms, Svgr};

use crate::beat::*;
use crate::shaders::SHADERS;
use crate::ui::*;

beat_scene!(AgentsScene, Some(128.0), Some(168.0));

/// Global beats of the stutter hits that bring in a new card.
pub const CARD_BEATS: &[f32] = &[135.0, 139.0, 143.0, 147.0, 151.0, 155.0];

const NODES: &[&str] = &["WRITE", "TIMELINE", "INSPECT", "STRIP", "FRAME", "RENDER"];

enum Body {
    Lines(&'static [(&'static str, &'static str)]),
    Image(&'static str, f32),
}

struct Card {
    node: usize,
    command: &'static str,
    body: Body,
}

/// Output of the commands, as printed for this video.
const CARDS: &[Card] = &[
    Card {
        node: 1,
        command: "cargo run -- timeline",
        body: Body::Lines(&[
            ("1920x1080 @ 60 fps, 6558 frames (109.30s)", BONE),
            ("#0   PromptScene      0.00s..2.15s", GREY),
            ("#1   OriginScene      2.15s..16.70s", GREY),
            ("#2   CodeScene       16.70s..23.97s", GREY),
            ("#3   GpuScene        23.97s..27.60s", GREY),
            ("audio music.wav       0.000s..109.300s", ORANGE),
        ]),
    },
    Card {
        node: 2,
        command: "cargo run -- inspect",
        body: Body::Lines(&[
            ("checked 465 frames: 10 findings", BONE),
            ("0 errors · 10 warnings: marquee rows and", GREY),
            ("two slams that enter from off the canvas", GREY),
        ]),
    },
    Card { node: 3, command: "cargo run -- strip all -n 30", body: Body::Image("strip_self.jpg", 1.887) },
    Card { node: 4, command: "cargo run -- frame Shader@2.7s", body: Body::Image("frame_self.jpg", 1.778) },
    Card {
        node: 0,
        command: "cargo run -- audio analyze",
        body: Body::Lines(&[
            ("integrated -14.2 LUFS, range 10.3 LU", BONE),
            ("true peak -1.2 dBTP, clipped samples 0", BONE),
            ("GpuScene        23.97s..27.60s   -13.0 LUFS", GREY),
            ("BenchmarkScene  45.78s..60.33s   -13.1 LUFS", GREY),
        ]),
    },
    Card {
        node: 5,
        command: "cargo run -- render --draft",
        body: Body::Lines(&[
            ("out.mp4 frames 0..6558 (0.00s..109.30s)", GREY),
            ("960x540 in 3.6s", ORANGE),
        ]),
    },
];

const RING_X: f32 = 1370.0;
const RING_Y: f32 = 590.0;
const RING_R: f32 = 250.0;

impl Scene for AgentsScene {
    fn duration(&self) -> Duration<'_> {
        Self::frames()
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let lb = Self::lb(&frame);
        let bg = SHADERS.contour.draw(
            &frame,
            ShaderUniforms::new()
                .float2("uWell", RING_X / 1920.0, RING_Y / 1080.0)
                .float("uDepth", 0.9)
                .float("uDensity", 20.0)
                .float("uBright", 0.3 * prog(lb, 0.0, 2.0))
                .color("uInk", Color::hex("#8a857c"))
                .color("uHot", Color::hex(ORANGE)),
        );
        let bg = fframes::svgr!(<image href={bg.href()} x="0" y="0" width="1920" height="1080" />);
        if lb >= 32.0 {
            return fframes::svgr!(<g>{bg}{confession(lb - 32.0)}</g>);
        }
        let title_a = snap(lb);
        let title_b = snap(lb - 1.5);
        let exit = expo_in(prog(lb, 31.6, 32.0));

        // which card is on screen
        let local_cards: Vec<f32> = CARD_BEATS.iter().map(|b| b - 128.0).collect();
        let active = local_cards.iter().rposition(|b| lb >= *b);
        let card_svg = match active {
            Some(i) => card(ctx, &CARDS[i], lb - local_cards[i], false),
            None => prompt_card(lb - 2.5),
        };
        let lit = active.map(|i| CARDS[i].node);

        fframes::svgr!(
            <g>
                {bg}
                <g opacity={1.0 - exit}>
                    <g transform={format!("translate(0 {})", (1.0 - title_a) * 60.0)} opacity={prog(lb, 0.0, 0.1)}>
                        <text x="150" y="235" font-family={DISPLAY} font-size="110" letter-spacing="-4" fill={BONE}>"BUILT FOR THE"</text>
                    </g>
                    <g transform={format!("translate(0 {})", (1.0 - title_b) * 60.0)} opacity={prog(lb, 1.5, 1.6)}>
                        <text x="150" y="345" font-family={DISPLAY} font-size="110" letter-spacing="-4" fill={ORANGE}>"AGENTIC LOOP."</text>
                    </g>
                    {ring(lb, lit)}
                    {card_svg}
                </g>
            </g>
        )
    }
}

fn ring(lb: f32, lit: Option<usize>) -> Svgr<'static> {
    let draw = cubic_in_out(prog(lb, 2.0, 5.0));
    let circ = std::f32::consts::TAU * RING_R;
    let dash = format!("{} {}", circ * draw, circ);
    let mut nodes = Vec::new();
    for (i, name) in NODES.iter().enumerate() {
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 / NODES.len() as f32 * std::f32::consts::TAU;
        let (x, y) = (RING_X + a.cos() * RING_R, RING_Y + a.sin() * RING_R);
        let at = 2.5 + i as f32 * 0.4;
        let s = snap(lb - at);
        let on = lit == Some(i);
        let lx = RING_X + a.cos() * (RING_R + 44.0);
        let ly = RING_Y + a.sin() * (RING_R + 44.0) + 8.0;
        let anchor = if a.cos() > 0.3 { "start" } else if a.cos() < -0.3 { "end" } else { "middle" };
        let size = 18.0 + if on { 8.0 } else { 0.0 };
        nodes.push(fframes::svgr!(
            <g opacity={prog(lb, at, at + 0.1)}>
                <rect x={x - size / 2.0} y={y - size / 2.0} width={size * s.max(0.05)} height={size * s.max(0.05)}
                      fill={if on { ORANGE } else { BG }} stroke={if on { ORANGE } else { BONE }} stroke-width="2" />
                <text x={lx} y={ly} text-anchor={anchor} font-family={MONO} font-weight="600" font-size="22" letter-spacing="3"
                      fill={if on { ORANGE } else { BONE }}>{*name}</text>
            </g>
        ));
    }
    // comet: one lap per 8 beats, a short trail behind it
    let comet: Vec<Svgr> = if lb > 5.0 {
        (0..10)
            .map(|k| {
                let t = (lb - 5.0) / 8.0 - k as f32 * 0.008;
                let a = -std::f32::consts::FRAC_PI_2 + t * std::f32::consts::TAU;
                let (x, y) = (RING_X + a.cos() * RING_R, RING_Y + a.sin() * RING_R);
                let r = 7.0 - k as f32 * 0.5;
                fframes::svgr!(<circle cx={x} cy={y} r={r} fill={ORANGE} opacity={1.0 - k as f32 / 10.0} />)
            })
            .collect()
    } else {
        Vec::new()
    };
    let center = prog(lb, 4.0, 4.5);
    fframes::svgr!(
        <g>
            <circle cx={RING_X} cy={RING_Y} r={RING_R} fill="none" stroke={GREY} stroke-width="1.5"
                    stroke-dasharray={dash} transform={format!("rotate(-90 {RING_X} {RING_Y})")} />
            <circle cx={RING_X} cy={RING_Y} r={RING_R - 30.0} fill="none" stroke={DIM} stroke-width="1" stroke-dasharray="4 8" opacity={draw} />
            {comet}
            {nodes}
            <g opacity={center}>
                <text x={RING_X} y={RING_Y - 6.0} text-anchor="middle" font-family={SERIF} font-style="italic" font-size="64" fill={BONE}>"the agent"</text>
                <text x={RING_X} y={RING_Y + 36.0} text-anchor="middle" font-family={MONO} font-weight="500" font-size="18" letter-spacing="3" fill={GREY}>"NEVER WATCHES A VIDEO"</text>
            </g>
        </g>
    )
}

/// A terminal card: the command, then its output lines or image.
fn card(ctx: &FFramesContext, card: &Card, l: f32, leaving: bool) -> Svgr<'static> {
    let (x, y, w) = (150.0, 440.0, 800.0);
    let enter = snap(l);
    let exit = if leaving { 1.0 } else { 0.0 };
    let typed = ((l / 0.5) * card.command.len() as f32).min(card.command.len() as f32) as usize;
    let command: String = card.command.chars().take(typed).collect();
    let body_o = prog(l, 0.45, 0.6);
    let (body, h) = match &card.body {
        Body::Lines(lines) => {
            let rows: Vec<Svgr> = lines
                .iter()
                .enumerate()
                .map(|(i, (text, color))| {
                    let o = prog(l, 0.5 + i as f32 * 0.08, 0.55 + i as f32 * 0.08);
                    fframes::svgr!(<text x={x + 28.0} y={y + 110.0 + i as f32 * 36.0} font-family={MONO} font-weight="500" font-size="23" fill={*color} opacity={o}>{*text}</text>)
                })
                .collect();
            (fframes::svgr!(<g>{rows}</g>), 90.0 + lines.len() as f32 * 36.0 + 20.0)
        }
        Body::Image(name, aspect) => {
            let iw = w - 56.0;
            let ih = iw / aspect;
            let img = ctx
                .get_image(name)
                .map(|img| fframes::svgr!(<image href={img.href()} x={x + 28.0} y={y + 84.0} width={iw} height={ih} />))
                .unwrap_or_default();
            (fframes::svgr!(<g opacity={body_o}>{img}</g>), 84.0 + ih + 28.0)
        }
    };
    fframes::svgr!(
        <g opacity={prog(l, 0.0, 0.08) * (1.0 - exit * 0.0)} transform={format!("translate({} 0)", (1.0 - enter) * -80.0)}>
            <rect x={x} y={y} width={w} height={h} fill="#0e0d0c" fill-opacity="0.92" stroke="#3d3a35" stroke-width="1.5" />
            <rect x={x} y={y} width="4" height={h} fill={ORANGE} />
            <text x={x + 28.0} y={y + 52.0} font-family={MONO} font-weight="600" font-size="25" fill={ORANGE}>"$"</text>
            <text x={x + 58.0} y={y + 52.0} font-family={MONO} font-weight="500" font-size="25" fill={BONE}>{command}</text>
            <rect x={x + 28.0} y={y + 70.0} width={w - 56.0} height="1" fill="#3d3a35" />
            {body}
        </g>
    )
}

/// THIS VIDEO WAS / MADE BY AN AGENT.
fn confession(l: f32) -> Svgr<'static> {
    let c = prog(l, 3.0, 3.4);
    let collapse = expo_in(prog(l, 7.3, 8.0));
    let s = 1.0 - collapse * 0.15;
    fframes::svgr!(
        <g opacity={1.0 - collapse} transform={format!("translate(960 540) scale({s}) translate(-960 -540)")}>
            {Slam::new(150.0, 470.0, "THIS VIDEO WAS", DISPLAY, 138.0, BONE).draw(l, 0.0, 160.0)}
            {Slam::new(150.0, 630.0, "MADE BY AN AGENT.", DISPLAY, 138.0, ORANGE).draw(l - 1.0, 0.0, 160.0)}
            <text x="156" y="712" font-family={SERIF} font-style="italic" font-size="54" fill={BONE} opacity={c}>"in Rust, with fframes. No timeline editor was opened."</text>
        </g>
    )
}

/// Before the first command: the prompt from the cold open, again.
fn prompt_card(l: f32) -> Svgr<'static> {
    if l < 0.0 {
        return Svgr::empty();
    }
    let text = crate::scenes::prompt::PROMPT;
    let typed = ((l / 3.0) * text.len() as f32).min(text.len() as f32) as usize;
    let shown: String = text.chars().take(typed).collect();
    let (first, second) = if shown.len() > 33 { (shown[..33].to_owned(), shown[33..].to_owned()) } else { (shown, String::new()) };
    let enter = snap(l);
    fframes::svgr!(
        <g opacity={prog(l, 0.0, 0.1)} transform={format!("translate({} 0)", (1.0 - enter) * -80.0)}>
            <text x="150" y="480" font-family={MONO} font-weight="500" font-size="20" letter-spacing="3" fill={GREY}>"IT STARTS WITH A PROMPT"</text>
            <rect x="150" y="510" width="800" height="150" fill="#0e0d0c" fill-opacity="0.92" stroke={ORANGE} stroke-width="2" />
            <text x="180" y="572" font-family={MONO} font-weight="600" font-size="32" fill={ORANGE}>"›"</text>
            <text x="220" y="572" font-family={MONO} font-weight="500" font-size="32" fill={BONE}>{first}</text>
            <text x="220" y="620" font-family={MONO} font-weight="500" font-size="32" fill={BONE}>{second}</text>
        </g>
    )
}
