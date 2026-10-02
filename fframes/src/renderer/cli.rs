//! A ready-made command line for video binaries (feature `cli`). One call gives every video
//! the same tools for rendering and for checking the result without watching it:
//!
//! ```ignore
//! fn main() -> std::process::ExitCode {
//!     let media = MyMedia::prepare().unwrap();
//!     let video = MyVideo { media: &media };
//!     fframes::cli::new(&video, RenderOptions { media: Some(&media), ..Default::default() }).run()
//! }
//! ```
//!
//! Everything else is optional: `.backend(..)` renders with another backend (the Skia renderer;
//! previews then use it too), `.preview(fframes_native_player::cli_preview)` adds the real-time
//! window, `.default_output("out.webm")` changes the file `render` writes. Flags of your own go
//! into a `#[derive(clap::Args)]` struct: `let args = cli::parse::<MyArgs>();`, build the video
//! from `args.app`, then `cli::new(&video, options).args(args).run()`.
use super::{
    CpuFrameRenderer, FFramesRenderBackend, FrameRenderer, FrameReport, Previewer, RgbaFrame,
    fframes_logger::FFramesLoggerVariant, sheet, snapshot,
};
use crate::diagnostics::Severity;
use crate::{AudioMixer, AudioTimelineSamples, AudioTimelineUnit, RenderOptions, Video};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use std::fmt::Write as _;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

pub use clap;

const TIME_SPECS: &str = "\
TIME SPECS (for --at, ranges and positional times):
  120, 120f        frame 120               3.2s, 500ms, 1:05.5   a timestamp
  50%              half of the video       start, end            first / last frame
  Intro            first frame of a scene (case-insensitive)   #3   scene index 3
  Intro[1]         second scene of type Intro
  Intro@1.2s       1.2s into the scene (also @12, @50%, @end)
RANGES: a..b (end exclusive), a.., ..b, all, or a scene name for the whole scene.
Run `timeline` to list scenes. Add --json to any command for machine-readable output.";

/// No application specific flags.
#[derive(Debug, Clone, Default, Args)]
pub struct NoArgs {}

#[derive(Debug, Parser)]
#[command(about = "Render and inspect this fframes video", after_long_help = TIME_SPECS, after_help = TIME_SPECS)]
pub struct Cli<A: Args = NoArgs> {
    #[command(flatten)]
    pub app: A,

    #[command(subcommand)]
    pub command: Option<Command>,

    /// Print one JSON document to stdout; progress and warnings go to stderr.
    #[arg(long, global = true)]
    pub json: bool,

    /// Output resolution factor, e.g. 0.5 for half resolution.
    #[arg(long, global = true)]
    pub scale: Option<f64>,
}

impl<A: Args> Cli<A> {
    pub fn parse() -> Self {
        <Self as Parser>::parse()
    }
}

/// Parses the command line with application specific flags `A` (available as `.app`). Needed
/// only when the video is built from those flags; pass the result to `Runner::args`.
pub fn parse<A: Args>() -> Cli<A> {
    Cli::<A>::parse()
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Render the video, or a range of it, to a file (default command).
    Render(RenderArgs),
    /// Render single frames to PNG and report problems found in them.
    Frame(FrameArgs),
    /// A labelled contact sheet of evenly spaced frames: one image to review motion.
    Strip(StripArgs),
    /// Blend several frames into one image (later frames stronger) to see a movement's path.
    Onion(OnionArgs),
    /// Print a frame as SVG after conversion (text laid out, styles resolved).
    Svg(SvgArgs),
    /// Scenes, duration and audio tracks.
    Timeline,
    /// Check frames for missing media and fonts, clipped text and panics without rendering.
    Inspect(InspectArgs),
    /// Compare frames with approved PNG snapshots (visual regression).
    Snapshot(SnapshotArgs),
    /// Render, measure and query the audio mix.
    #[command(subcommand)]
    Audio(AudioCommand),
    /// Play the video in real time in a native GPU window (needs `fframes_native_player`).
    Preview(PreviewArgs),
}

#[derive(Debug, Clone, Args)]
pub struct PreviewArgs {
    /// Where to start, e.g. `Intro`, `12s`.
    #[arg(default_value = "start")]
    pub at: String,
    /// Open paused instead of playing.
    #[arg(long)]
    pub paused: bool,
    /// Stop at the end instead of looping.
    #[arg(long)]
    pub no_loop: bool,
    /// Do not play the audio.
    #[arg(long)]
    pub mute: bool,
    /// Skia backend of the window: auto, metal, vulkan or cpu.
    #[arg(long, default_value = "auto")]
    pub backend: String,
}

#[derive(Debug, Clone, Args, Default)]
pub struct RenderArgs {
    /// Only render this range, e.g. `Intro`, `10s..20s`.
    pub range: Option<String>,
    /// Output file, `out.mp4` unless the video sets another default.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
    /// Fast low quality preview: half resolution (unless --scale) and fastest encoder preset.
    #[arg(long)]
    pub draft: bool,
}

#[derive(Debug, Clone, Args)]
pub struct FrameArgs {
    /// Frames to render (comma or space separated time specs).
    #[arg(required = true, value_delimiter = ',', num_args = 1..)]
    pub at: Vec<String>,
    #[arg(short, long, default_value = "frames")]
    pub output: PathBuf,
    /// Also write the converted SVG next to every PNG.
    #[arg(long)]
    pub svg: bool,
}

#[derive(Debug, Clone, Args)]
pub struct StripArgs {
    /// Range to sample.
    #[arg(default_value = "all")]
    pub range: String,
    /// Number of frames.
    #[arg(short = 'n', long, default_value_t = 12)]
    pub count: usize,
    #[arg(long, default_value_t = 4)]
    pub columns: usize,
    /// Width of one frame in pixels.
    #[arg(long, default_value_t = 480)]
    pub width: u32,
    #[arg(short, long, default_value = "strip.png")]
    pub output: PathBuf,
}

