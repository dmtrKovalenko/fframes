//! An 18 second vertical launch video for Lottie support in fframes.
//!
//! All timing uses the beat of the whole video (`global_beat`). The scenes overlap for their
//! cross-fades, and this keeps the overlaps on the beat.

pub mod beat;
pub mod soundtrack;

use std::f32::consts::TAU;
use std::sync::LazyLock;

use fframes::animation::{AnimationRuntime, Easing};
use fframes::{
    AnimateRuntimeInput, AudioMap, AudioTimestamp::*, AudioTrack, Color, Duration, FFramesContext,
    FontQuery, FontStyle, Frame, LottieOptions, Overlap, Scene, Scenes, Svgr, Video,
    include_media_dir, svgr,
};

use beat::*;

include_media_dir!(pub struct LaunchMedia, "examples/lottie/media");

pub const WIDTH: usize = 1080;
pub const HEIGHT: usize = 1920;

const BG: &str = "#0b0b0b";
const BONE: &str = "#ece8e1";
const ORANGE: &str = "#fb6a22";
const EMBER: &str = "#8a3712";
const SOOT: &str = "#4a2010";
/// Secondary text: 7.5:1 on the background.
const MUTED: &str = "#a8a39b";
const PEACH: &str = "#ffb37a";
const SAND: &str = "#f4d7b8";

const DISPLAY: &str = "Anton";
const MONO: &str = "IBM Plex Mono";
const SERIF: &str = "Instrument Serif";

/// Phone apps cover the top and the bottom. Content stays inside x 8-92 % and y 12-78 %.
const SAFE_LEFT: f32 = 86.0;
const SAFE_TOP: f32 = 230.0;

// ------------------------------------------------------------------ Motion

/// The soft spring of the fframes design guide.
const SPRING_SOFT: Easing = Easing::Spring {
    mass: 1.0,
    stiffness: 150.0,
    damping: 18.0,
};

static ENTER: LazyLock<AnimationRuntime> =
    LazyLock::new(|| AnimationRuntime::new(3.0, &SPRING_SOFT));
static FADE: LazyLock<AnimationRuntime> =
    LazyLock::new(|| AnimationRuntime::new(0.3, &Easing::EaseOut));
static EXIT: LazyLock<AnimationRuntime> =
    LazyLock::new(|| AnimationRuntime::new(0.2, &Easing::EaseIn));

/// Progress 0 -> 1 of `animation` that starts at beat `at` of the video.
fn progress(frame: &Frame, animation: &AnimationRuntime, at: f32) -> f32 {
    frame.animate_runtime(AnimateRuntimeInput {
        on_second: beat_time(at) - scene_start(frame),
        from: 0.0_f32,
        to: 1.0,
        animation_runtime: animation,
    })
}

/// An element that enters at beat `at` of the video (rises 60 px, scales up from 0.92 and
/// fades in) and leaves at beat `out` (rises 60 px more and fades out in 0.2 s). (`x`, `y`)
/// is its center.
fn enter_exit(
    frame: &Frame,
    at: f32,
    out: f32,
    (x, y): (f32, f32),
    content: Svgr<'static>,
) -> Svgr<'static> {
    let enter = progress(frame, &ENTER, at);
    let exit = progress(frame, &EXIT, out);
    let opacity = progress(frame, &FADE, at) * (1.0 - exit);
    if opacity <= 0.0 {
        return Svgr::empty();
    }
    let dy = (1.0 - enter) * 60.0 - exit * 60.0;
    let scale = 0.92 + 0.08 * enter;
    let transform = format!(
        "translate({x} {}) scale({scale}) translate({} {})",
        y + dy,
        -x,
        -y
    );
    svgr!(<g transform={transform} opacity={opacity}>{content}</g>)
}

/// The second of the video when the scene of `frame` starts, with its overlap. `play_lottie`
/// counts from this second.
fn scene_start(frame: &Frame) -> f32 {
    frame.frame_to_second(frame.global_index - frame.index)
}

/// A Lottie file in a square box that starts playing at beat `at`.
fn lottie(
    frame: &Frame,
    ctx: &FFramesContext,
    file: &str,
    (x, y, size): (f32, f32, f32),
    at: f32,
) -> Svgr<'static> {
    frame.play_lottie(
        ctx,
        file,
        LottieOptions {
            start: beat_time(at) - scene_start(frame),
            looping: true,
            x,
            y,
            width: Some(size),
            height: Some(size),
            ..Default::default()
        },
    )
}

