//! "One shot": a 27 s, 1440×1080 fframes promo in the made-of-motion idiom (paper,
//! oversized type, thermal footage, a nervous pen) with new footage, new ink and a
//! new edit. Cuts sit on the 120 BPM soundtrack: the gun fires on its drop.

mod author;
mod create;
mod gun;
mod ink;
mod logo_points;
mod patterns;
mod question;
mod slop;
mod tool;
mod tool_sizes;

use std::sync::{Arc, OnceLock};

use fframes::{
    AudioMap, AudioTimestamp, AudioTrack, Color, Duration, FFramesContext, FontQuery, Frame, Scene,
    Scenes, Shader, ShaderUniforms, Svgr, Video, include_media_dir,
};

include_media_dir!(pub struct OneShotMedia, "examples/one-shot/media");

pub(crate) const FPS: f32 = 24.;
pub(crate) const INK_TEXT: &str = "#1a1714";

#[derive(Clone, Copy, Debug)]
enum Kind {
    Question,
    Gun,
    Patterns,
    Slop,
    Tool,
    Author,
    Create,
}

// Scene ends in frames (exclusive). Every cut is on a beat; the gun fires on the
// drop at frame 147.
const EDIT: [(&str, usize, Kind); 7] = [
    ("Question", 135, Kind::Question),
    ("Gun", 195, Kind::Gun),
    ("Patterns", 267, Kind::Patterns),
    ("Slop", 339, Kind::Slop),
    ("Tool", 387, Kind::Tool),
    ("Author", 459, Kind::Author),
    ("Create", 648, Kind::Create),
];

/// A heat-field atlas made by tools/prep.py: three frames per RGB cell.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Atlas {
    pub file: &'static str,
    pub frames: usize,
    pub cell: [f32; 2],
    pub cols: usize,
}

pub(crate) const GUN: Atlas = Atlas {
    file: "gun-heat.png",
    frames: 40,
    cell: [700., 480.],
    cols: 4,
};
pub(crate) const GIRL: Atlas = Atlas {
    file: "girl-heat.png",
    frames: 42,
    cell: [707., 576.],
    cols: 4,
};
pub(crate) const REACH: Atlas = Atlas {
    file: "reach-heat.png",
    frames: 60,
    cell: [1126., 594.],
    cols: 4,
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Palette {
    Thermal = 0,
    Yellow = 1,
    Orange = 2,
    Echo = 3,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HeatDraw {
    pub index: usize,
    pub palette: Palette,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub gain: f32,
    pub glow: f32,
    pub opacity: f32,
    pub hot: f32,
    pub hot_pos: [f32; 2],
}

#[derive(Debug)]
pub(crate) struct Studio {
    paper: Shader,
    heat: Shader,
    pub muzzle: Shader,
    pub molten: Shader,
    pub pixel: Shader,
}

impl Studio {
    /// Paper (dark = 0) or warm black (dark = 1). `uClock` keeps drifting across cuts.
    pub(crate) fn paper(&self, frame: &Frame, dark: f32, vignette: f32) -> Svgr<'static> {
        let layer = self.paper.draw(
            frame,
            ShaderUniforms::new()
                .float("uDark", dark)
                .float("uVignette", vignette)
                .float("uClock", frame.global_index as f32 / FPS),
        );
        fframes::svgr!(<image href={layer.href()} x="-40" y="-40" width="1520" height="1160" />)
    }

    pub(crate) fn heat(
        &self,
        frame: &Frame,
        ctx: &FFramesContext<'_, '_>,
        atlas: Atlas,
        d: HeatDraw,
    ) -> Svgr<'static> {
        let Some(image) = ctx.get_image(atlas.file) else {
            return Svgr::empty();
        };
        let i = d.index.min(atlas.frames - 1);
        let cell = i / 3;
        let chan = i % 3;
        let [cw, ch] = atlas.cell;
        let layer = self.heat.draw(
            frame,
            ShaderUniforms::new()
                .image("uAtlas", image)
                .float2(
                    "uCell",
                    (cell % atlas.cols) as f32 * cw,
                    (cell / atlas.cols) as f32 * ch,
                )
                .float2("uCellSize", cw, ch)
                .float3(
                    "uChan",
                    f32::from(chan == 0),
                    f32::from(chan == 1),
                    f32::from(chan == 2),
                )
                .float("uPalette", d.palette as i32 as f32)
                .float("uGain", d.gain)
                .float("uGlow", d.glow)
                .float("uOpacity", d.opacity)
                .float("uHot", d.hot)
                .float2("uHotPos", d.hot_pos[0], d.hot_pos[1]),
        );
        fframes::svgr!(<image href={layer.href()} x={d.x} y={d.y} width={cw * d.scale} height={ch * d.scale} />)
    }
}