#[derive(Debug, Clone, Args)]
pub struct OnionArgs {
    /// Range to sample.
    pub range: String,
    #[arg(short = 'n', long, default_value_t = 6)]
    pub count: usize,
    #[arg(short, long, default_value = "onion.png")]
    pub output: PathBuf,
}

#[derive(Debug, Clone, Args)]
pub struct SvgArgs {
    pub at: String,
    /// Write to a file instead of stdout.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum FailOn {
    Error,
    Warning,
    Never,
}

#[derive(Debug, Clone, Args)]
pub struct InspectArgs {
    #[arg(default_value = "all")]
    pub range: String,
    /// Distance between checked frames (every scene's first and last frame are always checked).
    #[arg(long, default_value = "0.25s")]
    pub every: String,
    /// Check every single frame.
    #[arg(long)]
    pub all_frames: bool,
    /// Report frames entirely off canvas and other informational findings too.
    #[arg(long)]
    pub info: bool,
    /// Exit with code 2 when a finding of this severity is found.
    #[arg(long, value_enum, default_value = "error")]
    pub fail_on: FailOn,
}

#[derive(Debug, Clone, Args)]
pub struct SnapshotArgs {
    /// Frames to compare.
    #[arg(required = true, value_delimiter = ',', num_args = 1..)]
    pub at: Vec<String>,
    #[arg(long, default_value = "_frame_snapshots")]
    pub dir: PathBuf,
    /// Accept the current frames as the new snapshots.
    #[arg(long)]
    pub update: bool,
    /// Per channel difference (0-255) that still counts as equal.
    #[arg(long, default_value_t = 16)]
    pub threshold: u8,
    /// Fraction of pixels allowed to differ.
    #[arg(long, default_value_t = 0.001)]
    pub max_diff: f64,
}

#[derive(Debug, Clone, Subcommand)]
pub enum AudioCommand {
    /// Mix the audio (or a range of it) to a stereo WAV file.
    Render {
        range: Option<String>,
        #[arg(short, long, default_value = "audio.wav")]
        output: PathBuf,
        /// 32-bit float instead of 16-bit PCM.
        #[arg(long)]
        float: bool,
    },
    /// Loudness (LUFS), true peak, clipping, silence and loudness per scene.
    Analyze {
        range: Option<String>,
        /// Also draw the waveform with loudness, scenes and cues to a PNG.
        #[arg(long)]
        waveform: Option<PathBuf>,
    },
    /// Which tracks play at the given times, where in the file and how loud.
    At {
        #[arg(required = true, value_delimiter = ',', num_args = 1..)]
        at: Vec<String>,
    },
}

type CliResult<T = ExitCode> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Opens the real-time player for the `preview` command, see `Runner::preview`.
type Player<'r, 'a, 'media, V> = Box<
    dyn FnOnce(&'a V, &RenderOptions<'a, 'media>, &super::PreviewRequest) -> Result<(), String>
        + 'r,
>;

/// The command line of one video, created with `cli::new`.
pub struct Runner<'r, 'a, 'media, V, B = crate::cpu::CpuRenderingBackend, A: Args = NoArgs> {
    video: &'a V,
    options: RenderOptions<'a, 'media>,
    backend: B,
    args: Option<Cli<A>>,
    player: Option<Player<'r, 'a, 'media, V>>,
    default_output: PathBuf,
}

/// The command line for `video`: render, frame, strip, inspect, audio, ... Call `.run()`.
pub fn new<'a, 'media: 'a, V: Video + Send + Sync>(
    video: &'a V,
    options: RenderOptions<'a, 'media>,
) -> Runner<'static, 'a, 'media, V> {
    Runner {
        video,
        options,
        backend: crate::cpu::CpuRenderingBackend::default(),
        args: None,
        player: None,
        default_output: PathBuf::from("out.mp4"),
    }
}