/// Width of a text in pixels. Without the font it gives an estimate.
fn text_width(
    frame: &mut Frame,
    ctx: &FFramesContext,
    query: FontQuery,
    text: &'static str,
) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    frame
        .text_width(ctx, query, text)
        .map_or(text.len() as f32 * query.size as f32 * 0.5, |w| w as f32)
}

// ------------------------------------------------------------------ Scenes

/// The beginnings of "LOTTIE". Each letter goes after the measured width of the letters before it.
const PREFIXES: [&str; 7] = ["", "L", "LO", "LOT", "LOTT", "LOTTI", "LOTTIE"];

/// The first frame shows the wordmark and the party popper. "NOW PLAYS LOTTIE" is complete at 1.3 s.
#[derive(Debug)]
pub struct Hook;

impl Scene for Hook {
    fn duration(&self) -> Duration<'_> {
        Duration::Frames(beat_frames(GRID - HOOK))
    }

    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let out = GRID - 0.5;

        let mark = wordmark(&mut frame, ctx, 540.0, 330.0, 96, 1.0);
        let popper = lottie(&frame, ctx, "party.json", (290.0, 930.0, 520.0), HOOK);

        // "NOW PLAYS" is one centered line with 40 px between the words. The letter-spacing takes
        // 2 px per letter, and the measured width does not include it.
        let word_query = FontQuery {
            family: DISPLAY,
            size: 170,
            ..Default::default()
        };
        let now = text_width(&mut frame, ctx, word_query, "NOW") - 2.0 * 3.0;
        let plays = text_width(&mut frame, ctx, word_query, "PLAYS") - 2.0 * 5.0;
        let left = 540.0 - (now + 40.0 + plays) / 2.0;
        let words: Vec<Svgr> = [
            ("NOW", WORD_NOW, left, now),
            ("PLAYS", WORD_PLAYS, left + now + 40.0, plays),
        ]
        .iter()
        .map(|(word, at, x, width)| {
            enter_exit(&frame, *at, out, (x + width / 2.0, 520.0), svgr!(
                <text x={*x} y="580" font-family={DISPLAY} font-size="170" letter-spacing="-2" fill={BONE}>{*word}</text>
            ))
        })
        .collect();

        let query = FontQuery {
            family: DISPLAY,
            size: 300,
            ..Default::default()
        };
        let total = text_width(&mut frame, ctx, query, "LOTTIE");
        let letters: Vec<Svgr> = "LOTTIE"
            .chars()
            .enumerate()
            .map(|(i, letter)| {
                let x = 540.0 - total / 2.0 + text_width(&mut frame, ctx, query, PREFIXES[i]);
                let letter = letter.to_string();
                enter_exit(&frame, LETTERS + i as f32 * LETTER_STEP, out, (x, 760.0), svgr!(
                    <text x={x} y="860" font-family={DISPLAY} font-size="300" fill={ORANGE}>{letter}</text>
                ))
            })
            .collect();

        svgr!(
            <g>
                {enter_exit(&frame, HOOK - 1.0, out, (540.0, 300.0), mark)}
                {words}
                {letters}
                {enter_exit(&frame, HOOK - 1.0, out, (550.0, 1190.0), popper)}
            </g>
        )
    }
}

/// Twelve animations pop in, one per sixteenth note, and float.
#[derive(Debug)]
pub struct Grid;

const TILE_FILES: [&str; TILES] = [
    "fire.json",
    "rocket.json",
    "unicorn.json",
    "earth.json",
    "cool.json",
    "mind_blown.json",
    "switch.json",
    "rainbow.json",
    "cat.json",
    "watermelon.json",
    "popcorn.json",
    "clapper.json",
];

// The grid: 3 x 4 animations between the title and the bottom of the safe area.
const TILE: f32 = 220.0;
const COLUMN: f32 = 300.0;
const ROW: f32 = 255.0;
const GRID_TOP: f32 = 490.0;