/// Specks of dust drifting over the paper (or embers of soot over black).
pub(crate) fn dust(frame: &Frame, dark: bool) -> Svgr<'static> {
    let t = frame.global_index as f32 / FPS;
    let fill = if dark { "#d9cfc0" } else { "#2c2420" };
    let specks = (0..18)
        .map(|i| {
            let k = i as f32;
            let x = 40. + (k * 431.7 + t * (9. + k * 1.3)).rem_euclid(1360.);
            let y = 40. + (k * 197.3 - t * (6. + k * 1.7)).rem_euclid(1000.);
            let r = if i % 4 == 0 { 1.8 } else { 0.9 };
            fframes::svgr!(<circle cx={x} cy={y} r={r} fill={fill} opacity="0.35" />)
        })
        .collect::<Vec<_>>();
    fframes::svgr!(<g>{specks}</g>)
}

/// Printer's crop marks and a registration target, faint, on the paper scenes.
pub(crate) fn crop_marks() -> Svgr<'static> {
    fframes::svgr!(<g stroke="#3d2419" stroke-width="1.5" fill="none" opacity="0.32">
        <path d="M28 52 H52 V28 M1412 52 H1388 V28 M28 1028 H52 V1052 M1412 1028 H1388 V1052" />
        <circle cx="720" cy="34" r="8" />
        <path d="M708 34 H732 M720 22 V46" />
        <circle cx="720" cy="1046" r="8" />
        <path d="M708 1046 H732 M720 1034 V1058" />
    </g>)
}

/// Bold display copy, the voice of the film.
pub(crate) fn copy(text: &str, x: f32, y: f32, size: f32, color: &str) -> Svgr<'static> {
    fframes::svgr!(<text x={x} y={y} font-family="Inter 24pt" font-weight="700" font-size={size}
        letter-spacing={-size * 0.035} fill={color.to_owned()}>{text.to_owned()}</text>)
}

/// Small technical annotations.
pub(crate) fn mono(
    text: &str,
    x: f32,
    y: f32,
    size: f32,
    color: &str,
    opacity: f32,
) -> Svgr<'static> {
    fframes::svgr!(<text x={x} y={y} font-family="JetBrains Mono" font-size={size}
        fill={color.to_owned()} opacity={opacity}>{text.to_owned()}</text>)
}

/// Width of bold copy as drawn by `copy`, with a proportional fallback.
pub(crate) fn measure<'a>(
    frame: &mut Frame,
    ctx: &FFramesContext<'a, '_>,
    text: &'a str,
    size: f32,
) -> f32 {
    let font = FontQuery {
        family: "Inter 24pt",
        size: size.round() as usize,
        weight: 700,
        ..Default::default()
    };
    let spacing = size * 0.035 * text.chars().count().saturating_sub(1) as f32;
    frame
        .text_width(ctx, font, text)
        .map_or(text.chars().count() as f32 * size * 0.56, |w| {
            w as f32 * size / size.round()
        })
        - spacing
}

/// Left x of every word of `words` laid out from x = 0 at `size`, plus each width.
pub(crate) fn layout<'a>(
    frame: &mut Frame,
    ctx: &FFramesContext<'a, '_>,
    words: &[&'a str],
    size: f32,
) -> Vec<(f32, f32)> {
    let space = measure(frame, ctx, "a b", size) - measure(frame, ctx, "ab", size);
    let mut x = 0.;
    words
        .iter()
        .map(|w| {
            let width = measure(frame, ctx, w, size);
            let out = (x, width);
            x += width + space;
            out
        })
        .collect()
}

