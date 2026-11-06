use std::sync::OnceLock;

use fframes::media::ImageData;
use fframes::{
    AudioMap, Duration, FFramesContext, FFramesSyncedVideoFrame, Frame, Shader, ShaderUniforms,
    Svgr, Video, svgr,
};
use fframes_skia_renderer::render::compile_shader;

use crate::audit::{self, Audit, TILE_HEIGHT, TILE_WIDTH};
use crate::glass;
use crate::hero::{self, Pose};
use crate::motion::*;
use crate::sound;
use crate::ui::{self, CARD_HEIGHT, CARD_WIDTH, LIST_X, LIST_Y, PullRequest, ROW_PITCH};

const WIDTH: usize = 1440;
const HEIGHT: usize = 1280;
const PARTICLE_HEADROOM: f32 = 228.0;
const DUST_DURATION: f32 = 2.22;

struct Featured {
    index: usize,
    tile: (f32, f32),
}

pub struct SnapVideo {
    prs: Vec<PullRequest>,
    featured: [Featured; 5],
    removed: usize,
    footage: Shader,
    glass: OnceLock<ImageData<'static>>,
    dust: Shader,
    shock: Shader,
    gauntlet: Shader,
    charge: Shader,
    index: Shader,
    audit: Audit,
}

impl SnapVideo {
    pub fn new() -> Self {
        let prs: Vec<PullRequest> = serde_json::from_str(include_str!("../pull-requests.json"))
            .expect("parse GitHub pull request snapshot");
        let mut featured = std::array::from_fn(|i| {
            let repo = ["fframes", "fff"][i % 2];
            let index = prs
                .iter()
                .enumerate()
                .filter(|(_, pr)| pr.repo == repo)
                .nth(i / 2)
                .expect("three pull requests per repository")
                .0;
            Featured {
                index,
                tile: audit::tile_origin(&prs, index),
            }
        });
        featured.swap(1, 2);
        let removed = prs.iter().filter(|pr| pr.dusted).count();
        let audit = Audit::new(&prs);
        let footage = Shader::sksl(include_str!("shaders/footage.sksl"));
        let dust = Shader::sksl(include_str!("shaders/dust.sksl"));
        let shock = Shader::sksl(include_str!("shaders/shock.sksl"));
        let gauntlet = Shader::sksl(include_str!("shaders/gauntlet.sksl"));
        let charge = Shader::sksl(include_str!("shaders/charge.sksl"));
        let index = Shader::sksl(include_str!("shaders/index.sksl"));
        for (name, shader) in [
            ("footage", &footage),
            ("dust", &dust),
            ("shock", &shock),
            ("gauntlet", &gauntlet),
            ("charge", &charge),
            ("index", &index),
        ] {
            if let Err(error) = compile_shader(shader) {
                panic!("failed to compile {name}: {error}");
            }
        }

        Self {
            prs,
            featured,
            removed,
            footage,
            glass: OnceLock::new(),
            dust,
            shock,
            gauntlet,
            charge,
            index,
            audit,
        }
    }