impl Scene for Grid {
    fn duration(&self) -> Duration<'_> {
        Duration::Frames(beat_frames(CODE - GRID))
    }

    // the title enters while the hook leaves
    fn overlap(&self) -> Overlap {
        Overlap::Previous(0.3)
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let b = global_beat(&frame) - GRID;
        let out = CODE - 0.5;

        let tiles: Vec<Svgr> = TILE_FILES
            .iter()
            .enumerate()
            .map(|(i, file)| {
                let at = TILE_POP + i as f32 * TILE_STEP;
                let x = 540.0 + ((i % 3) as f32 - 1.0) * COLUMN - TILE / 2.0;
                // a slow float, with a different phase for each tile
                let float = (b * BEAT * TAU / 6.0 + i as f32 * 1.7).sin() * 8.0;
                let y = GRID_TOP + (i / 3) as f32 * ROW + float;
                let animation = lottie(&frame, ctx, file, (x, y, TILE), at);
                enter_exit(&frame, at, out, (x + TILE / 2.0, y + TILE / 2.0), animation)
            })
            .collect();

        svgr!(
            <g>
                {enter_exit(&frame, GRID - 0.4, out, (540.0, 300.0), svgr!(
                    <text x="540" y="340" text-anchor="middle" font-family={DISPLAY} font-size="130" letter-spacing="-2" fill={BONE}>"ANY LOTTIE FILE"</text>
                ))}
                {enter_exit(&frame, GRID + 0.3, out, (540.0, 400.0), svgr!(
                    <text x="540" y="420" text-anchor="middle" font-family={SERIF} font-style="italic" font-size="56" fill={MUTED}>"from After Effects or LottieFiles"</text>
                ))}
                {tiles}
            </g>
        )
    }
}

/// The tiger plays above the code that plays it.
#[derive(Debug)]
pub struct Code;

/// The position of the tiger in the code scene. The zoom starts from this position.
const TIGER_BOX: (f32, f32, f32) = (240.0, SAFE_TOP, 600.0);
/// The tiger loop (5.1 s) starts again when the zoom starts. The zoom freezes that first frame,
/// so the cut does not show.
const TIGER_START: f32 = ZOOM - 5.1 / BEAT;

impl Scene for Code {
    fn duration(&self) -> Duration<'_> {
        Duration::Frames(beat_frames(ZOOM - CODE))
    }

    // the tiger enters while the grid leaves
    fn overlap(&self) -> Overlap {
        Overlap::Previous(0.3)
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let b = global_beat(&frame) - CODE;
        let (x, y, size) = TIGER_BOX;
        let tiger = frame.play_lottie(
            ctx,
            "tiger.json",
            LottieOptions {
                start: beat_time(TIGER_START) - scene_start(&frame),
                looping: true,
                x,
                y,
                width: Some(size),
                ..Default::default()
            },
        );
        // the tiger stays for the zoom, the text leaves before the zoom
        let tiger = enter_exit(
            &frame,
            CODE - 0.4,
            f32::INFINITY,
            (x + size / 2.0, y + size / 2.0),
            tiger,
        );
        let out = ZOOM - 0.5;

        let typed = ((b - (TYPE_START - CODE)) * CHARS_PER_BEAT).max(0.0) as usize;
        svgr!(
            <g>
                {tiger}
                {enter_exit(&frame, CODE, out, (540.0, 1050.0), code_block(typed, b))}
                {enter_exit(&frame, CAPTION, out, (540.0, 1410.0), svgr!(
                    <text x={SAFE_LEFT + 22.0} y="1440" font-family={SERIF} font-style="italic" font-size="96" fill={BONE}>"one call."</text>
                ))}
            </g>
        )
    }
}

/// The code with the first `typed` characters visible and a blinking cursor after them.
fn code_block(typed: usize, beat: f32) -> Svgr<'static> {
    const SIZE: f32 = 40.0;
    // IBM Plex Mono advances 0.6 em per character
    const CHAR: f32 = SIZE * 0.6;
    const X: f32 = SAFE_LEFT + 22.0;
    const Y: f32 = 960.0;
    const LINE: f32 = 64.0;

    let mut left = typed;
    let mut cursor = None;
    let mut lines = Vec::new();
    for (row, line) in CODE_LINES.iter().enumerate() {
        let y = Y + row as f32 * LINE;
        let indent = line
            .first()
            .map_or(0, |(text, _)| text.len() - text.trim_start().len());
        let mut chars_in_line = 0;
        let spans: Vec<Svgr> = line
            .iter()
            .enumerate()
            .filter_map(|(column, (text, token))| {
                // the first token of a line holds the indentation, which is not typed
                let text = if column == 0 { text.trim_start() } else { text };
                if text.is_empty() || left == 0 {
                    return None;
                }
                let shown = &text[..left.min(text.len())];
                left -= shown.len();
                chars_in_line += shown.len();
                let color = match token {
                    Token::Plain => BONE,
                    Token::Punct => MUTED,
                    Token::Method | Token::Key => ORANGE,
                    Token::Str => PEACH,
                    Token::Type => SAND,
                };
                Some(svgr!(<tspan fill={color}>{shown}</tspan>))
            })
            .collect();
        if chars_in_line > 0 {
            let x = X + indent as f32 * CHAR;
            cursor = Some((x + chars_in_line as f32 * CHAR, y));
            lines.push(svgr!(
                <text x={x} y={y} font-family={MONO} font-weight="500" font-size={SIZE}>{spans}</text>
            ));
        }
    }
    let blink = if beat.fract() < 0.5 { 1.0 } else { 0.0 };
    let cursor = cursor.map_or_else(Svgr::empty, |(x, y)| {
        svgr!(<rect x={x + 2.0} y={y - SIZE * 0.8} width={CHAR * 0.9} height={SIZE} fill={ORANGE} opacity={blink} />)
    });
    svgr!(<g>{lines}{cursor}</g>)
}

