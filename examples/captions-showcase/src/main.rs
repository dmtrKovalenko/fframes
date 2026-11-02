//! Cinematic kinetic typography. Speech, fonts and word timings use the `FFrames` media loader.
mod annotations;
mod editorial;
mod montage;
mod motion;
mod word_effects;

use std::{path::PathBuf, process::ExitCode};

use fframes::{
    AudioMap,
    AudioTimestamp::{Eof, Second},
    AudioTrack, Color, Duration, EncoderOptions, FFramesContext, Frame, MediaDirectory,
    MediaProcessingOptions, MediaProvider, RenderOptions, Svgr, Video,
    captions::{CaptionError, CaptionTrack},
    cli::{self, clap},
    svgr,
};
use motion::DirectedCaption;
use word_effects::WordEffects;

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum Style {
    Showcase,
    #[value(alias = "flame")]
    Plasma,
    #[value(alias = "space")]
    Nebula,
    #[value(alias = "sparkles")]
    Diffraction,
    #[value(alias = "outline")]
    Etch,
    #[value(alias = "juggle")]
    Interference,
    #[value(alias = "lightning")]
    Discharge,
    #[value(alias = "comet")]
    Shear,
    #[value(alias = "liquid")]
    Mercury,
    #[value(alias = "confetti")]
    Fracture,
    #[value(alias = "neon")]
    ScanGlass,
    #[value(alias = "ink")]
    Engraving,
    #[value(alias = "shockwave")]
    Wavefront,
    #[value(alias = "chrome")]
    Quartz,
    #[value(alias = "frost")]
    Crystal,
    #[value(alias = "ribbon")]
    ThinFilm,
    #[value(alias = "gold")]
    Horizon,
    Astral,
    Combustion,
    Fluid,
    Raster,
    Contour,
    Prism,
    Lattice,
    Flux,
    Stress,
    Lamella,
    Photonic,
    LiquidMetal,
    Irradiance,
    Goo,
    GlassNebula,
}

impl Style {
    fn effect_override(self) -> Option<usize> {
        match self {
            Self::Showcase => None,
            Self::Plasma => Some(0),
            Self::Nebula => Some(1),
            Self::Diffraction => Some(2),
            Self::Etch => Some(3),
            Self::Interference => Some(4),
            Self::Discharge => Some(5),
            Self::Shear => Some(6),
            Self::Mercury => Some(7),
            Self::Fracture => Some(8),
            Self::ScanGlass => Some(9),
            Self::Engraving => Some(10),
            Self::Wavefront => Some(11),
            Self::Quartz => Some(12),
            Self::Crystal => Some(13),
            Self::ThinFilm => Some(14),
            Self::Horizon => Some(15),
            Self::Astral => Some(16),
            Self::Combustion => Some(17),
            Self::Fluid => Some(18),
            Self::Raster => Some(19),
            Self::Contour => Some(20),
            Self::Prism => Some(21),
            Self::Lattice => Some(22),
            Self::Flux => Some(23),
            Self::Stress => Some(24),
            Self::Lamella => Some(25),
            Self::Photonic => Some(26),
            Self::LiquidMetal => Some(27),
            Self::Irradiance => Some(28),
            Self::Goo => Some(29),
            Self::GlassNebula => Some(30),
        }
    }
}

#[derive(Debug, clap::Args)]
struct CaptionArgs {
    #[arg(long, global = true, default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/media"))]
    media_dir: PathBuf,
    #[arg(long, global = true, default_value = "moon.json")]
    track: String,
    #[arg(long, global = true, default_value = "kennedy-moon.wav")]
    voice: String,
    /// Follow the authored motion score, or use one effect for every word.
    #[arg(long, global = true, value_enum, default_value = "showcase")]
    style: Style,
    /// Optical materials, plasma and precision light shaders; requires --features skia.
    #[arg(long, global = true)]
    shader: bool,
    #[arg(long, global = true)]
    silent: bool,
    /// Render the typography on a dark background without external footage or audio.
    #[arg(long, global = true)]
    captions_only: bool,
    /// Keep only the original Kennedy recording, without the original synth score.
    #[arg(long, global = true)]
    voice_only: bool,
    /// Parallel Skia CPU render workers. GPU rendering keeps a single context.
    #[arg(long, global = true, default_value_t = 3, value_parser = clap::value_parser!(u8).range(1..=8))]
    render_workers: u8,
}

struct CaptionVideo {
    captions: Vec<DirectedCaption>,
    effects: WordEffects,
    style: Style,
    audio: String,
    silent: bool,
    score: bool,
    shaders: bool,
    captions_only: bool,
    seconds: f32,
    output: Option<editorial::OutputPolicy>,
}

impl CaptionVideo {
    fn new(
        args: &CaptionArgs,
        seconds: f32,
        track: &CaptionTrack,
        output: Option<editorial::OutputPolicy>,
    ) -> Result<Self, CaptionError> {
        Ok(Self {
            captions: track
                .cues()
                .iter()
                .cloned()
                .map(DirectedCaption::new)
                .collect::<Result<Vec<_>, _>>()?,
            effects: WordEffects::new(args.shader),
            style: args.style,
            audio: args.voice.clone(),
            silent: args.silent || args.captions_only,
            score: !args.voice_only,
            shaders: args.shader,
            captions_only: args.captions_only,
            seconds,
            output,
        })
    }
}

