use std::sync::OnceLock;

use fframes::media::ImageData;
use fframes::{FFramesContext, Frame, Shader, ShaderUniforms, Svgr, svgr};

use crate::motion::{SNAP, ease_out, overview, ramp};
use crate::ui::PullRequest;

const COLUMNS: usize = 25;
const GRID_WIDTH: u32 = 1182;
const GRID_HEIGHT: u32 = 538;
const GRID_LEFT: f32 = 129.0;
const GRID_TOP: f32 = 278.0;
const SECOND_ROW: usize = 164;
const MAP_DUST: f32 = SNAP + 0.32;
const MAP_RATE: f32 = 0.46;
pub const TILE_WIDTH: u32 = 43;
pub const TILE_HEIGHT: u32 = 21;
const REPOSITORIES: [(&str, &str, &str); 2] = [
    ("fframes", "#263748", "#c0d7ef"),
    ("fff", "#382f46", "#d8c5eb"),
];

pub struct Audit {
    survivors: OnceLock<ImageData<'static>>,
    selected: OnceLock<ImageData<'static>>,
    totals: [usize; 2],
    releases: [Vec<f32>; 2],
}

impl Audit {
    pub fn new(prs: &[PullRequest]) -> Self {
        let mut totals = [0; 2];
        let mut releases = [Vec::new(), Vec::new()];
        for pr in prs {
            let group = usize::from(pr.repo == "fff");
            let (x, _) = tile_offset(group, totals[group]);
            totals[group] += 1;
            if pr.dusted {
                // Track the shader's release front at the tile center.
                let across = (x as f32 + TILE_WIDTH as f32 * 0.5) / GRID_WIDTH as f32;
                releases[group].push(MAP_DUST + (0.89 - across * 0.80) / MAP_RATE);
            }
        }
        for release in &mut releases {
            release.sort_by(f32::total_cmp);
        }
        Self {
            totals,
            releases,
            survivors: OnceLock::new(),
            selected: OnceLock::new(),
        }
    }

    pub fn remaining(&self, t: f32) -> [usize; 2] {
        std::array::from_fn(|group| {
            self.totals[group] - self.releases[group].partition_point(|&at| at <= t)
        })
    }

    pub fn draw(
        &self,
        frame: &Frame,
        ctx: &FFramesContext<'_, '_>,
        dust: &Shader,
        index: &Shader,
        prs: &[PullRequest],
    ) -> Svgr<'static> {
        let t = frame.seconds();
        let opacity = overview(t);
        if opacity <= 0.0 {
            return Svgr::empty();
        }
        let Some(survivors) = render_grid(ctx, &self.survivors, prs, false) else {
            return Svgr::empty();
        };
        let Some(selected) = render_grid(ctx, &self.selected, prs, true) else {
            return Svgr::empty();
        };
        let [fframes, fff] = self.remaining(t);
        let y = 21.0 * (1.0 - ease_out(opacity));
        let age = (t - MAP_DUST) * MAP_RATE;
        let tiles = if age < 0.0 {
            let layer = index.draw(
                frame,
                ShaderUniforms::new()
                    .float("uAmount", opacity)
                    .image("uRetained", &survivors)
                    .image("uSelected", &selected),
            );
            svgr!(
                <image x={GRID_LEFT} y={GRID_TOP} width={GRID_WIDTH} height={GRID_HEIGHT} href={layer.href()}/>
            )
        } else {
            let fragments = if age < 2.22 {
                let layer = dust.draw(
                    frame,
                    ShaderUniforms::new()
                        .float2("uSize", GRID_WIDTH as f32, GRID_HEIGHT as f32)
                        .float2("uOffset", GRID_LEFT, 212.0)
                        .float("uTime", age)
                        .float("uSeed", 0.413)
                        .float("uWaveAge", -1.0)
                        .float2("uWaveOrigin", 0.0, 0.0)
                        .float2("uPosition", 0.0, 0.0)
                        .image("uCard", &selected),
                );
                svgr!(<image x="0" y={GRID_TOP - 212.0} width="1440" height="805" href={layer.href()}/>)
            } else {
                Svgr::empty()
            };
            let ghosts = ramp(age, 0.8, 1.17) * 0.13;
            svgr!(
                <g>
                    <image x={GRID_LEFT} y={GRID_TOP} width={GRID_WIDTH} height={GRID_HEIGHT} opacity={ghosts} href={selected.href()}/>
                    <image x={GRID_LEFT} y={GRID_TOP} width={GRID_WIDTH} height={GRID_HEIGHT} href={survivors.href()}/>
                    {fragments}
                </g>
            )
        };
        svgr!(
            <g opacity={ease_out(opacity)} transform={format!("translate(0 {y})")}>
                <rect x="104.5" y="211.5" width="1231" height="630" rx="19" fill="#887e8d" fill-opacity="0.092" stroke="#c3b7bc" stroke-opacity="0.14"/>
                <text x="129" y="254" fill="#e1ecfa" font-weight="700" font-size="24">"fframes"</text>
                <text x="1312" y="253" fill="#a6a4ae" text-anchor="end" font-size="19">{fframes.to_string()} " pull requests"</text>
                <path d="M129 387h1182" stroke="#3a3740"/>
                <text x="129" y="423" fill="#e7dafa" font-weight="700" font-size="24">"fff"</text>
                <text x="1312" y="422" fill="#a6a4ae" text-anchor="end" font-size="19">{fff.to_string()} " pull requests"</text>
                {tiles}
            </g>
        )
    }
}

