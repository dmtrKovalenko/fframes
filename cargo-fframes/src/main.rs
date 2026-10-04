//! `cargo fframes new <name>`: creates a video project that renders and previews out of
//! the box.
//!
//! Interactive in a terminal (asks for everything that was not passed as a flag); with
//! `--yes` or without a terminal it never asks and uses defaults, so scripts and AI agents
//! can run it in one command.
use clap::{Parser, Subcommand, ValueEnum};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const FFRAMES_GIT: &str = "https://github.com/dmtrKovalenko/fframes";

const LIB_SINGLE_SCENE: &str = include_str!("../templates/lib_single_scene.rs.tmpl");
const LIB_MULTI_SCENE: &str = include_str!("../templates/lib_multi_scene.rs.tmpl");
const MAIN_CPU: &str = include_str!("../templates/main_cpu.rs.tmpl");
const MAIN_SKIA: &str = include_str!("../templates/main_skia.rs.tmpl");
const CARGO_TOML: &str = include_str!("../templates/Cargo.toml.tmpl");
const README: &str = include_str!("../templates/README.md.tmpl");
const GITIGNORE: &str = include_str!("../templates/gitignore.tmpl");
const FONT: &[u8] = include_bytes!("../templates/DMSans-Medium.ttf");

#[derive(Debug, Parser)]
#[command(name = "cargo-fframes", about = "fframes project tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a new video project.
    New(NewArgs),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Backend {
    /// Built-in tiny-skia renderer: no GPU and no Skia build, but ~10x slower and no preview window.
    Cpu,
    /// Skia on the GPU through Metal (macOS, default there). Skia is downloaded prebuilt.
    SkiaMetal,
    /// Skia on the GPU through Vulkan (default on Linux and Windows).
    SkiaVulkan,
}