/// The camera dives into the tiger's eye until the pupil fills the screen.
#[derive(Debug)]
pub struct Zoom;

/// The pupil of the tiger in its own units (the file is 1024 x 1024).
const PUPIL: (f32, f32) = (160.0, 440.0);
/// The pupil covers the whole screen from about 70x. That is just before the outro.
const ZOOM_MAX: f32 = 80.0;

impl Scene for Zoom {
    fn duration(&self) -> Duration<'_> {
        Duration::Frames(beat_frames(OUTRO - ZOOM))
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        static DIVE: LazyLock<AnimationRuntime> =
            LazyLock::new(|| AnimationRuntime::new((OUTRO - ZOOM) * BEAT, &Easing::EaseInOut));

        let (x, y, size) = TIGER_BOX;
        let base = size / 1024.0;
        let pupil = (x + PUPIL.0 * base, y + PUPIL.1 * base);

        let dive = progress(&frame, &DIVE, ZOOM);
        let zoom = ZOOM_MAX.powf(dive);
        // the pupil travels to the middle of the safe area while the camera dives
        let target = (
            540.0 + (pupil.0 - 540.0) * (1.0 - dive),
            865.0 + (pupil.1 - 865.0) * (1.0 - dive),
        );
        let camera = format!(
            "translate({} {}) scale({zoom}) translate({} {}) translate({x} {y}) scale({base})",
            target.0, target.1, -pupil.0, -pupil.1
        );
        // the first frame of the loop, frozen, because the code scene ends on it
        let tiger = ctx
            .get_lottie("tiger.json")
            .map_or_else(Svgr::empty, |tiger| tiger.render_at(0.0));
        // the dark pupil fills the screen, then fades out for the outro
        let fade = 1.0 - progress(&frame, &FADE, OUTRO - 0.4);

        svgr!(
            <g>
                <g transform={camera} opacity={fade}>{tiger}</g>
                {enter_exit(&frame, ZOOM + 0.5, OUTRO - 0.8, (540.0, 1350.0), svgr!(
                    <text x="540" y="1450" text-anchor="middle" font-family={DISPLAY} font-size="260" letter-spacing="-3" fill={BONE}>"VECTOR."</text>
                ))}
            </g>
        )
    }
}

/// The wordmark, the feature flag and the address.
#[derive(Debug)]
pub struct Outro;