impl<'r, 'a, 'media: 'a, V: Video + Send + Sync, B: FFramesRenderBackend, A: Args>
    Runner<'r, 'a, 'media, V, B, A>
{
    /// Renders with another backend, e.g. `SkiaFFramesRenderer`. Frame previews use the
    /// backend's own renderer, so they look like the rendered video.
    pub fn backend<B2: FFramesRenderBackend>(
        self,
        backend: B2,
    ) -> Runner<'r, 'a, 'media, V, B2, A> {
        Runner {
            video: self.video,
            options: self.options,
            backend,
            args: self.args,
            player: self.player,
            default_output: self.default_output,
        }
    }

    /// Uses already parsed arguments with application flags (see `cli::parse`).
    pub fn args<A2: Args>(self, args: Cli<A2>) -> Runner<'r, 'a, 'media, V, B, A2> {
        Runner {
            video: self.video,
            options: self.options,
            backend: self.backend,
            args: Some(args),
            player: self.player,
            default_output: self.default_output,
        }
    }

    /// Enables the `preview` command, usually with `fframes_native_player::cli_preview`.
    pub fn preview<'r2>(
        self,
        play: impl FnOnce(
            &'a V,
            &RenderOptions<'a, 'media>,
            &super::PreviewRequest,
        ) -> Result<(), String>
        + 'r2,
    ) -> Runner<'r2, 'a, 'media, V, B, A> {
        Runner {
            video: self.video,
            options: self.options,
            backend: self.backend,
            args: self.args,
            player: Some(Box::new(play)),
            default_output: self.default_output,
        }
    }

    /// The file `render` writes without `-o`, `out.mp4` by default.
    pub fn default_output(mut self, path: impl Into<PathBuf>) -> Self {
        self.default_output = path.into();
        self
    }

    /// Parses the command line (unless `args` was given) and runs the command.
    pub fn run(self) -> ExitCode {
        let Runner {
            video,
            mut options,
            backend,
            args,
            player,
            default_output,
        } = self;
        let cli = args.unwrap_or_else(Cli::<A>::parse);

        let json = cli.json;
        if let Some(scale) = cli.scale {
            options.scale_resolution = scale;
        }
        if json {
            options.logger = FFramesLoggerVariant::Json;
        } else if !std::io::stderr().is_terminal() {
            options.logger = FFramesLoggerVariant::Lines;
        }
        crate::diagnostics::install_log_capture();

        let command = cli
            .command
            .clone()
            .unwrap_or(Command::Render(RenderArgs::default()));
        let result = match command {
            Command::Render(mut args) => {
                args.output = args.output.or(Some(default_output));
                render(json, cli.scale, video, options, args, backend)
            }
            command => {
                let mut frame_renderer = backend.frame_renderer();
                let mut fallback;
                let frame_renderer: &mut dyn FrameRenderer =
                    if let Some(renderer) = frame_renderer.as_mut() {
                        renderer
                    } else {
                        fallback = CpuFrameRenderer::default();
                        &mut fallback
                    };
                Previewer::new(video, &options)
                    .map_err(err)
                    .and_then(|mut previewer| match command {
                        Command::Frame(args) => frame(json, &mut previewer, frame_renderer, args),
                        Command::Strip(args) => strip(json, &mut previewer, frame_renderer, args),
                        Command::Onion(args) => {
                            onion(json, &mut previewer, frame_renderer, cli.scale, args)
                        }
                        Command::Svg(args) => svg(json, &mut previewer, args),
                        Command::Timeline => timeline(json, &previewer),
                        Command::Inspect(args) => inspect(json, &mut previewer, args),
                        Command::Snapshot(args) => {
                            snapshots(json, &mut previewer, frame_renderer, args)
                        }
                        Command::Audio(args) => audio(json, &previewer, args),
                        Command::Preview(args) => {
                            let Some(play) = player else {
                                return Err("this binary has no real-time player: add the \
                                    `fframes_native_player` crate and \
                                    `.preview(fframes_native_player::cli_preview)`. Without a \
                                    window use `strip`, `frame` or `render --draft`."
                                    .to_owned());
                            };
                            let request = super::PreviewRequest {
                                start_frame: previewer
                                    .timeline()
                                    .resolve_frame(&args.at)
                                    .map_err(err)?,
                                autoplay: !args.paused,
                                looping: !args.no_loop,
                                audio: !args.mute,
                                backend: args.backend,
                            };
                            let options = previewer.options().clone();
                            // Fonts and caches of the previewer are not needed by the player.
                            drop(previewer);
                            play(video, &options, &request).map(|()| ExitCode::SUCCESS)
                        }
                        Command::Render(_) => unreachable!(),
                    })
            }
        };

        match result {
            Ok(code) => code,
            Err(message) => {
                if json {
                    println!("{}", serde_json::json!({ "error": message }));
                } else {
                    eprintln!("error: {message}");
                }
                ExitCode::FAILURE
            }
        }
    }
}

fn print<T: Serialize>(json: bool, value: &T, text: impl FnOnce() -> String) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(value)
                .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
        );
    } else {
        let text = text();
        if !text.is_empty() {
            println!("{text}");
        }
    }
}

fn scenes_label(scenes: &[String]) -> String {
    if scenes.is_empty() {
        String::new()
    } else {
        format!(" [{}]", scenes.join(" + "))
    }
}

/// Warnings and errors of a frame; informational findings only as a count (they are in
/// the --json output).
fn diagnostics_text(report: &FrameReport, indent: &str) -> String {
    let mut text: String = report
        .diagnostics
        .iter()
        .filter(|d| d.severity > Severity::Info)
        .map(|d| format!("{indent}{:?}: {}\n", d.severity, d.message).to_lowercase_first())
        .collect();
    let info = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Info)
        .count();
    if info > 0 {
        let _ = writeln!(
            text,
            "{indent}({info} info findings, e.g. text outside the canvas, see --json)"
        );
    }
    text
}

trait LowercaseFirst {
    fn to_lowercase_first(self) -> String;
}

impl LowercaseFirst for String {
    fn to_lowercase_first(self) -> String {
        let mut chars = self.chars();
        let leading: String = chars.by_ref().take_while(|c| c.is_whitespace()).collect();
        let rest: String = self.chars().skip(leading.len()).collect();
        let mut rest_chars = rest.chars();
        match rest_chars.next() {
            Some(first) => format!("{leading}{}{}", first.to_lowercase(), rest_chars.as_str()),
            None => self,
        }
    }
}

fn resolve_frames<'a, 'm: 'a, V: Video>(
    previewer: &Previewer<'a, 'm, V>,
    specs: &[String],
) -> CliResult<Vec<(String, usize)>> {
    specs
        .iter()
        .flat_map(|spec| spec.split_whitespace())
        .map(|spec| {
            previewer
                .timeline()
                .resolve_frame(spec)
                .map(|frame| (spec.to_owned(), frame))
                .map_err(err)
        })
        .collect()
}

/// `count` frames evenly spread over a range, first and last included.
fn spread(range: std::ops::Range<usize>, count: usize) -> Vec<usize> {
    let count = count.clamp(1, range.len());
    if count == 1 {
        return vec![range.start];
    }
    let last = range.end - 1;
    (0..count)
        .map(|i| {
            range.start
                + ((last - range.start) as f64 * i as f64 / (count - 1) as f64).round() as usize
        })
        .collect()
}

