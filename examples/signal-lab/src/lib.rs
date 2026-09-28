//! A 24-second motion study: product UI, data storytelling, and a technical explainer.
use std::sync::LazyLock;

use fframes::{
    AnimateRuntimeInput, AudioMap, AudioTimestamp, AudioTrack, Color, Duration, FFramesContext,
    FontQuery, Frame, Scene, Scenes, Svgr, TextOverflow, Transform, Video,
    animation::{AnimationRuntime, Easing},
    include_media_dir,
};

include_media_dir!(pub struct SignalLabMedia, "examples/signal-lab/media");

pub const WIDTH: usize = 1920;
pub const HEIGHT: usize = 1080;
pub const SCENE_SECONDS: f32 = 6.0;
const PAPER: &str = "#eeeae2";
const INK: &str = "#152c2b";
const MUTED: &str = "#53645e";
const LIME: &str = "#d2f87a";
const FONT: &str = "DM Sans";
static EASE: LazyLock<AnimationRuntime> =
    LazyLock::new(|| AnimationRuntime::new(0.8, &Easing::CubicBezier(0.16, 1.0, 0.3, 1.0)));
static SPRING: LazyLock<AnimationRuntime> = LazyLock::new(|| {
    AnimationRuntime::new(
        3.0,
        &Easing::Spring {
            mass: 1.0,
            stiffness: 180.0,
            damping: 20.0,
        },
    )
});

/// Four motion studies with an embedded synthesized soundtrack.
#[derive(Debug)]
pub struct SignalLabVideo;

impl Video for SignalLabVideo {
    const FPS: usize = 30;
    const WIDTH: usize = WIDTH;
    const HEIGHT: usize = HEIGHT;
    const BACKGROUND_COLOR: Color = Color::WHITE;

    fn duration(&self) -> Duration<'_> {
        Duration::Auto
    }
    fn audio(&self) -> AudioMap<'_> {
        AudioMap::from([AudioTrack::new(
            "pulse.wav",
            AudioTimestamp::Second(0.0)..AudioTimestamp::Eof,
        )
        .gain_db(6.5)
        .fade_in(0.08)
        .fade_out(0.6)])
    }
    fn define_scenes(&self) -> Scenes<'_> {
        Scenes::from(vec![
            &ProductScene as &dyn Scene,
            &DataScene,
            &SystemScene,
            &FinalScene,
        ])
    }
    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let progress = (frame.seconds() / 24.0 * 1536.0).max(0.5);
        fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080">
                <rect width="1920" height="1080" fill={PAPER} />
                {ctx.render_scenes(&frame)}
                <path d="M192 946 H1728" stroke="#b4bdb1" stroke-width="2" />
                <rect x="192" y="945" width={progress} height="3" fill="#709542" />
                <text x="192" y="1002" font-family="DM Sans" font-weight="500" font-size="28" fill={MUTED}>"FFRAMES / MOTION STUDIES"</text>
                <text x="1728" y="1002" text-anchor="end" font-family="DM Sans" font-weight="500" font-size="28" fill={MUTED}>"24 SECONDS / SVG + RUST"</text>
            </svg>
        )
    }
}

fn ramp(frame: &Frame, start: f32) -> f32 {
    frame.animate_runtime(AnimateRuntimeInput {
        on_second: start,
        from: 0.0,
        to: 1.0,
        animation_runtime: &EASE,
    })
}
fn rise(frame: &Frame, start: f32) -> f32 {
    frame.animate_runtime(AnimateRuntimeInput {
        on_second: start,
        from: 56.0,
        to: 0.0,
        animation_runtime: &SPRING,
    })
}
fn label<'a>(number: &'a str, title: &'a str) -> Svgr<'a> {
    fframes::svgr!(<g font-family="DM Sans" font-size="30" font-weight="500" fill={INK}>
        <circle cx="209" cy="147" r="17" fill="#709542" />
        <text x="252" y="158">{title}</text>
        <text x="1728" y="158" text-anchor="end">{number}</text>
    </g>)
}
fn heading<'a>(frame: &mut Frame, ctx: &FFramesContext<'a, '_>, text: &'a str) -> Svgr<'a> {
    let text = frame
        .text_fit(
            ctx,
            FontQuery {
                family: FONT,
                size: 108,
                weight: 500,
                ..Default::default()
            },
            text,
            1536,
            TextOverflow::Ellipsis,
        )
        .unwrap_or_else(|| text.into())
        .into_owned();
    fframes::svgr!(<text x="192" y="314" font-family="DM Sans" font-weight="500" font-size="108" letter-spacing="-4" fill={INK}>{text}</text>)
}