pub(crate) fn memo<T: Clone>(cell: &OnceLock<T>, f: impl FnOnce() -> T) -> T {
    cell.get_or_init(f).clone()
}

/// Deterministic camera shake that decays over `len` seconds after `at`.
pub(crate) fn shake(t: f32, at: f32, len: f32, amp: f32, seed: f32) -> (f32, f32) {
    let age = t - at;
    if !(0.0..len).contains(&age) {
        return (0., 0.);
    }
    let k = (1. - age / len).powi(2) * amp;
    let n = (age * FPS).floor();
    (
        (ink::hash(n + seed) - 0.5) * 2. * k,
        (ink::hash(n + seed + 31.) - 0.5) * 2. * k,
    )
}

#[derive(Debug)]
struct Shot {
    name: &'static str,
    length: usize,
    kind: Kind,
    studio: Arc<Studio>,
    question: Arc<question::Question>,
    gun: Arc<gun::Gun>,
    patterns: Arc<patterns::Patterns>,
    slop: Arc<slop::Slop>,
    tool: Arc<tool::Tool>,
    author: Arc<author::Author>,
    create: Arc<create::Create>,
}

impl Scene for Shot {
    fn name(&self) -> &'static str {
        self.name
    }

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(self.length)
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let s = &self.studio;
        let dark = matches!(self.kind, Kind::Gun | Kind::Patterns | Kind::Author);
        let specks = dust(&frame, dark);
        let marks = if dark { Svgr::empty() } else { crop_marks() };
        let content = match self.kind {
            Kind::Question => self.question.render(frame, ctx, s),
            Kind::Gun => self.gun.render(frame, ctx, s),
            Kind::Patterns => self.patterns.render(frame, ctx, s),
            Kind::Slop => self.slop.render(frame, ctx, s),
            Kind::Tool => self.tool.render(frame, ctx, s),
            Kind::Author => self.author.render(frame, ctx, s),
            Kind::Create => self.create.render(frame, ctx, s),
        };
        fframes::svgr!(<g>{content}{specks}{marks}</g>)
    }
}

#[derive(Debug)]
pub struct OneShot {
    shots: Vec<Shot>,
}

impl OneShot {
    pub fn new() -> Self {
        let studio = Arc::new(Studio {
            paper: Shader::sksl(include_str!("shaders/paper.sksl")),
            heat: Shader::sksl(include_str!("shaders/heat.sksl")),
            muzzle: Shader::sksl(include_str!("shaders/muzzle.sksl")),
            molten: Shader::sksl(include_str!("shaders/molten.sksl")),
            pixel: Shader::sksl(include_str!("shaders/pixel.sksl")),
        });
        let question = Arc::new(question::Question::new());
        let gun = Arc::new(gun::Gun::new());
        let patterns = Arc::new(patterns::Patterns::new());
        let slop = Arc::new(slop::Slop::new());
        let tool = Arc::new(tool::Tool::new());
        let author = Arc::new(author::Author::new());
        let create = Arc::new(create::Create::new());
        let mut start = 0;
        let shots = EDIT
            .into_iter()
            .map(|(name, end, kind)| {
                let length = end - start;
                start = end;
                Shot {
                    name,
                    length,
                    kind,
                    studio: Arc::clone(&studio),
                    question: Arc::clone(&question),
                    gun: Arc::clone(&gun),
                    patterns: Arc::clone(&patterns),
                    slop: Arc::clone(&slop),
                    tool: Arc::clone(&tool),
                    author: Arc::clone(&author),
                    create: Arc::clone(&create),
                }
            })
            .collect();
        Self { shots }
    }
}

impl Default for OneShot {
    fn default() -> Self {
        Self::new()
    }
}