impl Backend {
    /// Skia on the platform's native GPU API: fast renders and the real-time `preview` window.
    fn platform_default() -> Self {
        if cfg!(target_os = "macos") {
            Backend::SkiaMetal
        } else {
            Backend::SkiaVulkan
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Template {
    /// One scene that adapts to any format: a greeting that springs in over a moving glow.
    SingleScene,
    /// Two scenes on a 16:9 grid: a product announcement and an animated chart.
    MultiScene,
}

impl Template {
    fn lib(self) -> &'static str {
        match self {
            Template::SingleScene => LIB_SINGLE_SCENE,
            Template::MultiScene => LIB_MULTI_SCENE,
        }
    }

    /// A range worth a contact sheet or a draft render in the README.
    fn strip_range(self) -> &'static str {
        match self {
            Template::SingleScene => "0..2s",
            Template::MultiScene => "DataScene",
        }
    }

    fn entrance_range(self) -> &'static str {
        match self {
            Template::SingleScene => "0..1.5s",
            Template::MultiScene => "ProductScene@0..ProductScene@1.5s",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    /// 1920x1080
    Landscape,
    /// 1080x1920 (reels, shorts, tiktok)
    Portrait,
    /// 1080x1080
    Square,
    /// 3840x2160
    Uhd,
}

impl Format {
    fn size(self) -> (usize, usize) {
        match self {
            Format::Landscape => (1920, 1080),
            Format::Portrait => (1080, 1920),
            Format::Square => (1080, 1080),
            Format::Uhd => (3840, 2160),
        }
    }
}

#[derive(Debug, clap::Args)]
struct NewArgs {
    /// Crate name, e.g. `launch-video`. Also the directory name.
    name: Option<String>,
    /// Title shown in the video.
    #[arg(long)]
    title: Option<String>,
    /// `single-scene` (any format) or `multi-scene` (two scenes, 16:9).
    #[arg(long, value_enum)]
    template: Option<Template>,
    #[arg(long, value_enum)]
    backend: Option<Backend>,
    #[arg(long, value_enum)]
    format: Option<Format>,
    #[arg(long)]
    fps: Option<usize>,
    /// Where to create the project (default: ./<name>).
    #[arg(long)]
    dir: Option<PathBuf>,
    /// Use a local fframes checkout instead of crates.io.
    #[arg(long, conflicts_with = "git")]
    fframes_path: Option<PathBuf>,
    /// Depend on the `main` branch of the fframes repository instead of the crates.io release
    /// matching this version of cargo-fframes.
    #[arg(long)]
    git: bool,
    /// Never ask, use defaults for everything not passed.
    #[arg(short, long)]
    yes: bool,
}

struct Project {
    name: String,
    template: Template,
    title: String,
    backend: Backend,
    width: usize,
    height: usize,
    fps: usize,
    dir: PathBuf,
}

fn main() -> ExitCode {
    // `cargo fframes new` runs `cargo-fframes fframes new`.
    let args = std::env::args()
        .enumerate()
        .filter_map(|(i, arg)| (i != 1 || arg != "fframes").then_some(arg));
    let cli = Cli::parse_from(args);

    let result = match cli.command {
        Command::New(args) => new(args),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn valid_name(name: &str) -> Result<(), String> {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_lowercase() => {}
        _ => return Err(format!("\"{name}\" must start with a lowercase letter")),
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(format!("\"{name}\" may only contain a-z, 0-9, - and _"));
    }
    Ok(())
}

fn pascal_case(name: &str) -> String {
    name.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect()
}

fn title_case(name: &str) -> String {
    name.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn ask(args: NewArgs) -> Result<Project, String> {
    let interactive =
        !args.yes && std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    let theme = dialoguer::theme::SimpleTheme;
    let prompt_err = |e: dialoguer::Error| e.to_string();

    let name = match args.name {
        Some(name) => name,
        None if interactive => dialoguer::Input::<String>::with_theme(&theme)
            .with_prompt("Project name")
            .default("my-video".into())
            .validate_with(|name: &String| valid_name(name))
            .interact_text()
            .map_err(prompt_err)?,
        None => return Err("pass a project name: cargo fframes new <name>".into()),
    };
    valid_name(&name)?;

    let title = match args.title {
        Some(title) => title,
        None if interactive => dialoguer::Input::<String>::with_theme(&theme)
            .with_prompt("Title shown in the video")
            .default(title_case(&name))
            .interact_text()
            .map_err(prompt_err)?,
        None => title_case(&name),
    };

    let template = match args.template {
        Some(template) => template,
        None if interactive => {
            let items = [
                "single-scene  - one scene that adapts to any format",
                "multi-scene   - two scenes: product announcement and animated chart (16:9)",
            ];
            let choice = dialoguer::Select::with_theme(&theme)
                .with_prompt("Template")
                .items(&items)
                .default(0)
                .interact()
                .map_err(prompt_err)?;
            [Template::SingleScene, Template::MultiScene][choice]
        }
        None => Template::SingleScene,
    };

    let backend = match args.backend {
        Some(backend) => backend,
        None if interactive => {
            let items = [
                "skia-metal   - GPU on macOS: fast renders + preview window",
                "skia-vulkan  - GPU on Linux/Windows (macOS via MoltenVK)",
                "cpu          - no Skia build, slower renders, no preview window",
            ];
            let default = match Backend::platform_default() {
                Backend::SkiaMetal => 0,
                _ => 1,
            };
            let choice = dialoguer::Select::with_theme(&theme)
                .with_prompt("Rendering backend")
                .items(&items)
                .default(default)
                .interact()
                .map_err(prompt_err)?;
            [Backend::SkiaMetal, Backend::SkiaVulkan, Backend::Cpu][choice]
        }
        None => Backend::platform_default(),
    };

    let format = match args.format {
        Some(format) => format,
        None if interactive => {
            let items = [
                "landscape 1920x1080",
                "portrait  1080x1920 (reels, shorts)",
                "square    1080x1080",
                "uhd       3840x2160",
            ];
            let choice = dialoguer::Select::with_theme(&theme)
                .with_prompt("Format")
                .items(&items)
                .default(0)
                .interact()
                .map_err(prompt_err)?;
            [
                Format::Landscape,
                Format::Portrait,
                Format::Square,
                Format::Uhd,
            ][choice]
        }
        None => Format::Landscape,
    };

    let fps = match args.fps {
        Some(fps) => fps,
        None if interactive => {
            let items = ["30", "60", "24", "25"];
            let choice = dialoguer::Select::with_theme(&theme)
                .with_prompt("Frames per second")
                .items(&items)
                .default(0)
                .interact()
                .map_err(prompt_err)?;
            items[choice].parse().expect("fps choices are numbers")
        }
        None => 30,
    };

    let (width, height) = format.size();
    if template == Template::MultiScene && width * 9 != height * 16 {
        return Err(format!(
            "the multi-scene template is laid out for 16:9 (landscape or uhd), not {width}x{height}; \
             use --template single-scene for other formats"
        ));
    }
    let dir = args.dir.unwrap_or_else(|| PathBuf::from(&name));
    Ok(Project {
        name,
        template,
        title,
        backend,
        width,
        height,
        fps,
        dir,
    })
}

/// The fframes repository containing `dir`, if any (its root `Cargo.toml` is a workspace
/// and it has the `fframes` crate).
fn find_fframes_workspace(dir: &Path) -> Option<PathBuf> {
    let absolute = std::path::absolute(dir).ok()?;
    absolute.ancestors().skip(1).find_map(|candidate| {
        let manifest = std::fs::read_to_string(candidate.join("Cargo.toml")).ok()?;
        (manifest.contains("[workspace]") && candidate.join("fframes/Cargo.toml").exists())
            .then(|| candidate.to_path_buf())
    })
}

fn render(template: &str, vars: &[(&str, String)]) -> String {
    vars.iter().fold(template.to_owned(), |text, (key, value)| {
        text.replace(&format!("{{{{{key}}}}}"), value)
    })
}

fn register_workspace_member(root: &Path, member: &str) -> Result<(), String> {
    let manifest_path = root.join("Cargo.toml");
    let manifest = std::fs::read_to_string(&manifest_path).map_err(|e| e.to_string())?;
    if manifest.contains(&format!("\"{member}\"")) {
        return Ok(());
    }
    let members = manifest
        .find("members = [")
        .ok_or("the workspace Cargo.toml has no `members = [` list")?;
    let close = members
        + manifest[members..]
            .find("\n]")
            .ok_or("can not find the end of the workspace members list")?;
    let updated = format!(
        "{}\n    \"{member}\",{}",
        &manifest[..close],
        &manifest[close..]
    );
    std::fs::write(&manifest_path, updated).map_err(|e| e.to_string())
}

fn new(args: NewArgs) -> Result<(), String> {
    let fframes_path = args.fframes_path.clone();
    let from_git = args.git;
    let project = ask(args)?;
    let dir = &project.dir;

    if dir.exists()
        && std::fs::read_dir(dir)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err(format!("{} already exists and is not empty", dir.display()));
    }

    let workspace = find_fframes_workspace(dir);
    let fframes_features = r#"features = ["compile-time-svgtree", "cli"]"#;
    let skia_feature = match project.backend {
        Backend::Cpu => None,
        Backend::SkiaMetal => Some("metal"),
        Backend::SkiaVulkan => Some("vulkan"),
    };

    // Skia renderer and the real-time player (the `preview` command) for GPU backends.
    let skia_deps = |renderer_source: &str, player_source: &str| {
        skia_feature
            .map(|f| {
                // Workspace dependencies can not turn default features off.
                let player_features = if player_source == "workspace = true" {
                    format!("features = [\"{f}\"]")
                } else {
                    format!("default-features = false, features = [\"{f}\", \"audio\"]")
                };
                format!(
                    "fframes_skia_renderer = {{ {renderer_source}, features = [\"{f}\"] }}\n\
                     fframes_native_player = {{ {player_source}, {player_features} }}\n"
                )
            })
            .unwrap_or_default()
    };

    let (fframes_dep, skia_dep, media_path, standalone_tables, run) = match (
        &workspace,
        &fframes_path,
    ) {
        (Some(root), _) => {
            let relative = std::path::absolute(dir)
                .map_err(|e| e.to_string())?
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            (
                format!("fframes = {{ workspace = true, {fframes_features} }}"),
                skia_deps("workspace = true", "workspace = true"),
                format!("{relative}/media"),
                String::new(),
                format!("cargo run --release -p {} --", project.name),
            )
        }
        (None, source) => {
            let [fframes_source, skia_source, player_source] = match source {
                Some(path) => {
                    let path = std::path::absolute(path).map_err(|e| e.to_string())?;
                    ["fframes", "fframes-skia-renderer", "fframes-native-player"]
                        .map(|crate_dir| format!("path = \"{}\"", path.join(crate_dir).display()))
                }
                None if from_git => {
                    std::array::from_fn(|_| format!("git = \"{FFRAMES_GIT}\", branch = \"main\""))
                }
                // fframes and cargo-fframes are released together with the same version, so
                // the generated project uses exactly the fframes this template was written for.
                None => {
                    std::array::from_fn(|_| format!("version = \"={}\"", env!("CARGO_PKG_VERSION")))
                }
            };
            (
                    format!("fframes = {{ {fframes_source}, {fframes_features} }}"),
                    skia_deps(&skia_source, &player_source),
                    "media".to_owned(),
                    // Its own workspace, and dependencies optimized in dev builds so that
                    // `cargo run` without --release still renders quickly.
                    "\n[workspace]\n\n[profile.dev]\nopt-level = 1\n\n[profile.dev.package.\"*\"]\nopt-level = 3\n"
                        .to_owned(),
                    "cargo run --release --".to_owned(),
                )
        }
    };

    let (skia_module, skia_ctx, skia_constructor, skia_hint, backend_label) = match project.backend
    {
        Backend::Cpu => ("", "", "", "", "built-in CPU"),
        Backend::SkiaMetal => ("metal", "SkiaMetalCtx", "new_metal", "", "Skia (Metal)"),
        Backend::SkiaVulkan => ("vulkan", "SkiaVulkanCtx", "new_vulkan", "", "Skia (Vulkan)"),
    };

    let lib_name = project.name.replace('-', "_");
    let vars: Vec<(&str, String)> = vec![
        ("crate_name", project.name.clone()),
        ("lib_name", lib_name.clone()),
        ("Struct", pascal_case(&project.name)),
        ("title", project.title.replace('"', "\\\"")),
        ("width", project.width.to_string()),
        ("height", project.height.to_string()),
        ("fps", project.fps.to_string()),
        ("media_path", media_path),
        // Codec features link a static ffmpeg (prebuilt or compiled); Windows links a prebuilt ffmpeg DLL.
        (
            "codecs",
            format!(
                "\n[target.'cfg(not(windows))'.dependencies]\n{}\n",
                fframes_dep.replace(
                    fframes_features,
                    r#"features = ["h264", "libav-agree-gpl"]"#
                )
            ),
        ),
        ("fframes_dep", fframes_dep),
        ("skia_dep", skia_dep),
        ("standalone_tables", standalone_tables),
        ("skia_module", skia_module.to_owned()),
        ("skia_ctx", skia_ctx.to_owned()),
        ("skia_constructor", skia_constructor.to_owned()),
        ("skia_hint", skia_hint.to_owned()),
        ("backend_label", backend_label.to_owned()),
        ("run", run.clone()),
        ("strip_range", project.template.strip_range().to_owned()),
        (
            "entrance_range",
            project.template.entrance_range().to_owned(),
        ),
        // The real-time player only exists in Skia projects.
        (
            "preview_line",
            if project.backend == Backend::Cpu {
                String::new()
            } else {
                format!(
                    "{run} preview                       # real-time GPU window with sound (space, h/l, j/k, q)\n"
                )
            },
        ),
    ];

    let main = if project.backend == Backend::Cpu {
        MAIN_CPU
    } else {
        MAIN_SKIA
    };
    let files: [(&str, String); 5] = [
        ("Cargo.toml", render(CARGO_TOML, &vars)),
        ("src/lib.rs", render(project.template.lib(), &vars)),
        ("src/main.rs", render(main, &vars)),
        ("README.md", render(README, &vars)),
        (".gitignore", render(GITIGNORE, &vars)),
    ];

    for (path, content) in &files {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().expect("files are in the project"))
            .map_err(|e| e.to_string())?;
        std::fs::write(&path, content).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(dir.join("media")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("media/DMSans-Medium.ttf"), FONT).map_err(|e| e.to_string())?;

    if let Some(root) = &workspace {
        let member = std::path::absolute(dir)
            .map_err(|e| e.to_string())?
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        register_workspace_member(root, &member)?;
    }

    let cd = format!("cd {}", dir.display());
    println!(
        "Created {} ({}x{} @ {} fps, {backend_label})",
        dir.display(),
        project.width,
        project.height,
        project.fps
    );
    if workspace.is_some() {
        println!("Added it to the fframes workspace members.");
    }
    println!(
        "\nNext:\n  {cd}\n  {run} timeline\n  {run} frame 1s,50%,end   # writes frames/*.png\n  {run} strip -n 12        # writes strip.png{preview}\n  {run} render             # writes out.mp4",
        preview = if project.backend == Backend::Cpu {
            String::new()
        } else {
            format!("\n  {run} preview            # real-time GPU window with sound")
        }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(pascal_case("launch-video_2"), "LaunchVideo2");
        assert_eq!(title_case("launch-video"), "Launch Video");
        assert!(valid_name("launch-video").is_ok());
        assert!(valid_name("Launch").is_err());
        assert!(valid_name("1video").is_err());
        assert!(valid_name("my video").is_err());
    }

    #[test]
    fn templates_have_no_unknown_placeholders() {
        let vars: Vec<(&str, String)> = [
            "crate_name",
            "lib_name",
            "Struct",
            "title",
            "width",
            "height",
            "fps",
            "media_path",
            "fframes_dep",
            "codecs",
            "skia_dep",
            "standalone_tables",
            "skia_module",
            "skia_ctx",
            "skia_constructor",
            "skia_hint",
            "backend_label",
            "run",
            "preview_line",
            "strip_range",
            "entrance_range",
        ]
        .into_iter()
        .map(|k| (k, "x".to_owned()))
        .collect();
        for template in [
            LIB_SINGLE_SCENE,
            LIB_MULTI_SCENE,
            MAIN_CPU,
            MAIN_SKIA,
            CARGO_TOML,
            README,
            GITIGNORE,
        ] {
            let rendered = render(template, &vars);
            assert!(
                !rendered.contains("{{"),
                "unreplaced placeholder in:\n{rendered}"
            );
        }
    }
}
