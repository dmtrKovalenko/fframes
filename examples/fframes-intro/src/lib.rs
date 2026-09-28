//! A 109 second introduction to fframes, made with fframes.
//!
//! Everything is placed on the beat grid of the soundtrack (see `beat.rs`).
//! Scenes are named after what they show; `cargo run --release -- timeline`
//! lists them with their bars.

pub mod beat;
pub mod facts;
pub mod scenes;
pub mod shaders;
pub mod ui;

use fframes::{
    AudioMap, AudioTimestamp::*, AudioTrack, Color, Duration, FFramesContext, Frame, Scene,
    Scenes, ShaderUniforms, Svgr, Video, include_media_dir,
};

use beat::*;
use scenes::*;
use shaders::SHADERS;
use ui::*;

include_media_dir!(pub struct IntroMedia, "examples/fframes-intro/media");

pub const WIDTH: usize = 1920;
pub const HEIGHT: usize = 1080;

#[derive(Debug)]
pub struct IntroVideo;

/// Section titles for the HUD, by the beat they start on.
const SECTIONS: &[(f32, &str)] = &[
    (-99.0, "00 / PROMPT"),
    (0.0, "01 / ORIGIN"),
    (32.0, "02 / CODE"),
    (48.0, "03 / GPU"),
    (56.0, "04 / TEXT"),
    (62.0, "05 / IMAGE"),
    (66.0, "06 / VIDEO"),
    (72.0, "07 / SHADER"),
    (80.0, "08 / SCALE"),
    (96.0, "09 / BENCHMARK"),
    (128.0, "10 / AGENTS"),
    (168.0, "11 / GPU"),
    (184.0, "12 / RECAP"),
    (196.0, "13 / DONE"),
    (200.0, "14 / FFRAMES"),
];

/// Sound effects, placed on the beat minus the file's attack.
fn sfx(name: &'static str, at: f32, gain: f32) -> AudioTrack<'static> {
    AudioTrack::new(name, Second(at.max(0.0))..Eof).gain_db(gain)
}

impl Video for IntroVideo {
    const FPS: usize = FPS;
    const WIDTH: usize = WIDTH;
    const HEIGHT: usize = HEIGHT;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Auto
    }

    fn audio(&self) -> AudioMap<'_> {
        let mut tracks = vec![
            AudioTrack::new("music.wav", Second(0.0)..Eof).fade_out(1.5),
            sfx("sfx_typing.mp3", 0.02, -15.0),
            sfx("sfx_enter.mp3", beat_time(-1.0) - 0.067, -9.0),
            sfx("sfx_impact.mp3", beat_time(48.0), -10.0),
            sfx("sfx_shutter.mp3", beat_time(62.0) - 0.02, -2.0),
            sfx("sfx_glitch.mp3", beat_time(70.0) - 0.2, -8.0),
            sfx("sfx_impact.mp3", beat_time(96.0), -10.0),
            sfx("sfx_impact.mp3", beat_time(168.0), -11.0),
            sfx("sfx_typing.mp3", beat_time(196.2), -18.0),
            sfx("sfx_impact.mp3", beat_time(200.0), -7.0),
            sfx("sfx_braam.mp3", beat_time(200.0), -9.0),
        ];
        for b in [56.0, 62.0, 66.0, 72.0, 80.0, 128.0, 184.0] {
            tracks.push(sfx("sfx_whoosh.mp3", beat_time(b) - 0.18, -15.0));
        }
        for b in agents::CARD_BEATS {
            tracks.push(sfx("sfx_blip.mp3", beat_time(*b), -9.0));
        }
        AudioMap::from(tracks)
    }

    fn define_scenes(&self) -> Scenes<'_> {
        Scenes::from(vec![
            &PromptScene as &dyn Scene,
            &OriginScene,
            &CodeScene,
            &GpuScene,
            &TextScene,
            &ImageScene,
            &VideoScene,
            &ShaderScene,
            &ScaleScene,
            &BenchmarkScene,
            &AgentsScene,
            &RenderScene,
            &RecapScene,
            &DoneScene,
            &OutroScene,
        ])
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let b = gbeat(&frame);
        let grain = SHADERS.grain.draw(
            &frame,
            ShaderUniforms::new().float("uGrain", 0.07).float("uVignette", 0.55),
        );
        fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080">
                <rect width="1920" height="1080" fill={BG} />
                {energy(&frame, ctx, b)}
                <image href={grain.href()} x="0" y="0" width="1920" height="1080" />
                {hud(&frame, b)}
            </svg>
        )
    }
}

