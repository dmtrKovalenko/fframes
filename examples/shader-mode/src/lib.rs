//! A source-frame-matched shader promo rendered by Skia, with native preview recordings.

mod catalog;
mod edit;
mod outro;
mod titles;
mod transitions;

use fframes::{
    AudioMap, AudioTimestamp, AudioTrack, Color, Duration, FFramesContext, FFramesSyncedVideoFrame,
    Frame, Scene, Scenes, Shader, ShaderUniforms, Svgr, SyncVideoFrameInput, Video,
};

fframes::include_media_dir!(pub struct ShaderModeMedia, "examples/shader-mode/media");

const COMMON: &str = include_str!("shaders/common.sksl");
const SOURCE_EFFECTS: &str = include_str!("shaders/source-effects.sksl");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Effect {
    Ascii,
    Glass,
    Material,
    LiquidChrome,
    Tunnel,
    Halftone,
    Gradient,
    Particles,
    Obsidian,
    Aurora,
    Flow,
    Emboss,
    Rays,
    Spectral,
    Eclipse,
    Catalog,
    Preview,
    Binary,
}

impl Effect {
    const ALL: [Self; 18] = [
        Self::Ascii,
        Self::Glass,
        Self::Material,
        Self::LiquidChrome,
        Self::Tunnel,
        Self::Halftone,
        Self::Gradient,
        Self::Particles,
        Self::Obsidian,
        Self::Aurora,
        Self::Flow,
        Self::Emboss,
        Self::Rays,
        Self::Spectral,
        Self::Eclipse,
        Self::Catalog,
        Self::Preview,
        Self::Binary,
    ];

    fn source(self) -> &'static str {
        match self {
            Self::Ascii => include_str!("shaders/ascii.sksl"),
            Self::Glass => include_str!("shaders/glass.sksl"),
            Self::Material => include_str!("shaders/material.sksl"),
            Self::LiquidChrome => include_str!("shaders/liquid-chrome.sksl"),
            Self::Tunnel => include_str!("shaders/tunnel.glsl"),
            Self::Halftone => include_str!("shaders/halftone.sksl"),
            Self::Gradient => include_str!("shaders/gradient.sksl"),
            Self::Particles => include_str!("shaders/particles.sksl"),
            Self::Obsidian => include_str!("shaders/obsidian.sksl"),
            Self::Aurora => include_str!("shaders/aurora.sksl"),
            Self::Flow => include_str!("shaders/flow.sksl"),
            Self::Emboss => include_str!("shaders/emboss.sksl"),
            Self::Rays => include_str!("shaders/rays.sksl"),
            Self::Spectral => include_str!("shaders/spectral.sksl"),
            Self::Eclipse => include_str!("shaders/eclipse.sksl"),
            Self::Catalog => include_str!("shaders/catalog.sksl"),
            Self::Preview => include_str!("shaders/preview.sksl"),
            Self::Binary => include_str!("shaders/binary-intro.sksl"),
        }
    }

    fn mask(self) -> Option<&'static str> {
        match self {
            Self::Ascii => Some("glyph-atlas.png"),
            Self::Binary => Some("binary-atlas.png"),
            Self::Halftone | Self::Emboss => Some("fframes-sdf.png"),
            Self::Catalog => Some("catalog.png"),
            _ => None,
        }
    }

    fn shader(self) -> Shader {
        if self == Self::Tunnel {
            Shader::shadertoy(format!(
                "uniform float uClock;\n{}",
                self.source().replace("iTime", "uClock")
            ))
        } else {
            let sculpture = if matches!(self, Self::LiquidChrome | Self::Obsidian) {
                include_str!("shaders/sculpture.sksl")
            } else {
                ""
            };
            let gradient = if matches!(self, Self::Gradient | Self::Catalog) {
                include_str!("shaders/gradient-shared.sksl")
            } else {
                ""
            };
            Shader::sksl(
                format!(
                    "{COMMON}\n{SOURCE_EFFECTS}\n{sculpture}\n{gradient}\n{}",
                    self.source()
                )
                .replace("iTime", "uClock"),
            )
        }
    }
}

use edit::{SHOTS, ShotSpec};

#[derive(Debug)]
struct Shot {
    spec: ShotSpec,
    shader: Option<Shader>,
    outro: Option<outro::Outro>,
    opening_overlay: Option<Shader>,
    catalog: Option<catalog::Catalog>,
}