#[derive(Debug)]
struct ProductScene;
impl Scene for ProductScene {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SCENE_SECONDS)
    }
    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let rows: Vec<_> = [("Build", "Ready"), ("Review", "Ready"), ("Publish", "Ready")].into_iter().enumerate().map(|(i, (name, status))| {
            let y = 470 + i * 105;
            let p = ramp(&frame, 0.7 + i as f32 * 0.12);
            fframes::svgr!(<g opacity={p} transform={Transform::translate(0, rise(&frame, 0.7 + i as f32 * 0.12))}>
                <rect x="1088" y={y - 46} width="540" height="82" rx="14" fill="#29403d" />
                <text x="1124" y={y + 8} font-size="38" fill={PAPER}>{name}</text>
                <text x="1586" y={y + 8} text-anchor="end" font-size="30" fill={LIME}>{status}</text>
            </g>)
        }).collect();
        let angle = frame.seconds() * 8.0;
        fframes::svgr!(<g font-family="DM Sans" font-weight="500">
            {label("01 / 04", "PRODUCT ANNOUNCEMENT")}
            <text x="183" y="445" font-size="208" letter-spacing="-10" fill={INK}>"Signal."</text>
            <text x="192" y="543" font-size="52" fill={INK}>"Your next release,"</text>
            <text x="192" y="610" font-size="52" fill={INK}>"in motion."</text>
            <g opacity={ramp(&frame, 0.4)}>
                <rect x="192" y="703" width="380" height="76" rx="38" fill={INK} />
                <text x="382" y="753" text-anchor="middle" font-size="32" fill={LIME}>"A FICTIONAL PRODUCT"</text>
            </g>
            <g transform={Transform::translate(0, rise(&frame, 0.0))}>
                <rect x="1000" y="262" width="728" height="576" rx="32" fill={INK} />
                <circle cx="1095" cy="352" r="14" fill={LIME} />
                <text x="1138" y="365" font-size="38" fill={PAPER}>"Release / 01"</text>
                {rows}
                <g transform={format!("translate(1650 278) rotate({angle})")}>
                    <circle r="73" fill={LIME} />
                    <path d="M-34 0 H34 M0 -34 V34 M-24 -24 L24 24 M-24 24 L24 -24" stroke={INK} stroke-width="7" />
                </g>
            </g>
        </g>)
    }
}

#[derive(Debug)]
struct DataScene;
impl Scene for DataScene {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SCENE_SECONDS)
    }
    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let bars: Vec<_> = [("01", 24.0), ("02", 41.0), ("03", 58.0), ("04", 76.0)].into_iter().enumerate().map(|(i, (name, value))| {
            let p = ramp(&frame, 0.25 + i as f32 * 0.1);
            let h = (value * 4.3 * p).max(0.5);
            let x = 982 + i * 188;
            fframes::svgr!(<g>
                <rect x={x} y={798.0 - h} width="126" height={h} rx="8" fill={if i == 3 { "#709542" } else { "#b4bdb1" }} />
                <text x={x + 63} y={775.0 - h} text-anchor="middle" font-size="36" fill={INK}>{format!("{:.0}", value * p)}</text>
                <text x={x + 63} y="851" text-anchor="middle" font-size="30" fill={MUTED}>{name}</text>
            </g>)
        }).collect();
        let count = 76.0 * ramp(&frame, 0.55);
        fframes::svgr!(<g font-family="DM Sans" font-weight="500">
            {label("02 / 04", "DATA STORY")}
            {heading(&mut frame, ctx, "Give numbers a rhythm.")}
            <text x="180" y="646" font-size="248" letter-spacing="-12" fill={INK}>{format!("{count:.0}")}</text>
            <text x="192" y="734" font-size="46" fill={INK}>"Weekly activations"</text>
            <text x="192" y="802" font-size="30" fill={MUTED}>"ILLUSTRATIVE DATA / WEEKS 01–04"</text>
            {bars}
        </g>)
    }
}