fn ensure_parent(path: &Path) -> CliResult<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent).map_err(err)?;
    }
    Ok(())
}

#[derive(Serialize)]
struct RenderResult {
    output: PathBuf,
    frames: std::ops::Range<usize>,
    start_seconds: f32,
    end_seconds: f32,
    width: usize,
    height: usize,
    elapsed_seconds: f32,
}

fn render<'a, 'media: 'a, V: Video + Send + Sync, B: FFramesRenderBackend>(
    json: bool,
    scale: Option<f64>,
    video: &'a V,
    mut options: RenderOptions<'a, 'media>,
    args: RenderArgs,
    backend: B,
) -> CliResult {
    let started = Instant::now();
    let previewer = Previewer::new(video, &options).map_err(err)?;
    let timeline = previewer.timeline().clone();
    drop(previewer);

    let range = match &args.range {
        Some(spec) => timeline.resolve_range(spec).map_err(err)?,
        None => timeline.full_range(),
    };
    options.frame_range = Some(range.clone());

    if args.draft {
        if scale.is_none() {
            options.scale_resolution = 0.5;
        }
        let encoder = options.video_encoder_options.preferred_encoder;
        if matches!(encoder, None | Some("libx264" | "libx265")) {
            options.video_encoder_options.codec_params =
                Some(&[("preset", "ultrafast"), ("crf", "30")]);
        }
    }

    let output = args.output.unwrap_or_else(|| PathBuf::from("out.mp4"));
    ensure_parent(&output)?;
    super::render(&output, video, backend, &options).map_err(err)?;

    let size = crate::VideoSize::new_scaled(V::WIDTH, V::HEIGHT, options.scale_resolution);
    let result = RenderResult {
        output,
        start_seconds: timeline.frame_to_seconds(range.start),
        end_seconds: timeline.frame_to_seconds(range.end),
        frames: range,
        width: size.width,
        height: size.height,
        elapsed_seconds: started.elapsed().as_secs_f32(),
    };
    print(json, &result, || {
        format!(
            "{} frames {}..{} ({:.2}s..{:.2}s) {}x{} in {:.1}s",
            result.output.display(),
            result.frames.start,
            result.frames.end,
            result.start_seconds,
            result.end_seconds,
            result.width,
            result.height,
            result.elapsed_seconds
        )
    });
    Ok(ExitCode::SUCCESS)
}

#[derive(Serialize)]
struct FrameResult {
    spec: String,
    path: PathBuf,
    svg: Option<PathBuf>,
    #[serde(flatten)]
    report: FrameReport,
}

fn frame<'a, 'm: 'a, V: Video>(
    json: bool,
    previewer: &mut Previewer<'a, 'm, V>,
    renderer: &mut dyn FrameRenderer,
    args: FrameArgs,
) -> CliResult {
    std::fs::create_dir_all(&args.output).map_err(err)?;
    let mut results = Vec::new();
    for (spec, frame) in resolve_frames(previewer, &args.at)? {
        let (pixels, report) = previewer.render_inspected(frame, renderer).map_err(err)?;
        let name = snapshot::snapshot_name(&spec);
        let path = args.output.join(format!("{name}.png"));
        pixels.save_png(&path).map_err(err)?;
        let svg = if args.svg {
            let svg_path = args.output.join(format!("{name}.svg"));
            std::fs::write(&svg_path, previewer.svg(frame).map_err(err)?).map_err(err)?;
            Some(svg_path)
        } else {
            None
        };
        results.push(FrameResult {
            spec,
            path,
            svg,
            report,
        });
    }

    print(json, &results, || {
        results
            .iter()
            .fold(String::new(), |mut text, r| {
                let _ = write!(
                    text,
                    "{} frame {} {:.2}s{} -> {}\n{}",
                    r.spec,
                    r.report.frame,
                    r.report.seconds,
                    scenes_label(&r.report.scenes),
                    r.path.display(),
                    diagnostics_text(&r.report, "  ")
                );
                text
            })
            .trim_end()
            .to_owned()
    });
    Ok(ExitCode::SUCCESS)
}

#[derive(Serialize)]
struct SheetResult {
    output: PathBuf,
    frames: Vec<usize>,
    diagnostics: Vec<FrameReport>,
}

fn render_cells<'a, 'm: 'a, V: Video>(
    previewer: &mut Previewer<'a, 'm, V>,
    renderer: &mut dyn FrameRenderer,
    frames: &[usize],
) -> CliResult<(Vec<sheet::SheetCell>, Vec<FrameReport>)> {
    let mut cells = Vec::new();
    let mut reports = Vec::new();
    for &frame in frames {
        let (pixels, report) = previewer.render_inspected(frame, renderer).map_err(err)?;
        cells.push(sheet::SheetCell {
            frame: pixels,
            label: format!(
                "{:.2}s  f{}{}",
                report.seconds,
                frame,
                scenes_label(&report.scenes)
            ),
        });
        if report
            .diagnostics
            .iter()
            .any(|d| d.severity > Severity::Info)
        {
            reports.push(report);
        }
    }
    Ok((cells, reports))
}