    fn backdrop(&self, frame: &Frame, ctx: &FFramesContext<'_, '_>) -> Svgr<'static> {
        let t = frame.seconds();
        // Cut on the snap from the undisturbed floor to the first clean eruption shot.
        // The source task is crossing the horizon between these two in-points.
        let source_time = if t < SNAP {
            4.00 * t / SNAP
        } else {
            5.60 + (9.10 - 5.60) * (t - SNAP) / (DURATION - SNAP)
        };
        let Some(decoded) = frame.video_frame_at(ctx, glass::FOOTAGE, f64::from(source_time), None)
        else {
            return svgr!(<rect width={WIDTH} height={HEIGHT} fill="#110e12"/>);
        };
        let image = decoded.into_image();
        let plate = self.footage.draw(
            frame,
            ShaderUniforms::new()
                .float("uSourceTime", source_time)
                .image("uSource", &image),
        );
        svgr!(<image width={WIDTH} height={HEIGHT} href={plate.href()}/>)
    }

    fn list<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>, t: f32) -> Svgr<'a> {
        let opacity = 1.0 - overview(t);
        if opacity <= 0.0 {
            return Svgr::empty();
        }
        if self.glass.get().is_none()
            && let Some(material) = glass::row(&frame, ctx)
        {
            let _ = self.glass.set(material);
        }
        let glass = self.glass.get();
        let folding = browse(t);
        let (wave_x, wave_y) = Pose::at(SNAP).tip();
        let shock_age = t - SNAP;
        let has_wave = (0.0..0.62).contains(&shock_age);
        let mut survivors = 0;
        let drag = ramp(t, PICKUP, PICKUP + 0.15) * (1.0 - ramp(t, DROP, DROP + 0.19));
        let drag_scale = 1.0 + 0.029 * drag;
        let dragged_top =
            LIST_Y + ROW_PITCH * (2.0 - (539.0 - cursor(t.clamp(PICKUP, DROP)).1) / 126.0);
        let drag_x = LIST_X + 10.0 * drag - CARD_WIDTH as f32 * 0.5 * (drag_scale - 1.0);
        let drag_y = dragged_top - drag * 6.0;
        let mut rows: Vec<_> = self.featured.iter().enumerate().map(|(slot, featured)| {
            let pr = &self.prs[featured.index];
            let lifted = slot == 2 && drag > 0.0;
            let rank = slot as f32;
            let mut top = LIST_Y + rank * ROW_PITCH;
            if slot == 1 {
                top += ROW_PITCH * (1.0 - ramp(t, PICKUP + 0.38, DROP - 0.04));
            } else if slot == 2 {
                top = dragged_top;
            }
            if folding > 0.0 {
                let stagger = rank * 0.042;
                let morph = ease_out(phase(folding, stagger, 0.83 + stagger * 0.73));
                let (end_x, end_y) = featured.tile;
                let x = LIST_X + (end_x - LIST_X) * morph;
                let y = top + (end_y - top) * morph + morph * (1.0 - morph) * (28.0 + rank * 5.0);
                let card_x = 1.0 - morph * (1.0 - TILE_WIDTH as f32 / CARD_WIDTH as f32);
                let card_y = 1.0 - morph * (1.0 - TILE_HEIGHT as f32 / CARD_HEIGHT as f32);
                let badge_alpha = ramp(morph, 0.39, 0.85) * (1.0 - ramp(morph, 0.97, 1.0));
                let card_alpha = 1.0 - ramp(morph, 0.44, 0.88);
                return svgr!(
                    <g transform={format!("translate({x} {y})")}>
                        <g transform={format!("scale({card_x} {card_y})")} opacity={card_alpha}>
                            {ui::card(pr, &mut frame, ctx, glass, 0.0)}
                        </g>
                        <g transform={format!("scale({})", 1.0 + (1.0 - morph) * 0.75)} opacity={badge_alpha}>
                            {audit::tile_badge(pr)}
                        </g>
                    </g>
                );
            }

            let age = if pr.dusted { shock_age - 0.085 - 0.048 * rank } else { -1.0 };
            if age >= 0.0 || has_wave {
                if age >= DUST_DURATION {
                    return Svgr::empty();
                }
                if let Some(image) = cached_card(pr, &mut frame, ctx, glass) {
                    let (x, y, width, height) = if age >= 0.0 {
                        (0.0, top - PARTICLE_HEADROOM, WIDTH as f32, 376.0)
                    } else {
                        (LIST_X - 20.0, top - 20.0, CARD_WIDTH as f32 + 40.0, CARD_HEIGHT as f32 + 40.0)
                    };
                    let dust = self.dust.draw(
                        &frame,
                        ShaderUniforms::new()
                            .float2("uSize", CARD_WIDTH as f32, CARD_HEIGHT as f32)
                            .float2("uOffset", LIST_X - x, top - y)
                            .float("uTime", age)
                            .float("uSeed", random(pr.number))
                            .float("uWaveAge", shock_age)
                            .float2("uWaveOrigin", wave_x, wave_y)
                            .float2("uPosition", x, y)
                            .image("uCard", &image),
                    );
                    if !pr.dusted {
                        survivors += 1;
                    }
                    return svgr!(<image x={x} y={y} width={width} height={height} href={dust.href()}/>);
                }
            }
            if !pr.dusted {
                let shift = (rank - survivors as f32) * ROW_PITCH;
                let settle = SNAP + 0.83 + rank * 0.072;
                let p = ease_out(phase(t, settle, settle + 0.42));
                let carry = phase(t, settle + 0.22, settle + 0.57);
                let recoil = (carry * std::f32::consts::PI).sin() * (1.0 - carry);
                top -= shift * p + f32::from(shift > 0.0) * recoil * 10.0;
                survivors += 1;
            }
            let hot = if t < OVERVIEW {
                hover(t, LIST_X, top, CARD_WIDTH as f32, CARD_HEIGHT as f32)
            } else { 0.0 };
            let card = ui::card(pr, &mut frame, ctx, glass, hot * 0.42);
            let (x, y, scale) = if lifted { (drag_x, drag_y, drag_scale) } else { (LIST_X, top, 1.0) };
            svgr!(
                <g transform={format!("translate({x} {y}) scale({scale})")}>
                    {card}
                    {if lifted {
                        svgr!(<rect x="1" y="1" width={CARD_WIDTH - 2} height={CARD_HEIGHT - 2} rx="20"
                             fill="#d9c3b1" fill-opacity={drag * 0.05} stroke="#d7cbc1" stroke-opacity={drag * 0.13}/>)
                    } else { Svgr::empty() }}
                </g>
            )
        }).collect();
        if drag > 0.0 {
            let picked = rows.remove(2);
            let base = svgr!(<g>{rows}</g>);
            let hull = svgr!(<rect x={drag_x} y={drag_y} width={CARD_WIDTH as f32 * drag_scale}
                                  height={CARD_HEIGHT as f32 * drag_scale} rx="20"/>);
            return svgr!(<g>
                <defs><clipPath id="drag-glass">{hull.clone()}</clipPath></defs>
                {base.clone()}
                <g transform="translate(0 8)" fill="#060506" opacity={drag * 0.24}>{hull.clone()}</g>
                <g clip-path="url(#drag-glass)" opacity={drag}>
                    <g fill="#1d1719">{hull}</g>
                    <g filter="url(#frosted-app)" opacity="0.59">{base}</g>
                </g>
                {picked}
            </g>);
        }
        let amount = if folding > 0.0 { 1.0 } else { opacity };
        svgr!(<g opacity={amount}>{rows}</g>)
    }

    fn snap_flash(&self, frame: &Frame, t: f32) -> Svgr<'static> {
        let age = t - SNAP;
        if !(0.0..0.77).contains(&age) {
            return Svgr::empty();
        }
        let (x, y) = Pose::at(SNAP).tip();
        let layer = self.shock.draw(
            frame,
            ShaderUniforms::new()
                .float("uAge", age)
                .float2("uOrigin", x, y),
        );
        svgr!(<image width="1440" height="1280" href={layer.href()}/>)
    }
}