#[derive(Debug)]
struct SystemScene;
impl Scene for SystemScene {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SCENE_SECONDS)
    }
    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let cards: Vec<_> = [("01", "Describe", "Rust + SVG"), ("02", "Inspect", "Frames + sound"), ("03", "Render", "Video file")].into_iter().enumerate().map(|(i, (n, title, subtitle))| {
            let x = 192 + i * 548;
            fframes::svgr!(<g opacity={ramp(&frame, 0.12 + i as f32 * 0.1)} transform={Transform::translate(0, rise(&frame, 0.12 + i as f32 * 0.1))}>
                <rect x={x} y="440" width="440" height="302" rx="24" fill={INK} />
                <text x={x + 36} y="505" font-size="30" fill={LIME}>{n}</text>
                <text x={x + 36} y="612" font-size="64" fill={PAPER}>{title}</text>
                <text x={x + 36} y="684" font-size="32" fill="#b4bdb1">{subtitle}</text>
            </g>)
        }).collect();
        let travel = ((frame.seconds() - 1.2).max(0.0) / 2.8).min(1.0);
        let x = 1508.0 - 1096.0 * travel;
        fframes::svgr!(<g font-family="DM Sans" font-weight="500">
            {label("03 / 04", "TECHNICAL EXPLAINER")}
            {heading(&mut frame, ctx, "Make the process visible.")}
            <g opacity={ramp(&frame, 0.4)}>
                <path d="M632 591 H740 M1180 591 H1288" stroke="#709542" stroke-width="5" />
                <path d="M718 577 L740 591 L718 605 M1266 577 L1288 591 L1266 605" stroke="#709542" stroke-width="5" fill="none" />
            </g>
            {cards}
            <g opacity={ramp(&frame, 0.65)}>
                <path d="M412 780 V824 H1508 V780" stroke="#b4bdb1" stroke-width="3" fill="none" />
                <circle cx={x} cy="824" r="12" fill="#709542" />
                <text x="960" y="894" text-anchor="middle" font-size="30" fill={MUTED}>"REVISE AND REPEAT"</text>
            </g>
        </g>)
    }
}

#[derive(Debug)]
struct FinalScene;
impl Scene for FinalScene {
    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(SCENE_SECONDS)
    }
    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let p = ramp(&frame, 0.2);
        let shift = rise(&frame, 0.2);
        fframes::svgr!(<g font-family="DM Sans" font-weight="500">
            {label("04 / 04", "CREATED WITH FFRAMES")}
            <g transform={Transform::translate(0, shift)}>
                <text x="183" y="457" font-size="146" letter-spacing="-6" fill={INK}>"Built from code."</text>
                <text x="183" y="631" font-size="146" letter-spacing="-6" fill={INK}>"Ready to revise."</text>
            </g>
            <g opacity={p}>
                <rect x="192" y="750" width="1536" height="90" rx="16" fill={INK} />
                <text x="240" y="809" font-size="34" fill={PAPER}>"PRODUCT DEMOS"</text>
                <circle cx="664" cy="797" r="7" fill={LIME} />
                <text x="716" y="809" font-size="34" fill={PAPER}>"DATA STORIES"</text>
                <circle cx="1100" cy="797" r="7" fill={LIME} />
                <text x="1152" y="809" font-size="34" fill={PAPER}>"VISUAL EXPLAINERS"</text>
            </g>
        </g>)
    }
}