fn sheet_text(result: &SheetResult) -> String {
    let mut text = format!(
        "{} ({} frames: {})",
        result.output.display(),
        result.frames.len(),
        result
            .frames
            .iter()
            .map(std::string::ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );
    for report in &result.diagnostics {
        let _ = write!(text, "\nframe {} {:.2}s:\n", report.frame, report.seconds);
        text.push_str(diagnostics_text(report, "  ").trim_end());
    }
    text
}

fn strip<'a, 'm: 'a, V: Video>(
    json: bool,
    previewer: &mut Previewer<'a, 'm, V>,
    renderer: &mut dyn FrameRenderer,
    args: StripArgs,
) -> CliResult {
    let range = previewer
        .timeline()
        .resolve_range(&args.range)
        .map_err(err)?;
    let frames = spread(range, args.count);
    previewer.set_scale(f64::from(args.width) / V::WIDTH as f64);
    let (cells, diagnostics) = render_cells(previewer, renderer, &frames)?;
    let image = sheet::contact_sheet(
        &cells,
        args.columns,
        previewer.font_db(),
        previewer.options().default_font,
    )
    .map_err(err)?;
    ensure_parent(&args.output)?;
    image.save_png(&args.output).map_err(err)?;

    let result = SheetResult {
        output: args.output,
        frames,
        diagnostics,
    };
    print(json, &result, || sheet_text(&result));
    Ok(ExitCode::SUCCESS)
}

fn onion<'a, 'm: 'a, V: Video>(
    json: bool,
    previewer: &mut Previewer<'a, 'm, V>,
    renderer: &mut dyn FrameRenderer,
    scale: Option<f64>,
    args: OnionArgs,
) -> CliResult {
    let range = previewer
        .timeline()
        .resolve_range(&args.range)
        .map_err(err)?;
    let frames = spread(range, args.count);
    if scale.is_none() {
        previewer.set_scale(0.5);
    }
    let (cells, diagnostics) = render_cells(previewer, renderer, &frames)?;
    let pixels: Vec<RgbaFrame> = cells.into_iter().map(|c| c.frame).collect();
    let image = sheet::onion_skin(&pixels).map_err(err)?;
    ensure_parent(&args.output)?;
    image.save_png(&args.output).map_err(err)?;

    let result = SheetResult {
        output: args.output,
        frames,
        diagnostics,
    };
    print(json, &result, || sheet_text(&result));
    Ok(ExitCode::SUCCESS)
}

fn svg<'a, 'm: 'a, V: Video>(
    json: bool,
    previewer: &mut Previewer<'a, 'm, V>,
    args: SvgArgs,
) -> CliResult {
    let frame = previewer.timeline().resolve_frame(&args.at).map_err(err)?;
    let svg = previewer.svg(frame).map_err(err)?;
    match &args.output {
        Some(path) => {
            ensure_parent(path)?;
            std::fs::write(path, &svg).map_err(err)?;
            print(
                json,
                &serde_json::json!({ "frame": frame, "output": path }),
                || path.display().to_string(),
            );
        }
        None => print(
            json,
            &serde_json::json!({ "frame": frame, "svg": svg }),
            || svg.clone(),
        ),
    }
    Ok(ExitCode::SUCCESS)
}

// Every command shares the `CliResult` signature.
#[allow(clippy::unnecessary_wraps)]
fn timeline<'a, 'm: 'a, V: Video>(json: bool, previewer: &Previewer<'a, 'm, V>) -> CliResult {
    let report = previewer.timeline_report();
    print(json, &report, || {
        let mut text = format!(
            "{}x{} @ {} fps, {} frames ({:.2}s)\n",
            report.width,
            report.height,
            report.fps,
            report.duration_frames,
            report.duration_seconds
        );
        if report.scenes.is_empty() {
            text.push_str("no scenes\n");
        }
        for scene in &report.scenes {
            let _ = writeln!(
                text,
                "#{:<3} {:<28} frames {:>14} {:>18}",
                scene.index,
                scene.name,
                format!("{}..{}", scene.start_frame, scene.end_frame),
                format!("{:.2}s..{:.2}s", scene.start_seconds, scene.end_seconds)
            );
        }
        for track in &report.audio {
            let mix = &track.mix;
            let mut extra = Vec::new();
            if mix.gain_db != 0. {
                extra.push(format!("{:+.1} dB", mix.gain_db));
            }
            if mix.pan != 0. {
                extra.push(format!("pan {:+.2}", mix.pan));
            }
            if mix.fade_in > 0. || mix.fade_out > 0. {
                extra.push(format!("fades {:.2}s/{:.2}s", mix.fade_in, mix.fade_out));
            }
            if mix.offset > 0. {
                extra.push(format!("from {:.2}s of the file", mix.offset));
            }
            if mix.voice {
                extra.push("voice".into());
            }
            if mix.duck.is_some() {
                extra.push("ducked".into());
            }
            let _ = writeln!(
                text,
                "audio {:<28} {:>8.3}s..{:.3}s {}",
                track.file,
                track.start_seconds,
                track.end_seconds,
                extra.join(", ")
            );
        }
        text.trim_end().to_owned()
    });
    Ok(ExitCode::SUCCESS)
}

#[derive(Serialize)]
struct Finding {
    severity: Severity,
    #[serde(skip)]
    key: String,
    /// The message as seen in the first frame.
    message: String,
    first_frame: usize,
    last_frame: usize,
    first_seconds: f32,
    last_seconds: f32,
    /// Number of checked frames it was found in.
    frames: usize,
    scenes: Vec<String>,
}

#[derive(Serialize)]
struct InspectResult {
    checked_frames: usize,
    findings: Vec<Finding>,
}