impl Scene for Outro {
    fn duration(&self) -> Duration<'_> {
        Duration::Frames(beat_frames(END - OUTRO))
    }

    // the wordmark rises while the pupil fades
    fn overlap(&self) -> Overlap {
        Overlap::Previous(0.3)
    }

    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let b = global_beat(&frame) - OUTRO;
        let stay = f32::INFINITY;

        let star = lottie(
            &frame,
            ctx,
            "star_struck.json",
            (400.0, SAFE_TOP + 20.0, 280.0),
            OUTRO,
        );
        let sparkle = lottie(
            &frame,
            ctx,
            "sparkles.json",
            (820.0, 520.0, 150.0),
            OUTRO + 0.5,
        );
        let fan = progress(&frame, &ENTER, OUTRO - 0.5);
        let mark = wordmark(&mut frame, ctx, 540.0, 800.0, 200, fan);

        let typed = ((b - (FEATURE_TYPE_START - OUTRO)) * FEATURE_CHARS_PER_BEAT).max(0.0) as usize;
        let feature = &FEATURE_LINE[..typed.min(FEATURE_LINE.len())];

        svgr!(
            <g>
                {enter_exit(&frame, OUTRO + 0.2, stay, (540.0, 390.0), star)}
                {enter_exit(&frame, OUTRO + 0.6, stay, (895.0, 595.0), sparkle)}
                {enter_exit(&frame, OUTRO - 0.5, stay, (540.0, 740.0), mark)}
                {enter_exit(&frame, OUTRO + 0.75, stay, (540.0, 950.0), svgr!(
                    <g>
                        <text x="540" y="930" text-anchor="middle" font-family={MONO} font-weight="600" font-size="30" letter-spacing="6" fill={ORANGE}>"NEW: LOTTIE FILES"</text>
                        <text x="540" y="1010" text-anchor="middle" font-family={MONO} font-weight="500" font-size="42" fill={BONE}>{feature}</text>
                    </g>
                ))}
                {enter_exit(&frame, OUTRO + 3.0, stay, (540.0, 1200.0), svgr!(
                    <g>
                        <text x="540" y="1220" text-anchor="middle" font-family={SERIF} font-style="italic" font-size="92" fill={BONE}>"fframes.studio"</text>
                        <text x="540" y="1290" text-anchor="middle" font-family={MONO} font-weight="500" font-size="30" fill={MUTED}>"github.com/dmtrkovalenko/fframes"</text>
                    </g>
                ))}
                {enter_exit(&frame, OUTRO + 3.5, stay, (540.0, 1460.0), svgr!(
                    <text x="540" y="1470" text-anchor="middle" font-family={MONO} font-weight="500" font-size="28" fill={MUTED}>"Emoji: Noto Animated Emoji, CC BY 4.0"</text>
                ))}
            </g>
        )
    }
}

/// "fframes" in Instrument Serif centered on `x`, with a trail of three f's on its left.
/// `appear` 0 -> 1 fans the trail out.
fn wordmark(
    frame: &mut Frame,
    ctx: &FFramesContext,
    x: f32,
    y: f32,
    size: usize,
    appear: f32,
) -> Svgr<'static> {
    let query = FontQuery {
        family: SERIF,
        size,
        weight: 400,
        style: FontStyle::Italic,
        ..Default::default()
    };
    let word = text_width(frame, ctx, query, "fframes");
    let f = text_width(frame, ctx, query, "f");
    let step = f * 0.62;
    // center the visible part, "fframes" and the orange f. The two dark f's are on the left.
    let x0 = x - f32::midpoint(word, step) + step;
    let trail: Vec<Svgr> = [(3.0_f32, SOOT), (2.0, EMBER), (1.0, ORANGE)]
        .iter()
        .map(|(k, color)| {
            let dx = -k * step * appear;
            svgr!(<text x={x0 + dx} y={y} font-family={SERIF} font-style="italic" font-size={size} fill={*color}>"f"</text>)
        })
        .collect();
    svgr!(
        <g>
            {trail}
            <text x={x0} y={y} font-family={SERIF} font-style="italic" font-size={size} fill={BONE}>"fframes"</text>
        </g>
    )
}

// ------------------------------------------------------------------ Video

#[derive(Debug)]
pub struct LaunchVideo;

impl Video for LaunchVideo {
    const FPS: usize = FPS;
    const WIDTH: usize = WIDTH;
    const HEIGHT: usize = HEIGHT;
    const BACKGROUND_COLOR: Color = Color::hex(BG);

    fn duration(&self) -> Duration<'_> {
        Duration::Auto
    }

    fn define_scenes(&self) -> Scenes<'_> {
        let scenes: Vec<&dyn Scene> = vec![&Hook, &Grid, &Code, &Zoom, &Outro];
        Scenes::from(scenes)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::from([AudioTrack::new("music", Second(0.0)..Eof)])
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1080 1920" width={WIDTH} height={HEIGHT}>
                {rings(global_beat(&frame))}
                {ctx.render_scenes(&frame)}
            </svg>
        )
    }
}

/// Contour rings behind everything. They drift, and one turn of the wobble takes 8 s.
fn rings(beat: f32) -> Svgr<'static> {
    let rings: Vec<Svgr> = (1..26)
        .map(|i| {
            let i = i as f32;
            let r = i * 62.0;
            let wobble = (i * 0.9 + beat * BEAT * TAU / 8.0).sin() * 10.0;
            svgr!(<ellipse cx="540" cy="960" rx={r + wobble} ry={r * 0.86 - wobble} fill="none" stroke={EMBER} stroke-width="2" opacity={(0.32 - i * 0.012).max(0.0)} />)
        })
        .collect();
    let transform = format!("rotate({} 540 960)", beat * BEAT * 3.0);
    svgr!(<g transform={transform}>{rings}</g>)
}