impl Video for SnapVideo {
    const WIDTH: usize = WIDTH;
    const HEIGHT: usize = HEIGHT;
    const FPS: usize = 60;

    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(DURATION)
    }

    fn audio(&self) -> AudioMap<'_> {
        sound::tracks()
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.seconds();
        let age = t - SNAP;
        let focus = ramp(t, THANOS, THANOS + 0.51) * (1.0 - ramp(t, SNAP + 0.01, SNAP + 0.76));
        let arm =
            ease_out(phase(t, YES + 0.06, YES + 0.76)) * (1.0 - ramp(t, SNAP + 0.05, SNAP + 0.62));
        let plate = self.backdrop(&frame, ctx);
        let shake = if (0.0..0.37).contains(&age) {
            (-age * 13.0).exp() * (age * 51.0).sin()
        } else {
            0.0
        };
        let roll = -0.43 * arm + 0.58 * shake;
        let push = 1.0 + 0.038 * focus + 0.027 * arm + 0.013 * shake;
        let camera = format!(
            "translate({} {}) rotate({roll}) scale({push}) translate(-720 -512)",
            720.0 + 6.0 * arm - 9.0 * shake,
            512.0 + 11.0 * arm + 6.0 * shake,
        );
        let rows = self.list(frame.clone(), ctx, t);
        let map = self
            .audit
            .draw(&frame, ctx, &self.dust, &self.index, &self.prs);
        let remaining = self.audit.remaining(t).into_iter().sum();
        let app = svgr!(<g>{ui::heading(t, self.prs.len(), remaining)}{map}{rows}</g>);
        let frost = ramp(t, THANOS, THANOS + 0.26) * (1.0 - ramp(t, YES + 0.02, YES + 0.24));
        let app = if frost > 0.0 {
            svgr!(<g>
                <g opacity={1.0 - frost}>{app.clone()}</g>
                <g filter="url(#frosted-app)" opacity={frost}>{app}</g>
            </g>)
        } else {
            app
        };
        let pose = Pose::at(t);
        let hero = hero::gauntlet(&frame, ctx, &self.gauntlet, &self.charge, &pose);

        svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width={WIDTH} height={HEIGHT} viewBox="0 0 1440 1280">
                <defs>
                    <filter id="frosted-app" x="-0.05" y="-0.05" width="1.1" height="1.1">
                        <feGaussianBlur stdDeviation="6.5"/>
                    </filter>
                    <linearGradient id="thanos-button" x1="0" x2="0.7" y1="0" y2="1">
                        <stop offset="0" stop-color="#c99262" stop-opacity="0.20"/>
                        <stop offset="1" stop-color="#b3a3b2" stop-opacity="0.065"/>
                    </linearGradient>
                    <linearGradient id="dialog" x1="0" x2="0.9" y1="0" y2="1">
                        <stop offset="0" stop-color="#3c3439" stop-opacity="0.85"/>
                        <stop offset="0.49" stop-color="#27232a" stop-opacity="0.77"/>
                        <stop offset="1" stop-color="#342720" stop-opacity="0.76"/>
                    </linearGradient>
                    <radialGradient id="gold-aura">
                        <stop offset="0" stop-color="#f7a76b" stop-opacity="0.26"/>
                        <stop offset="0.47" stop-color="#c9723d" stop-opacity="0.14"/>
                        <stop offset="1" stop-color="#884524" stop-opacity="0"/>
                    </radialGradient>
                    <radialGradient id="comment-shadow">
                        <stop offset="0" stop-color="#08090d" stop-opacity="0.93"/>
                        <stop offset="0.66" stop-color="#08090d" stop-opacity="0.73"/>
                        <stop offset="1" stop-color="#08090d" stop-opacity="0"/>
                    </radialGradient>
                </defs>
                {plate}
                <g transform={camera}>
                    {app}
                    {ui::dialog(t, self.prs.len(), self.removed)}
                    {hero}
                    {self.snap_flash(&frame, t)}
                    {ui::pointer(t)}
                </g>
                {hero::comments(ctx, t)}
            </svg>
        )
    }
}

fn cached_card<'a>(
    pr: &'a PullRequest,
    frame: &mut Frame,
    ctx: &FFramesContext<'a, '_>,
    glass: Option<&ImageData<'static>>,
) -> Option<ImageData<'static>> {
    if let Some(image) = pr.card_image.get() {
        return Some(image.clone());
    }
    let tree = svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width={CARD_WIDTH} height={CARD_HEIGHT} font-family="Inter 24pt">
            {ui::card(pr, frame, ctx, glass, 0.0)}
        </svg>
    );
    let image = ctx.render_svgr(tree, CARD_WIDTH, CARD_HEIGHT)?;
    Some(pr.card_image.get_or_init(|| image).clone())
}