fn inspect<'a, 'm: 'a, V: Video>(
    json: bool,
    previewer: &mut Previewer<'a, 'm, V>,
    args: InspectArgs,
) -> CliResult {
    let timeline = previewer.timeline().clone();
    let range = timeline.resolve_range(&args.range).map_err(err)?;
    let step = if args.all_frames {
        1
    } else {
        // `every` is a duration: resolve it as an offset from the start.
        timeline
            .resolve_range(&format!("0..{}", args.every))
            .map_err(err)?
            .end
            .max(1)
    };

    let mut frames: Vec<usize> = range.clone().step_by(step).collect();
    for scene in &timeline.scenes {
        for frame in [scene.frames.start, scene.frames.end.saturating_sub(1)] {
            if range.contains(&frame) {
                frames.push(frame);
            }
        }
    }
    frames.push(range.end - 1);
    frames.sort_unstable();
    frames.dedup();

    let mut findings: Vec<Finding> = Vec::new();
    for &frame in &frames {
        let report = previewer.inspect(frame).map_err(err)?;
        for diagnostic in report.diagnostics {
            if diagnostic.severity == Severity::Info && !args.info {
                continue;
            }
            match findings.iter_mut().find(|f| f.key == diagnostic.key) {
                Some(finding) => {
                    finding.last_frame = frame;
                    finding.last_seconds = report.seconds;
                    finding.frames += 1;
                    for scene in &report.scenes {
                        if !finding.scenes.contains(scene) {
                            finding.scenes.push(scene.clone());
                        }
                    }
                }
                None => findings.push(Finding {
                    severity: diagnostic.severity,
                    key: diagnostic.key,
                    message: diagnostic.message,
                    first_frame: frame,
                    last_frame: frame,
                    first_seconds: report.seconds,
                    last_seconds: report.seconds,
                    frames: 1,
                    scenes: report.scenes.clone(),
                }),
            }
        }
    }
    findings.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then(a.first_frame.cmp(&b.first_frame))
    });

    let failed = findings.iter().any(|f| match args.fail_on {
        FailOn::Error => f.severity >= Severity::Error,
        FailOn::Warning => f.severity >= Severity::Warning,
        FailOn::Never => false,
    });

    let result = InspectResult {
        checked_frames: frames.len(),
        findings,
    };
    print(json, &result, || {
        let mut text = format!("checked {} frames: ", result.checked_frames);
        if result.findings.is_empty() {
            text.push_str("no problems found");
        } else {
            let _ = write!(text, "{} findings", result.findings.len());
        }
        for f in &result.findings {
            let _ = write!(
                text,
                "\n{:?} {:.2}s..{:.2}s (frames {}..{}, seen in {}){}: {}",
                f.severity,
                f.first_seconds,
                f.last_seconds,
                f.first_frame,
                f.last_frame,
                f.frames,
                scenes_label(&f.scenes),
                f.message
            );
        }
        text
    });

    Ok(if failed {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    })
}

fn snapshots<'a, 'm: 'a, V: Video>(
    json: bool,
    previewer: &mut Previewer<'a, 'm, V>,
    renderer: &mut dyn FrameRenderer,
    args: SnapshotArgs,
) -> CliResult {
    let options = snapshot::SnapshotOptions {
        directory: args.dir,
        channel_threshold: args.threshold,
        max_diff_ratio: args.max_diff,
        update: args.update || snapshot::SnapshotOptions::default().update,
    };
    let mut results = Vec::new();
    for spec in args.at.iter().flat_map(|s| s.split_whitespace()) {
        results.push(snapshot::check_frame(previewer, renderer, spec, &options).map_err(err)?);
    }
    let failed = results
        .iter()
        .any(|r| r.status == snapshot::SnapshotStatus::Failed);

    print(json, &results, || {
        results
            .iter()
            .map(|r| {
                let mut line = format!("{:?} {} (frame {})", r.status, r.spec, r.frame);
                if r.status == snapshot::SnapshotStatus::Failed {
                    let _ = write!(
                        line,
                        ": {:.3}% of pixels differ, actual {}, diff {}",
                        r.diff_ratio * 100.,
                        r.actual.as_deref().unwrap_or(Path::new("-")).display(),
                        r.diff.as_deref().unwrap_or(Path::new("-")).display()
                    );
                }
                line
            })
            .collect::<Vec<_>>()
            .join("\n")
    });
    Ok(if failed {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    })
}

fn mixer_for<'a, 'm: 'a, V: Video>(
    previewer: &Previewer<'a, 'm, V>,
    range: std::ops::Range<usize>,
) -> (AudioMixer<'m>, std::ops::Range<usize>) {
    let options = previewer.options();
    let time_base = crate::TimeBase {
        fps: V::FPS,
        sample_rate: options.audio_encoder_options.sample_rate,
    };
    let samples = AudioTimelineSamples::from_frames(range.start, &time_base).as_usize()
        ..AudioTimelineSamples::from_frames(range.end, &time_base).as_usize();
    let total =
        AudioTimelineSamples::from_frames(previewer.timeline().duration_in_frames, &time_base)
            .as_usize();
    let mixer = AudioMixer::new(
        previewer.resolved_timeline().audio_map.as_ref(),
        previewer.media(),
        time_base.sample_rate,
        samples.clone(),
        total,
        options.audio_mix,
    );
    (mixer, samples)
}

