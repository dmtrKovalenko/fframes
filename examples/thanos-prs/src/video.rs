use fframes::media::ImageData;
use fframes::{
    AudioMap, Duration, FFramesContext, Frame, Shader, ShaderUniforms, Svgr, Video, svgr,
};
use fframes_skia_renderer::render::compile_shader;

use crate::audit::{self, Audit, TILE_HEIGHT, TILE_WIDTH};
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
    featured: [Featured; 6],
    removed: usize,
    lava: Shader,
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
        let featured = std::array::from_fn(|i| {
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
        let removed = prs.iter().filter(|pr| pr.dusted).count();
        let audit = Audit::new(&prs);
        let lava = Shader::sksl(include_str!("shaders/lava.sksl"));
        let dust = Shader::sksl(include_str!("shaders/dust.sksl"));
        let shock = Shader::sksl(include_str!("shaders/shock.sksl"));
        let gauntlet = Shader::sksl(include_str!("shaders/gauntlet.sksl"));
        let charge = Shader::sksl(include_str!("shaders/charge.sksl"));
        let index = Shader::sksl(include_str!("shaders/index.sksl"));
        for (name, shader) in [
            ("lava", &lava),
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
            lava,
            dust,
            shock,
            gauntlet,
            charge,
            index,
            audit,
        }
    }

    fn list<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>, t: f32) -> Svgr<'a> {
        let opacity = 1.0 - overview(t);
        if opacity <= 0.0 {
            return Svgr::empty();
        }
        let folding = browse(t);
        let (wave_x, wave_y) = Pose::at(SNAP).tip();
        let shock_age = t - SNAP;
        let has_wave = (0.0..0.62).contains(&shock_age);
        let mut survivors = 0;
        let rows = self.featured.iter().enumerate().map(|(rank, featured)| {
            let pr = &self.prs[featured.index];
            let rank = rank as f32;
            let mut top = LIST_Y + rank * ROW_PITCH;
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
                            {ui::card(pr, &mut frame, ctx, 0.0)}
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
                if let Some(image) = cached_card(pr, &mut frame, ctx) {
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
            top += (1.0 - ease_out(phase(t, -0.11 + rank * 0.014, 0.23 + rank * 0.033))) * (20.0 + rank * 2.0);
            let hover = if t < OVERVIEW {
                hover(t, LIST_X, top, CARD_WIDTH as f32, CARD_HEIGHT as f32)
            } else {
                0.0
            };
            svgr!(<g transform={format!("translate({LIST_X} {top})")}>{ui::card(pr, &mut frame, ctx, hover)}</g>)
        }).collect::<Svgr>();
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
        let impact = if age < 0.0 {
            0.23 * ramp(t, YES, SNAP)
        } else {
            1.36 * (-age * 3.05).exp()
        };
        let (wave_x, _) = Pose::at(SNAP).tip();
        let focus = ramp(t, THANOS, THANOS + 0.51) * (1.0 - ramp(t, SNAP + 0.01, SNAP + 0.76));
        let arm =
            ease_out(phase(t, YES + 0.06, YES + 0.76)) * (1.0 - ramp(t, SNAP + 0.05, SNAP + 0.62));
        let lava = self.lava.draw(
            &frame,
            ShaderUniforms::new()
                .float("uShock", impact)
                .float("uSnapAge", age)
                .float("uSnapX", wave_x / WIDTH as f32)
                .float("uFocus", arm),
        );
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
        let pose = Pose::at(t);
        let hero = hero::gauntlet(&frame, ctx, &self.gauntlet, &self.charge, &pose);

        svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width={WIDTH} height={HEIGHT} viewBox="0 0 1440 1280">
                <defs>
                    <linearGradient id="thanos-button" x1="0" x2="0.7" y1="0" y2="1">
                        <stop offset="0" stop-color="#442f26"/>
                        <stop offset="1" stop-color="#292124"/>
                    </linearGradient>
                    <linearGradient id="dialog" x1="0" x2="0.9" y1="0" y2="1">
                        <stop offset="0" stop-color="#302b31"/>
                        <stop offset="0.49" stop-color="#24222a"/>
                        <stop offset="1" stop-color="#1c1b22"/>
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
                <image width={WIDTH} height={HEIGHT} href={lava.href()}/>
                <g transform={camera}>
                    {ui::heading(t, self.prs.len(), remaining)}
                    {map}
                    {rows}
                    {embers(t, impact)}
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
) -> Option<ImageData<'static>> {
    if let Some(image) = pr.card_image.get() {
        return Some(image.clone());
    }
    let tree = svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width={CARD_WIDTH} height={CARD_HEIGHT} font-family="Inter 24pt">
            {ui::card(pr, frame, ctx, 0.0)}
        </svg>
    );
    let image = ctx.render_svgr(tree, CARD_WIDTH, CARD_HEIGHT)?;
    Some(pr.card_image.get_or_init(|| image).clone())
}

fn embers(t: f32, impact: f32) -> Svgr<'static> {
    (0..58_u32)
        .map(|i| {
            let a = random(i * 11 + 1);
            let b = random(i * 11 + 2);
            let c = random(i * 11 + 3);
            let d = random(i * 11 + 4);
            let life = (b + t * (0.12 + 0.09 * c)).fract();
            let x = a * 1450.0 - 8.0 + life * (16.0 + 77.0 * d) + (t + a * 33.0).sin() * 13.0;
            let y = 908.0 - life * (120.0 + 564.0 * c.powi(2));
            let glow = (life * 20.0).min(1.0) * (1.0 - life).powi(2);
            let flicker = 0.68 + 0.32 * (t * (4.0 + d * 9.0) + 90.0 * a).sin();
            let opacity = (glow * flicker * (0.64 + impact * 0.43)).min(1.0);
            let size = 0.65 + 2.1 * d.powi(3);
            svgr!(
                <g opacity={opacity}>
                    <circle cx={x} cy={y} r={size * 2.7} fill="#ff8741" fill-opacity="0.16"/>
                    <ellipse cx={x} cy={y} rx={size * 0.69} ry={size * 1.34} fill="#ffce9d"/>
                </g>
            )
        })
        .collect()
}