impl Video for CaptionVideo {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;
    const BACKGROUND_COLOR: Color = Color::BLACK;
    fn duration(&self) -> Duration<'_> {
        if let Some(output) = self.output {
            Duration::Frames((output.end_seconds * Self::FPS as f64).ceil() as usize)
        } else {
            Duration::Seconds(self.seconds)
        }
    }
    fn audio(&self) -> AudioMap<'_> {
        if self.silent {
            return AudioMap::none();
        }
        let mut tracks = vec![
            AudioTrack::new(
                &self.audio,
                Second(0.0)..self.output.map_or(Eof, |_| Second(self.seconds)),
            )
            .highpass(65.0)
            .normalize_lufs(-16.0)
            .fade_in(0.025)
            .fade_out(
                self.output
                    .map_or(0.22, |output| output.voice_fade_seconds as f32),
            )
            .voice(),
        ];
        if self.score {
            tracks.push(
                AudioTrack::new("mission-score.wav", Second(0.0)..Second(self.seconds))
                    .normalize_lufs(-29.0)
                    .fade_in(0.1)
                    .fade_out(
                        self.output
                            .map_or(0.7, |output| output.music_fade_seconds as f32),
                    ),
            );
        }
        AudioMap::from(tracks)
    }
    fn render_frame<'a>(&'a self, mut frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.global_index as f64 / Self::FPS as f64;
        let captions = self
            .captions
            .iter()
            .filter(|cue| cue.active_at(t))
            .map(|cue| cue.render(&mut frame, ctx, &self.effects, self.style.effect_override()))
            .collect::<Vec<_>>();
        let environment = if self.captions_only {
            Svgr::empty()
        } else {
            montage::render(&frame, ctx, self.shaders)
        };
        let fade_seconds = self
            .output
            .map_or(0.65, |output| output.picture_fade_seconds);
        let fade = if fade_seconds > 0.0 {
            ((f64::from(self.seconds) - t) / fade_seconds).clamp(0.0, 1.0)
        } else {
            1.0
        };
        svgr!(<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080" viewBox="0 0 1920 1080">
            <rect width="1920" height="1080" fill="#010307" />
            <g opacity={fade}>
                {environment}
                {captions}
            </g>
        </svg>)
    }
}

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let args = cli::parse::<CaptionArgs>();
    if args.app.shader && !cfg!(feature = "skia") {
        return Err("--shader requires --features skia".into());
    }
    if args.app.shader && args.renderer == Some(cli::Renderer::Cpu) {
        return Err("--shader requires --renderer skia-cpu or gpu".into());
    }
    let directory = MediaDirectory::read_folder(&args.app.media_dir)?;
    let media = directory.process_media_source_with_options(MediaProcessingOptions {
        decode_video_audio: false,
    })?;
    let track = media
        .resolve_subtitles(&args.app.track)
        .ok_or("selected subtitles are missing")?;
    let track_end = track.cues().iter().map(|cue| cue.end).fold(0.0, f64::max);
    let output = editorial::output_policy(track)?;
    let legacy_seconds = if args.app.silent || args.app.captions_only {
        track_end as f32
    } else {
        let audio = media
            .resolve_audio(&args.app.voice)
            .ok_or("selected audio is missing")?;
        (track_end as f32).max(audio.duration_in_seconds())
    };
    let seconds = output.map_or(legacy_seconds, |output| output.end_seconds as f32);
    if !args.app.captions_only {
        montage::validate(&media, seconds)?;
    }
    if !args.app.silent
        && !args.app.captions_only
        && !args.app.voice_only
        && media.resolve_audio("mission-score.wav").is_none()
    {
        return Err(
            "mission-score.wav is missing; use --voice-only to render just the speech".into(),
        );
    }
    let video = CaptionVideo::new(&args.app, seconds, track, output)?;
    #[cfg(feature = "skia")]
    let pipeline = fframes_skia_renderer::SkiaPipelineConfig {
        concurrency_policy: if args.renderer == Some(cli::Renderer::Gpu) {
            fframes_skia_renderer::SkiaPipelineConcurrencyPolicy::OnePipeline
        } else {
            fframes_skia_renderer::SkiaPipelineConcurrencyPolicy::Concurrency(usize::from(
                args.app.render_workers,
            ))
        },
        encoder_threads: 4,
        buffer_queue_size: 3,
        ..Default::default()
    };
    let runner = cli::new(
        &video,
        RenderOptions {
            media: Some(&media),
            video_encoder_options: EncoderOptions {
                preferred_encoder: Some("libx264"),
                codec_params: Some(&[("crf", "18"), ("preset", "medium")]),
                color_matrix: fframes::YuvMatrix::Bt709,
                ..EncoderOptions::default()
            },
            ..Default::default()
        },
    )
    .args(args)
    .default_output("words-in-orbit.mp4");
    #[cfg(feature = "skia")]
    let runner = runner.renderers(fframes_skia_renderer::cli::SkiaRenderers {
        default: Some(cli::Renderer::SkiaCpu),
        pipeline,
    });
    Ok(runner.run())
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("captions: {error}");
            ExitCode::FAILURE
        }
    }
}