/// Sound effects, placed in seconds on the global timeline.
const SFX: &[(&str, f32, f32)] = &[
    // opening: brush, flash, whip pans, selection box, collapse, typing
    ("sfx-scribble.wav", 0.0, -12.),
    ("sfx-pop-hard.wav", 0.167, -15.),
    ("sfx-sweep.wav", 0.2, -14.),
    ("sfx-sweep-small.wav", 0.54, -15.),
    ("sfx-pop-hard.wav", 0.58, -17.),
    ("sfx-slice.wav", 0.96, -13.),
    ("sfx-sweep-small.wav", 1.12, -17.),
    ("sfx-type.wav", 1.33, -10.),
    ("sfx-type.wav", 1.98, -11.),
    ("sfx-pencil.wav", 2.75, -9.),
    ("sfx-sweep.wav", 3.08, -16.),
    ("sfx-sparkle.wav", 3.42, -15.),
    ("sfx-pencil.wav", 4.83, -10.),
    ("sfx-sweep-small.wav", 5.29, -13.),
    // gun: it fires on the drop
    ("sfx-gun-move.wav", 5.62, -10.),
    ("sfx-scribble.wav", 5.72, -17.),
    ("sfx-arrow.wav", 6.1, -10.),
    ("sfx-gunshot.wav", 6.13, -4.),
    ("sfx-impact.wav", 6.13, -9.),
    ("sfx-pencil.wav", 6.5, -10.),
    // patterns
    ("sfx-run.wav", 8.13, -12.),
    ("sfx-pop.wav", 9.96, -15.),
    ("sfx-pop.wav", 10.13, -16.),
    ("sfx-pencil.wav", 10.29, -10.),
    // slop
    ("sfx-squelch.wav", 11.13, -11.),
    ("sfx-splat.wav", 11.63, -8.),
    ("sfx-splat-small.wav", 12.13, -10.),
    ("sfx-scribble.wav", 12.29, -11.),
    ("sfx-splat-small.wav", 12.63, -12.),
    ("sfx-splat-small.wav", 13.13, -15.),
    // tool
    ("sfx-swirl.wav", 13.95, -10.),
    ("sfx-pencil.wav", 14.63, -10.),
    ("sfx-scribble.wav", 14.8, -14.),
    // author
    ("sfx-riser.wav", 16.13, -13.),
    ("sfx-pencil.wav", 17.13, -10.),
    ("sfx-pencil.wav", 17.8, -12.),
    // create
    ("sfx-scribble.wav", 19.13, -14.),
    ("sfx-pop.wav", 19.13, -13.),
    ("sfx-pop.wav", 19.25, -14.),
    ("sfx-sweep.wav", 20.0, -13.),
    ("sfx-pops.wav", 20.13, -12.),
    ("sfx-sparkle.wav", 20.6, -16.),
    ("sfx-swirl.wav", 22.0, -12.),
    ("sfx-logo.wav", 22.9, -11.),
    ("sfx-pencil.wav", 23.29, -11.),
];

impl Video for OneShot {
    const FPS: usize = 24;
    const WIDTH: usize = 1440;
    const HEIGHT: usize = 1080;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(648)
    }

    fn define_scenes(&self) -> Scenes<'_> {
        Scenes::from(
            self.shots
                .iter()
                .map(|s| s as &dyn Scene)
                .collect::<Vec<_>>(),
        )
    }

    fn audio(&self) -> AudioMap<'_> {
        let mut tracks = vec![
            AudioTrack::new(
                "soundtrack.wav",
                AudioTimestamp::Second(0.)..AudioTimestamp::Eof,
            )
            .gain_db(-5.),
        ];
        tracks.extend(SFX.iter().map(|&(file, at, gain)| {
            AudioTrack::new(file, AudioTimestamp::Second(at)..AudioTimestamp::Eof).gain_db(gain)
        }));
        AudioMap::from(tracks)
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        fframes::svgr!(<svg xmlns="http://www.w3.org/2000/svg" width="1440" height="1080" viewBox="0 0 1440 1080">
            {ctx.render_scenes(&frame)}
        </svg>)
    }
}