pub fn tile_origin(prs: &[PullRequest], index: usize) -> (f32, f32) {
    let repo = &prs[index].repo;
    let group = usize::from(repo == "fff");
    let rank = prs[..index].iter().filter(|pr| pr.repo == *repo).count();
    let (x, y) = tile_offset(group, rank);
    (GRID_LEFT + x as f32, GRID_TOP + y as f32)
}

pub fn tile_badge(pr: &PullRequest) -> Svgr<'static> {
    let (_, fill, text_color) = REPOSITORIES[usize::from(pr.repo == "fff")];
    let number = format!("#{}", pr.number);
    svgr!(
        <g>
            <rect width={TILE_WIDTH} height={TILE_HEIGHT} rx="4" fill={fill}/>
            <text x="21.5" y="14.4" font-family="JetBrains Mono" font-size="10.8"
                  text-anchor="middle" fill={text_color}>{number}</text>
        </g>
    )
}

fn tile_offset(group: usize, rank: usize) -> (usize, usize) {
    (
        rank % COLUMNS * 47,
        rank / COLUMNS * 25 + group * SECOND_ROW,
    )
}

fn render_grid(
    ctx: &FFramesContext<'_, '_>,
    cache: &OnceLock<ImageData<'static>>,
    prs: &[PullRequest],
    selected: bool,
) -> Option<ImageData<'static>> {
    if let Some(image) = cache.get() {
        return Some(image.clone());
    }
    let tiles = REPOSITORIES
        .iter()
        .enumerate()
        .map(|(group, &(repo, _, _))| {
            prs.iter()
                .filter(|pr| pr.repo == repo)
                .enumerate()
                .filter(|(_, pr)| pr.dusted == selected)
                .map(|(i, pr)| {
                    let (x, y) = tile_offset(group, i);
                    svgr!(
                        <g transform={format!("translate({x} {y})")}>
                            {tile_badge(pr)}
                        </g>
                    )
                })
                .collect::<Svgr>()
        })
        .collect::<Svgr>();
    let tree = svgr!(
        <svg xmlns="http://www.w3.org/2000/svg" width={GRID_WIDTH} height={GRID_HEIGHT}>{tiles}</svg>
    );
    let image = ctx.render_svgr(tree, GRID_WIDTH, GRID_HEIGHT)?;
    Some(cache.get_or_init(|| image).clone())
}