fn audio<'a, 'm: 'a, V: Video>(
    json: bool,
    previewer: &Previewer<'a, 'm, V>,
    command: AudioCommand,
) -> CliResult {
    let timeline = previewer.timeline().clone();
    let resolve = |range: &Option<String>| match range {
        Some(spec) => timeline.resolve_range(spec).map_err(err),
        None => Ok(timeline.full_range()),
    };
    let sample_rate = previewer.options().audio_encoder_options.sample_rate;

    match command {
        AudioCommand::Render {
            range,
            output,
            float,
        } => {
            let (mut mixer, samples) = mixer_for(previewer, resolve(&range)?);
            let (left, right) = mixer.render_all();
            ensure_parent(&output)?;
            std::fs::write(
                &output,
                crate::encode_wav(&left, &right, sample_rate, float),
            )
            .map_err(err)?;
            print(
                json,
                &serde_json::json!({
                    "output": output,
                    "start_seconds": samples.start as f64 / sample_rate as f64,
                    "end_seconds": samples.end as f64 / sample_rate as f64,
                    "missing_files": mixer.missing_files(),
                }),
                || {
                    let mut text = format!(
                        "{} {:.2}s..{:.2}s",
                        output.display(),
                        samples.start as f64 / sample_rate as f64,
                        samples.end as f64 / sample_rate as f64
                    );
                    for file in mixer.missing_files() {
                        let _ = write!(
                            text,
                            "\nwarning: audio \"{file}\" is not in the media provider"
                        );
                    }
                    text
                },
            );
        }
        AudioCommand::Analyze { range, waveform } => {
            let frames = resolve(&range)?;
            let (mut mixer, samples) = mixer_for(previewer, frames.clone());
            let (left, right) = mixer.render_all();
            let to_samples = |frame: usize| {
                ((frame as f64 / V::FPS as f64 * sample_rate as f64).round() as usize)
                    .clamp(samples.start, samples.end)
                    - samples.start
            };
            let sections: Vec<(String, std::ops::Range<usize>)> = timeline
                .scenes
                .iter()
                .filter(|s| s.frames.start < frames.end && s.frames.end > frames.start)
                .map(|s| {
                    (
                        s.name.clone(),
                        to_samples(s.frames.start)..to_samples(s.frames.end),
                    )
                })
                .collect();
            let mut report = crate::analyze_audio(&left, &right, sample_rate, &sections);
            let offset = samples.start as f64 / sample_rate as f64;
            report.sections.iter_mut().for_each(|s| {
                s.start_seconds += offset;
                s.end_seconds += offset;
            });
            report.silent_ranges.iter_mut().for_each(|r| {
                r[0] += offset;
                r[1] += offset;
            });

            if let Some(path) = &waveform {
                let cues: Vec<(String, f64)> = previewer
                    .timeline_report()
                    .audio
                    .into_iter()
                    .map(|t| (t.file, t.start_seconds - offset))
                    .collect();
                let scenes: Vec<(String, f64)> = sections
                    .iter()
                    .map(|(name, range)| (name.clone(), range.start as f64 / sample_rate as f64))
                    .collect();
                let image = waveform_image(
                    &left,
                    &right,
                    sample_rate,
                    offset,
                    &scenes,
                    &cues,
                    previewer,
                )?;
                ensure_parent(path)?;
                image.save_png(path).map_err(err)?;
            }

            let missing = mixer.missing_files().to_vec();
            print(
                json,
                &serde_json::json!({ "report": report, "waveform": waveform, "missing_files": missing }),
                || audio_report_text(&report, waveform.as_deref(), &missing),
            );
        }
        AudioCommand::At { at } => {
            let (mixer, _) = mixer_for(previewer, timeline.full_range());
            let mut results = Vec::new();
            for (spec, frame) in resolve_frames(previewer, &at)? {
                let sample = (frame as f64 / V::FPS as f64 * sample_rate as f64).round() as usize;
                results.push(serde_json::json!({
                    "spec": spec,
                    "frame": frame,
                    "seconds": frame as f64 / V::FPS as f64,
                    "tracks": mixer.active_tracks_at(sample),
                }));
            }
            print(json, &results, || {
                results
                    .iter()
                    .map(|r| {
                        let mut text = format!(
                            "{} ({:.3}s):",
                            r["spec"].as_str().unwrap_or(""),
                            r["seconds"].as_f64().unwrap_or(0.)
                        );
                        let tracks = r["tracks"].as_array().cloned().unwrap_or_default();
                        if tracks.is_empty() {
                            text.push_str(" silence");
                        }
                        for t in tracks {
                            let _ = write!(
                                text,
                                "\n  {} at {:.3}s of the file, {:.1} dB{}{}",
                                t["file"].as_str().unwrap_or(""),
                                t["file_seconds"].as_f64().unwrap_or(0.),
                                t["gain_db"].as_f64().unwrap_or(f64::NEG_INFINITY),
                                if t["voice"].as_bool() == Some(true) {
                                    ", voice"
                                } else {
                                    ""
                                },
                                match t["ducked_db"].as_f64() {
                                    Some(d) if d < -0.05 => format!(", ducked {d:.1} dB"),
                                    _ => String::new(),
                                }
                            );
                        }
                        text
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn audio_report_text(
    report: &crate::AudioReport,
    waveform: Option<&Path>,
    missing: &[String],
) -> String {
    let db =
        |v: Option<f64>, unit: &str| v.map_or("silent".to_owned(), |v| format!("{v:.1} {unit}"));
    let mut text = format!(
        "integrated {}, range {}, max momentary {}, max short-term {}\n\
         sample peak {}, true peak {}, clipped samples {}",
        db(report.integrated_lufs, "LUFS"),
        report
            .loudness_range_lu
            .map_or("-".to_owned(), |v| format!("{v:.1} LU")),
        db(report.max_momentary_lufs, "LUFS"),
        db(report.max_short_term_lufs, "LUFS"),
        db(report.sample_peak_dbfs, "dBFS"),
        db(report.true_peak_dbtp, "dBTP"),
        report.clipped_samples
    );
    for range in &report.silent_ranges {
        let _ = write!(text, "\nsilent {:.2}s..{:.2}s", range[0], range[1]);
    }
    for section in &report.sections {
        let _ = write!(
            text,
            "\n{:<28} {:>18} {} / peak {}",
            section.name,
            format!("{:.2}s..{:.2}s", section.start_seconds, section.end_seconds),
            db(section.integrated_lufs, "LUFS"),
            db(section.true_peak_dbtp, "dBTP")
        );
    }
    for file in missing {
        let _ = write!(
            text,
            "\nwarning: audio \"{file}\" is not in the media provider"
        );
    }
    if let Some(path) = waveform {
        let _ = write!(text, "\nwaveform {}", path.display());
    }
    text
}

/// Waveform (min/max per column), momentary loudness, scene starts and cue starts.
fn waveform_image<'a, 'm: 'a, V: Video>(
    left: &[f32],
    right: &[f32],
    sample_rate: usize,
    offset_seconds: f64,
    scenes: &[(String, f64)],
    cues: &[(String, f64)],
    previewer: &Previewer<'a, 'm, V>,
) -> CliResult<RgbaFrame> {
    let (width, height) = (1600usize, 420usize);
    let (top, wave_h) = (28usize, 300usize);
    let duration = left.len().max(1) as f64 / sample_rate as f64;
    let x_of = |seconds: f64| seconds / duration * width as f64;
    let per_column = (left.len() / width).max(1);

    let mut wave = String::new();
    for x in 0..width {
        let from = (x * per_column).min(left.len());
        let to = ((x + 1) * per_column).min(left.len());
        let (mut lo, mut hi) = (0f32, 0f32);
        for i in from..to {
            let v = f32::midpoint(left[i], right[i]);
            lo = lo.min(v);
            hi = hi.max(v);
        }
        let mid = top as f32 + wave_h as f32 / 2.;
        let scale = wave_h as f32 / 2.;
        let _ = write!(
            wave,
            "M{x} {:.1}V{:.1}",
            mid - hi * scale,
            mid - lo * scale + 0.5
        );
    }

    let loudness = crate::LoudnessAnalysis::new(left, right, sample_rate).momentary();
    let loudness_y = |lufs: f64| top as f64 + wave_h as f64 * (lufs.clamp(-60., 0.) / -60.);
    let loudness_path = loudness
        .iter()
        .enumerate()
        .fold(String::new(), |mut path, (i, lufs)| {
            let x = x_of((i as f64 + 4.) * 0.1);
            let _ = write!(
                path,
                "{}{x:.1} {:.1}",
                if i == 0 { "M" } else { "L" },
                loudness_y(*lufs)
            );
            path
        });

    let mut markers = String::new();
    for (name, seconds) in scenes {
        let x = x_of(*seconds);
        let _ = write!(
            markers,
            r##"<path d="M{x:.1} {top}V{}" stroke="#a78bfa" stroke-width="1"/><text x="{:.1}" y="{}" font-size="13" fill="#c4b5fd">{}</text>"##,
            top + wave_h,
            x + 3.,
            top - 8,
            name.replace('&', "&amp;").replace('<', "&lt;")
        );
    }
    for (_, seconds) in cues.iter().filter(|(_, s)| *s >= 0. && *s <= duration) {
        let x = x_of(*seconds);
        let _ = write!(
            markers,
            r##"<path d="M{x:.1} {}V{}" stroke="#fbbf24" stroke-width="1.5"/>"##,
            top + wave_h + 4,
            top + wave_h + 16
        );
    }
    let tick = [1., 2., 5., 10., 15., 30., 60.]
        .into_iter()
        .find(|t| duration / t <= 16.)
        .unwrap_or(120.);
    let mut t = 0.;
    while t <= duration {
        let _ = write!(
            markers,
            r##"<text x="{:.1}" y="{}" font-size="12" fill="#a1a1aa">{:.0}s</text>"##,
            x_of(t) + 2.,
            height - 12,
            t + offset_seconds
        );
        t += tick;
    }

    let family = previewer.options().default_font;
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" font-family="'{family}', sans-serif">
<rect width="100%" height="100%" fill="#18181b"/>
<path d="M0 {mid}H{width}" stroke="#3f3f46"/>
<path d="{wave}" stroke="#38bdf8" stroke-width="1"/>
<path d="{loudness_path}" stroke="#f472b6" stroke-width="1.5" fill="none"/>
{markers}
<text x="8" y="{legend}" font-size="12" fill="#a1a1aa">waveform (blue), momentary loudness -60..0 LUFS (pink), scenes (purple), audio cues (yellow)</text>
</svg>"##,
        mid = top + wave_h / 2,
        legend = height - 30,
    );

    let tree =
        crate::usvgr::Tree::from_str(&svg, &crate::usvgr::Options::default(), previewer.font_db())
            .map_err(err)?;
    CpuFrameRenderer::new(0)
        .render_tree(&tree, crate::Color::BLACK, width as u32, height as u32)
        .map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spread_includes_both_ends() {
        assert_eq!(spread(0..101, 5), vec![0, 25, 50, 75, 100]);
        assert_eq!(spread(10..11, 5), vec![10]);
        assert_eq!(spread(0..3, 10), vec![0, 1, 2]);
    }

    #[test]
    fn parses_commands() {
        let cli =
            Cli::<NoArgs>::try_parse_from(["video", "frame", "1s,Intro@50%", "--json"]).unwrap();
        assert!(cli.json);
        assert!(
            matches!(cli.command, Some(Command::Frame(FrameArgs { ref at, .. })) if at.len() == 2)
        );

        let cli = Cli::<NoArgs>::try_parse_from(["video", "render", "Intro", "--draft"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Render(RenderArgs { draft: true, .. }))
        ));

        let cli = Cli::<NoArgs>::try_parse_from(["video"]).unwrap();
        assert!(cli.command.is_none());

        let cli = Cli::<NoArgs>::try_parse_from(["video", "preview", "Intro", "--paused"]).unwrap();
        assert!(
            matches!(cli.command, Some(Command::Preview(PreviewArgs { paused: true, ref at, .. })) if at == "Intro")
        );

        let cli =
            Cli::<NoArgs>::try_parse_from(["video", "audio", "analyze", "--waveform", "w.png"])
                .unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Audio(AudioCommand::Analyze { .. }))
        ));
    }
}