impl Scene for Shot {
    fn name(&self) -> &'static str {
        self.spec.name
    }

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(self.spec.frames())
    }

    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let Some(shader) = &self.shader else {
            return Svgr::empty();
        };
        let progress = frame.index as f32 / self.spec.frames() as f32;
        let source_index = frame.global_index;
        // Keep geometry continuous through color/material cuts, while progress is
        // local to the edit segment. Use uClock for the source's actual rational clock.
        frame.index = source_index - self.spec.clock_start;
        if let Some(catalog) = &self.catalog {
            return transitions::apply(
                source_index,
                self.spec.name,
                catalog.render(&frame, ctx, shader),
            );
        }
        let seconds =
            frame.index as f32 * edit::SOURCE_FPS_DEN as f32 / edit::SOURCE_FPS_NUM as f32;
        let mut uniforms = ShaderUniforms::new()
            .float("uClock", seconds)
            .float("uProgress", progress)
            .float("uVariant", f32::from(self.spec.variant))
            .float("uBeat", (-(seconds % 0.5) * 12.0).exp());
        if let Some(name) = self.spec.effect.and_then(Effect::mask) {
            let Some(mask) = ctx.get_image(name) else {
                return Svgr::empty();
            };
            uniforms = uniforms.image("uMask", mask);
        }
        if self.spec.effect == Some(Effect::Preview) {
            let file = "native-scrubbing.mp4";
            let mut recording_frame = frame.clone();
            recording_frame.index = source_index - 496;
            let input = SyncVideoFrameInput {
                looping: false,
                ..Default::default()
            };
            let recording = ctx
                .media_source
                .and_then(|media| media.resolve_video(file))
                .and_then(|_| recording_frame.get_synced_video_frame(ctx, file, &input));
            if let Some(recording) = recording {
                let image = recording.into_image();
                uniforms = uniforms.image("uRecording", &image).float2(
                    "uRecordingSize",
                    recording.width() as f32,
                    recording.height() as f32,
                );
            } else if let Some(poster) = ctx.get_image("native-preview-poster.jpg") {
                // A poster keeps shader inspection and source-only builds useful.
                // The reference export tool requires the actual recording.
                uniforms =
                    uniforms
                        .image("uRecording", poster)
                        .float2("uRecordingSize", 1920.0, 1124.0);
            } else {
                return Svgr::empty();
            }
        }
        let layer = shader.draw(&frame, uniforms);
        let title_name = match self.spec.name {
            "GlassAmber" => "Glass",
            "OpenSourceHold" => "OpenSource",
            "ShaderModeEditor" | "ShaderModeZoom" | "ShaderModeWide" => "ShaderMode",
            other => other,
        };
        let titles = match &self.outro {
            Some(outro) => outro.render(&mut frame, ctx),
            None => titles::render(title_name, &frame),
        };
        let titles = if self.spec.name == "Glass" && source_index < 35 {
            let opacity = ((source_index as f32 - 31.0) / 4.0).clamp(0.0, 1.0);
            fframes::svgr!(<g opacity={opacity}>{titles}</g>)
        } else {
            titles
        };
        let composition = fframes::svgr!(
            <g>
                <image href={layer.href()} x="0" y="0" width="1920" height="1080" />
                {titles}
            </g>
        );
        let composition = transitions::apply(source_index, self.spec.name, composition);
        if source_index < 33
            && let Some(opening) = &self.opening_overlay
            && let Some(atlas) = ctx.get_image("binary-atlas.png")
        {
            let layer = opening.draw(
                &frame,
                ShaderUniforms::new()
                    .float("uClock", (source_index - 6) as f32 * 1001.0 / 30000.0)
                    .float("uVariant", 1.0)
                    .image("uMask", atlas),
            );
            return fframes::svgr!(
                <g>
                    {composition}
                    <image href={layer.href()} width="1920" height="1080" />
                </g>
            );
        }
        composition
    }
}

/// Native Skia showcase: 932 source frames, 1920×1080.
/// Export with `tools/render_reference.py` for 30000/1001 fps and untouched AAC.
#[derive(Debug)]
pub struct ShaderMode {
    shots: Vec<Shot>,
}

impl ShaderMode {
    /// Creates shader definitions once; Skia compiles and caches them on first use.
    pub fn new() -> Self {
        let shaders = Effect::ALL.map(Effect::shader);
        let shots = SHOTS
            .iter()
            .copied()
            .map(|spec| Shot {
                shader: spec.effect.map(|effect| shaders[effect as usize].clone()),
                outro: (spec.name == "Eclipse").then(outro::Outro::default),
                opening_overlay: (spec.name == "Glass")
                    .then(|| shaders[Effect::Binary as usize].clone()),
                catalog: (spec.effect == Some(Effect::Catalog)).then(catalog::Catalog::new),
                spec,
            })
            .collect();
        Self { shots }
    }
}

impl Default for ShaderMode {
    fn default() -> Self {
        Self::new()
    }
}

impl Video for ShaderMode {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(edit::SOURCE_FRAMES)
    }

    fn define_scenes(&self) -> Scenes<'_> {
        Scenes::from(
            self.shots
                .iter()
                .map(|shot| shot as &dyn Scene)
                .collect::<Vec<_>>(),
        )
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::from([AudioTrack::new(
            "reference-audio.wav",
            AudioTimestamp::Second(0.0)..AudioTimestamp::Eof,
        )])
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080">
                <rect width="1920" height="1080" fill="#000" />
                {ctx.render_scenes(&frame)}
            </svg>
        )
    }
}