fn hud(frame: &Frame, b: f32) -> Svgr<'static> {
    // the HUD sits out the silent bar and the logo slam
    let hidden = (196.0..204.0).contains(&b);
    let appear = prog(gsec(frame), 0.1, 0.5);
    if hidden || appear <= 0.0 {
        return Svgr::empty();
    }
    let section = SECTIONS
        .iter()
        .rev()
        .find(|(start, _)| b >= *start)
        .map(|(_, name)| *name)
        .unwrap_or("");
    let bar = if b < 0.0 { 0 } else { (b / 4.0).floor() as i32 + 1 };
    let beat_in_bar = if b < 0.0 { 0 } else { (b.rem_euclid(4.0)).floor() as i32 + 1 };
    let tc = timecode(gsec(frame));
    let fnum = format!("F {:05}", frame.global_index);
    let bpm = format!("132 BPM   BAR {bar:03}.{beat_in_bar}");
    // a tick that flashes on every beat
    let tick = if b < 0.0 { 0.0 } else { pulse(b, 7.0) };
    let progress = (gsec(frame) / TOTAL_SECONDS * 1788.0).max(0.5);
    fframes::svgr!(
        <g opacity={appear * 0.9} style="mix-blend-mode:difference">
            {corners(36.0, 36.0, 1848.0, 1008.0, 26.0, "#d8d4cc", 2.0)}
            {label(66.0, 78.0, "FFRAMES — INTRO".to_owned(), "#d8d4cc", 17.0, "start")}
            {label(1854.0, 78.0, bpm, "#d8d4cc", 17.0, "end")}
            <rect x="1840" y="92" width="14" height="14" fill={ORANGE} opacity={0.25 + tick * 0.75} />
            {label(66.0, 1018.0, tc, "#d8d4cc", 17.0, "start")}
            {label(290.0, 1018.0, fnum, "#77736d", 17.0, "start")}
            {label(1854.0, 1018.0, section.to_owned(), "#d8d4cc", 17.0, "end")}
            <rect x="66" y="1034" width="1788" height="1" fill="#d8d4cc" opacity="0.25" />
            <rect x="66" y="1033" width={progress} height="3" fill="#d8d4cc" />
        </g>
    )
}

/// Sections where the full beat plays: the picture pumps with the kick.
const PUMPING: &[(f32, f32)] = &[(48.0, 80.0), (96.0, 128.0), (168.0, 196.0), (200.0, 228.0)];
/// Cuts that glitch for a few frames.
const GLITCH_CUTS: &[f32] = &[32.0, 56.0, 62.0, 66.0, 72.0, 80.0, 128.0, 184.0];
/// Drops that flash.
const DROPS: &[f32] = &[48.0, 96.0, 168.0];

fn energy<'a>(frame: &Frame, ctx: &FFramesContext<'a, '_>, b: f32) -> Svgr<'a> {
    let pumping = PUMPING.iter().any(|(s, e)| (*s..*e).contains(&b));
    let pump = if pumping { pulse(b, 9.0) } else { 0.0 };
    let s = 1.0 + pump * 0.008;
    let flash = DROPS
        .iter()
        .map(|d| if b >= *d { (-(b - d) * 7.0).exp() } else { 0.0 })
        .fold(0.0_f32, f32::max);

    // a cut glitches for 3 frames: the picture splits in bands pushed sideways
    let since_cut = GLITCH_CUTS
        .iter()
        .map(|c| (b - c) * BEAT * FPS as f32)
        .filter(|f| *f >= 0.0 && *f < 3.0)
        .next();
    let scene = if let Some(f) = since_cut {
        let bands: Vec<Svgr> = (0..6)
            .map(|i| {
                let y = i as f32 * 180.0;
                let dx = (hash(i as f32 * 13.0 + f.floor() * 7.0) - 0.5) * 140.0 * (1.0 - f / 3.0);
                let id = format!("glitch-{i}");
                let url = format!("url(#{id})");
                fframes::svgr!(
                    <g>
                        <clipPath id={id.clone()}><rect x="0" y={y} width="1920" height="180" /></clipPath>
                        <g clip-path={url} transform={format!("translate({dx} 0)")}>{ctx.render_scenes(frame)}</g>
                    </g>
                )
            })
            .collect();
        fframes::svgr!(<g>{bands}</g>)
    } else {
        ctx.render_scenes(frame)
    };
    fframes::svgr!(
        <g>
            <g transform={format!("translate(960 540) scale({s}) translate(-960 -540)")}>{scene}</g>
            <rect width="1920" height="1080" fill={BONE} opacity={flash * 0.55} />
        </g>
    )
}
