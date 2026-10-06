//! Separate textured cards in camera space. Geometry is prepared once; Skia
//! projects and samples each card on the GPU, within its visible screen bounds.

use std::fmt::Write;

use fframes::{FFramesContext, Frame, Shader, ShaderUniforms, Svgr};

#[path = "catalog_layout.rs"]
mod layout;

type V3 = [f32; 3];
const PIXELS_PER_UNIT: f32 = 768.0;
const FOCAL: f32 = 1.6 * 1080.0;

fn add(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] + b[i])
}
fn sub(a: V3, b: V3) -> V3 {
    std::array::from_fn(|i| a[i] - b[i])
}
fn mul(a: V3, s: f32) -> V3 {
    a.map(|v| v * s)
}
fn turn(mut p: V3, a: usize, b: usize, angle: f32) -> V3 {
    let (s, c) = angle.sin_cos();
    (p[a], p[b]) = (c * p[a] - s * p[b], s * p[a] + c * p[b]);
    p
}
fn smooth(start: f32, end: f32, value: f32) -> f32 {
    let t = ((value - start) / (end - start)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn random(index: usize, salt: usize) -> f32 {
    ((index * 73 + salt * 151 + index * index * 17) % 997) as f32 / 997.0
}

#[derive(Clone, Copy, Debug)]
struct Pose {
    center: V3,
    right: V3,
    down: V3,
}

impl Pose {
    fn world(index: usize, frame: f32) -> Self {
        let [x, y, w, h] = layout::RECTS[index];
        // Reference frames 280–295: different cards release at slightly
        // different times, open gaps, and overlap at different depths.
        let amount = smooth(23.0 + random(index, 1) * 3.0, 38.0, frame);
        let drift = smooth(36.0, 49.0, frame);
        let r = |salt| random(index, salt) * 2.0 - 1.0;
        let mut angles = [r(2) * 0.24, r(3) * 0.28, r(4) * 0.060];
        let mut offset = [r(5) * 0.042, r(6) * 0.035, r(7) * 0.19 - 0.035];
        offset[1] += drift * (0.018 + random(index, 8) * 0.035);
        if index == layout::PORTAL_TILE {
            angles = [0.10, -0.13, 0.045];
            offset = [-0.01, 0.015, -0.13];
        }
        let rotate = |p| {
            let p = turn(p, 1, 2, angles[0] * amount);
            let p = turn(p, 0, 2, angles[1] * amount);
            turn(p, 0, 1, angles[2] * amount)
        };
        Self {
            center: add(
                [
                    (x + w * 0.5) / PIXELS_PER_UNIT,
                    (y + h * 0.5) / PIXELS_PER_UNIT,
                    0.0,
                ],
                mul(offset, amount),
            ),
            right: rotate([1.0, 0.0, 0.0]),
            down: rotate([0.0, 1.0, 0.0]),
        }
    }

    fn point(self, x: f32, y: f32) -> V3 {
        add(self.center, add(mul(self.right, x), mul(self.down, y)))
    }
}

struct Camera {
    origin: V3,
    angles: V3,
}

impl Camera {
    fn new(frame: f32) -> Self {
        let entering = (1.0 - (frame / 18.0).clamp(0.0, 1.0)).powi(2);
        let leaving = smooth(39.0, 47.0, frame) * (1.0 - smooth(48.0, 50.0, frame));
        let distance = if frame < 18.0 {
            2.25 - 2.01 * entering
        } else if frame < 43.0 {
            2.25
        } else if frame < 48.0 {
            2.25 - 1.80 * ((frame - 43.0) / 5.0).powf(1.6)
        } else if frame < 49.0 {
            0.45 - 0.19 * (frame - 48.0)
        } else {
            0.26 - 0.155 * (frame - 49.0).clamp(0.0, 1.0)
        };
        let [x, y, w, h] = layout::RECTS[layout::PORTAL_TILE];
        let hero = Pose::world(layout::PORTAL_TILE, frame).point(
            (layout::PORTAL[0] - x - w * 0.5) / PIXELS_PER_UNIT,
            (layout::PORTAL[1] - y - h * 0.5) / PIXELS_PER_UNIT,
        );
        let base = [
            (2360.0 - 100.0 * entering) / PIXELS_PER_UNIT,
            1100.0 / PIXELS_PER_UNIT,
            0.0,
        ];
        let target = add(base, mul(sub(hero, base), smooth(40.0, 49.0, frame)));
        let angles = [
            0.20 * entering - 0.10 * leaving,
            -0.48 * entering + 0.25 * leaving,
            0.12 * entering - 0.025 * leaving,
        ];
        let forward = turn(turn([0.0, 0.0, 1.0], 0, 2, angles[1]), 1, 2, angles[2]);
        Self {
            origin: sub(target, mul(forward, distance)),
            angles,
        }
    }

    fn direction(&self, p: V3) -> V3 {
        let p = turn(p, 1, 2, -self.angles[2]);
        let p = turn(p, 0, 2, -self.angles[1]);
        turn(p, 0, 1, -self.angles[0])
    }

    fn pose(&self, pose: Pose) -> Pose {
        Pose {
            center: self.direction(sub(pose.center, self.origin)),
            right: self.direction(pose.right),
            down: self.direction(pose.down),
        }
    }
}

fn project(p: V3) -> [f32; 2] {
    [960.0 + FOCAL * p[0] / p[2], 540.0 + FOCAL * p[1] / p[2]]
}

#[derive(Debug)]
struct Card {
    index: usize,
    pose: Pose,
    motion: Pose,
    bounds: [f32; 4],
}

impl Card {
    fn new(index: usize, start: f32, end: f32) -> Option<Self> {
        let pose = Camera::new(start).pose(Pose::world(index, start));
        let last = Camera::new(end).pose(Pose::world(index, end));
        let [_, _, width, height] = layout::RECTS[index];
        let mut lo = [1920.0_f32, 1080.0_f32];
        let mut hi = [0.0_f32, 0.0_f32];
        for sample in [pose, last] {
            let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(x, y)| {
                sample.point(
                    x * width / (2.0 * PIXELS_PER_UNIT),
                    y * height / (2.0 * PIXELS_PER_UNIT),
                )
            });
            // Clip to the camera's near plane before projecting. During the
            // icon dive, the large hero card straddles that plane.
            for i in 0..4 {
                let a = corners[i];
                let b = corners[(i + 1) % 4];
                let mut points = [None, None];
                if a[2] >= 0.01 {
                    points[0] = Some(a);
                }
                if (a[2] >= 0.01) != (b[2] >= 0.01) {
                    points[1] = Some(add(a, mul(sub(b, a), (0.01 - a[2]) / (b[2] - a[2]))));
                }
                for point in points.into_iter().flatten() {
                    let p = project(point);
                    for axis in 0..2 {
                        lo[axis] = lo[axis].min(p[axis]);
                        hi[axis] = hi[axis].max(p[axis]);
                    }
                }
            }
        }
        let x = (lo[0] - 5.0).floor().max(0.0);
        let y = (lo[1] - 5.0).floor().max(0.0);
        let w = (hi[0] + 5.0).ceil().min(1920.0) - x;
        let h = (hi[1] + 5.0).ceil().min(1080.0) - y;
        (w > 0.0 && h > 0.0).then_some(Self {
            index,
            pose,
            motion: Pose {
                center: sub(last.center, pose.center),
                right: sub(last.right, pose.right),
                down: sub(last.down, pose.down),
            },
            bounds: [x, y, w, h],
        })
    }
}

#[derive(Debug)]
struct CatalogFrame {
    cards: Vec<Card>,
    portal_clip: String,
    shutter: bool,
}

#[derive(Debug)]
pub(super) struct Catalog {
    frames: Vec<CatalogFrame>,
}

impl Catalog {
    pub(super) fn new() -> Self {
        let frames = (0..51)
            .map(|index| {
                let frame = index as f32;
                let shutter = index < 7 || (44..49).contains(&index);
                let exposure = if shutter { 0.35 } else { 0.0 };
                let mut cards: Vec<_> = (0..layout::RECTS.len())
                    .filter_map(|i| {
                        Card::new(i, (frame - exposure * 0.5).max(0.0), frame + exposure * 0.5)
                    })
                    .collect();
                cards.sort_by(|a, b| b.pose.center[2].total_cmp(&a.pose.center[2]));
                let mut portal_clip = String::new();
                if index >= 49 {
                    let pose = Camera::new(frame).pose(Pose::world(layout::PORTAL_TILE, frame));
                    let [x, y, w, h] = layout::RECTS[layout::PORTAL_TILE];
                    for i in 0..96 {
                        let angle = i as f32 * std::f32::consts::TAU / 96.0;
                        let p = project(pose.point(
                            (layout::PORTAL[0] - x - w * 0.5 + 23.0 * angle.cos())
                                / PIXELS_PER_UNIT,
                            (layout::PORTAL[1] - y - h * 0.5 + 23.0 * angle.sin())
                                / PIXELS_PER_UNIT,
                        ));
                        let _ = write!(
                            portal_clip,
                            "{}{} {} ",
                            if i == 0 { 'M' } else { 'L' },
                            p[0],
                            p[1]
                        );
                    }
                    portal_clip.push('Z');
                }
                CatalogFrame {
                    cards,
                    portal_clip,
                    shutter,
                }
            })
            .collect();
        Self { frames }
    }

    pub(super) fn render<'a>(
        &'a self,
        frame: &Frame,
        ctx: &FFramesContext<'a, '_>,
        shader: &Shader,
    ) -> Svgr<'a> {
        let Some(atlas) = ctx.get_image("catalog.png") else {
            return Svgr::empty();
        };
        let Some(state) = self.frames.get(frame.index) else {
            return Svgr::empty();
        };
        let cards: Vec<_> = state
            .cards
            .iter()
            .map(|card| {
                let [x, y, w, h] = card.bounds;
                let [ax, ay, aw, ah] = layout::RECTS[card.index];
                let mut uniforms = ShaderUniforms::new()
                    .image("uMask", atlas)
                    .float("uClock", frame.index as f32 * 1001.0 / 30000.0)
                    .float("uProgress", frame.index as f32 / 51.0)
                    .float("uVariant", 0.0)
                    .float("uBeat", 0.0)
                    .float("uShutter", if state.shutter { 1.0 } else { 0.0 })
                    .float(
                        "uHero",
                        if card.index == layout::PORTAL_TILE {
                            1.0
                        } else {
                            0.0
                        },
                    )
                    .float2("uOrigin", x, y)
                    .float2("uPortal", layout::PORTAL[0], layout::PORTAL[1])
                    .float4("uAtlas", ax, ay, aw, ah);
                for (name, value) in [
                    ("uCenter", card.pose.center),
                    ("uRight", card.pose.right),
                    ("uDown", card.pose.down),
                    ("uCenterMotion", card.motion.center),
                    ("uRightMotion", card.motion.right),
                    ("uDownMotion", card.motion.down),
                ] {
                    uniforms = uniforms.float3(name, value[0], value[1], value[2]);
                }
                let layer = shader.draw(frame, uniforms);
                fframes::svgr!(<image href={layer.href()} x={x} y={y} width={w} height={h} />)
            })
            .collect();
        let title = if state.portal_clip.is_empty() {
            Svgr::empty()
        } else {
            fframes::svgr!(
                <g>
                    <defs><clipPath id="catalog-portal-title"><path d={state.portal_clip.as_str()} /></clipPath></defs>
                    <text x="960" y="576" text-anchor="middle" clip-path="url(#catalog-portal-title)"
                        font-family="DM Sans" font-weight="500" font-size="64" letter-spacing="-1.8" fill="#fcf5ff">"SkSL + SHADERTOY"</text>
                </g>
            )
        };
        fframes::svgr!(<g><rect width="1920" height="1080" fill="#08080a" />{cards}{title}</g>)
    }
}
