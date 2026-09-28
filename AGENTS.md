# AGENTS.md - AI Coding Agent Guidelines for fframes

This file provides guidelines for AI coding agents working on the fframes codebase.

## Project Overview

fframes is a Rust-based video generation framework that renders videos from SVG-based scene descriptions.

**Tech Stack:**

- **Core:** Rust (Edition 2024), Cargo workspace
- **Editor:** ReScript (compiles to JS), React 19, Tailwind CSS v4
- **WASM:** wasm-bindgen, wasm-pack for browser preview
- **Graphics:** `svgr!` SVG trees rendered by the built-in CPU backend (tiny-skia) or the optional Skia GPU backend; FFmpeg for encoding/decoding

## Build & Development Commands

### Task Runner: Just (justfile)

```bash
# Full repository setup (run first after clone)
just init-repo

# Build everything (cargo + editor)
just build

# Watch editor during development
just watch-editor

# Run example with live reload (editor + WASM hot reload)
just run <example-name>

# Render example to video file
just render <example-name>
```

### Rust Commands

```bash
# Build
cargo build
cargo build --release

# Lint (warnings treated as errors)
just clippy
# Or directly:
cargo clippy -- -D warnings -A clippy::option-map-unit-fn -A clippy::module_inception -A clippy::single-match -A clippy::single-range-in-vec-init

# Run all tests
just test
# Or:
cargo test
cargo test -p fframes_test_utils --no-default-features

# Run a single test
cargo test <test_name>
cargo test -p <package> <test_name>

# Run tests in specific package
cargo test -p fframes
cargo test -p fframes-media

# Check WASM compilation for examples
just check-wasm <example-name>
just check-examples  # checks all examples
```

### Editor (ReScript/TypeScript) Commands

```bash
cd fframes-editor

# Build ReScript and bundle
yarn build

# Development mode with watch
yarn dev

# Clean ReScript build
yarn rescript:clean
```

### Package Management

```bash
# Lint package.json consistency
yarn syncpack lint

# Format JS/TS files
yarn prettier --write .
```

## Code Style Guidelines

### Rust Conventions

**Imports:**

- Group imports: std lib first, then external crates, then local modules
- Use `use crate::` for internal module imports
- Re-export public API through `lib.rs` using `pub use module::*;`

```rust
use std::{error::Error, fmt};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::{Color, Duration, Frame, Svgr};
```

**Error Handling:**

- Define custom errors as enums in `error.rs` modules
- Implement `std::error::Error` and `fmt::Display` for error types
- Use `type Result<T> = std::result::Result<T, CustomError>;` pattern
- Implement `From` traits for error conversion

```rust
#[derive(Debug)]
pub enum FFramesError {
    UserError(String),
    MediaError(crate::media::FFramesMediaError),
}

impl From<SomeError> for FFramesError {
    fn from(err: SomeError) -> Self { ... }
}
```

**Naming:**

- Types: `PascalCase` (e.g., `AnimationRuntime`, `KeyFrame`)
- Functions/methods: `snake_case` (e.g., `render_frame`, `get_duration`)
- Constants: `SCREAMING_SNAKE_CASE` (e.g., `BACKGROUND_COLOR`)
- Traits: `PascalCase`, often adjectives (e.g., `Animatable`, `Sync`)

**Traits:**

- Use associated constants for video metadata: `const FPS`, `const WIDTH`, `const HEIGHT`
- Prefer `&self` methods over consuming self when possible
- Use lifetimes explicitly when returning references tied to self

```rust
pub trait Video: Sync + Sized {
    const FPS: usize;
    const WIDTH: usize;
    const HEIGHT: usize;

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a>;
}
```

**Documentation:**

- Use `///` doc comments for public APIs
- Include examples in doc comments with ```rust blocks
- Document panics, safety concerns, and performance considerations

**WASM Compatibility:**

- Use `#[cfg(not(target_arch = "wasm32"))]` for native-only code
- Use `#[cfg(target_arch = "wasm32")]` for WASM-specific code

### ReScript Conventions

**File Structure:**

- One component per file, named after component (e.g., `Editor.res`)
- Use `open` sparingly, prefer qualified access
- Place bindings in `src/bindings/` directory

**Components:**

```rescript
@genType.as("ComponentName") @react.component
let make = (~prop1: type1, ~prop2: type2) => {
  // hooks first
  let (state, setState) = React.useState(_ => initialValue)

  // effects
  React.useEffect0(() => { ... None })

  // render JSX
  <div className="tailwind-classes">
    {React.string("text")}
  </div>
}
```

**Naming:**

- Components: `PascalCase` files, lowercase `make` function
- Types: `lowerCamelCase` (ReScript convention)
- Variants: `PascalCase` (e.g., `MediaList.Grid`, `MediaList.List`)

**Styling:**

- Use Tailwind CSS classes via `className`
- Use `Cx.cx([...])` for conditional class concatenation

## Testing

**Rust Tests:**

- Unit tests in `#[cfg(test)] mod tests` blocks
- Integration tests in `/e2e/tests/`
- Visual regression tests compare rendered frames using `odiff`

**Running specific tests:**

```bash
# Run test by name
cargo test test_name

# Run tests matching pattern
cargo test animation

# Run with output
cargo test -- --nocapture
```

## Project Structure

```
/fframes                   # Core: Video/Scene traits, svgr! trees, animation, text, CPU renderer, encoding
/fframes-media             # Audio/video decoding (ffmpeg), fonts, images, subtitles
/fframes-skia-renderer     # Optional Skia backend (GPU via vulkan/metal, or Skia CPU)
/fframes-native-player     # Real-time preview window: fframes_native_player::play(&video, &PlayerOptions)
/fframes-editor            # Web editor UI (ReScript + React), published as @fframes/editor
/fframes-editor-controller # WASM bridge between a Video impl and the editor
/fframes-test-utils        # Snapshot helpers for svgr! trees
/svgr-macro                # The svgr! procedural macro (SVG DSL + static-subtree hashing)
/media-dir-macro           # include_media_dir! (embeds a media folder into the binary)
/webvtt-parser             # Subtitle format parser
/cargo-fframes             # `cargo fframes new`: project scaffolding (templates/)
/scripts/new-video.sh      # curl | bash bootstrap for cargo-fframes
/e2e                       # End-to-end visual regression test
/examples                  # Example video projects (each is a lib + bin + editor bridge)
```

## Creating a New Video

Every video is a Rust crate under `examples/` (or your own crate) with three parts: a library
crate implementing `fframes::Video`, a binary that renders it, and a small WASM crate that
plugs it into the web editor for live preview. `examples/hello-world` is the minimal template;
`examples/teej-podcast` shows video sources, dynamic media and the Skia GPU backend;
`examples/beta` shows scenes.

### 1. Scaffold the crate

```bash
just new my-video                      # = cargo run -p cargo-fframes -- fframes new my-video --dir examples/my-video --yes
cargo fframes new my-video --template multi-scene --fps 60 --backend skia-metal --yes   # anywhere
curl -fsSL https://raw.githubusercontent.com/dmtrKovalenko/fframes/main/scripts/new-video.sh | bash -s -- my-video
```

`cargo-fframes new` asks for anything not passed when run in a terminal; `--yes` (or no
terminal) never asks. Flags: `--template single-scene|multi-scene` (one scene in any format, or
two scenes on a 16:9 grid), `--title`, `--format landscape|portrait|square|uhd`, `--fps`,
`--backend cpu|skia-metal|skia-vulkan`, `--dir`, `--fframes-path <checkout>`. Inside this
repository it uses workspace dependencies and registers the crate in the workspace members;
elsewhere it pins the crates.io release matching its own version (`--git` for `main`,
`--fframes-path` for a checkout) and makes the crate its own workspace. `release.sh` publishes
`cargo-fframes` and `fframes_native_player` with the other crates on every push to main.

The generated crate compiles and renders as is:

```
my-video/
  Cargo.toml          # fframes with the `cli` and `compile-time-svgtree` features
  media/              # embedded by include_media_dir! (a DM Sans font to start with)
  src/lib.rs          # the Video from the template
  src/main.rs         # fframes::cli (render, frame, strip, onion, svg, timeline, inspect, snapshot, audio)
  tests/frames.rs     # frame snapshots + "no warnings in any frame"
```

For the web editor copy `examples/hello-world/editor` next to it and adapt step 9; gate the
`cli` and `compile-time-svgtree` features behind a `renderer` feature like hello-world does,
the editor bridge builds the library with `default-features = false`.

### 2. Media

Embed static assets with `include_media_dir!`. The path is relative to the **workspace root**
and every file in the folder becomes a field of the generated struct; fonts are registered by
their family name. Audio (`mp3`, `wav`, `flac`, `aac`, `ogg`, `m4a`) is decoded to mono at
compile time; files loaded with `MediaDirectory` keep stereo.

```rust
use fframes::include_media_dir;

include_media_dir!(pub struct MyVideoMedia, "examples/my-video/media");
```

- In `main.rs`: `let media = MyVideoMedia::prepare()?;` and pass `media: Some(&media)` in
  `RenderOptions`.
- Large or changing files (video sources) go into a runtime folder instead (renderer only;
  the editor loads the same files over HTTP). The provider borrows the directory, so keep
  both bindings alive:
  `let folder = MediaDirectory::read_folder("./dynamic_media")?;`
  `let media = folder.process_media_source()?;`
- Inside `render_frame` resolve media through the context: `ctx.get_image("bg.png")`,
  `ctx.get_audio("track.mp3")`, `ctx.get_subtitles("subs.vtt")`, `ctx.get_video("clip.mp4")`.
  Never `.expect()` on these in real code; return a fallback element instead.
- Fonts are referenced by family name in `font-family=...`. `load_system_fonts: true` is a
  development convenience only; ship every font in `media/` so renders are reproducible.

### 3. Implement `Video`

```rust
use fframes::{AudioMap, AudioTimestamp, Color, Duration, FFramesContext, Frame, Svgr, Video};

#[derive(Debug)]
pub struct MyVideo<'a> {
    pub media: &'a MyVideoMedia,
    pub title: &'a str,
}

impl Video for MyVideo<'_> {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;
    const BACKGROUND_COLOR: Color = Color::BLACK; // Color::TRANSPARENT needs an alpha-capable encoder

    fn duration(&self) -> Duration<'_> {
        Duration::Seconds(10.0)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::from([("track.mp3", AudioTimestamp::Second(0.)..AudioTimestamp::Eof)])
    }

    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1920 1080"
                 width={Self::WIDTH} height={Self::HEIGHT}>
                <text x="100" y="300" font-family="DM Sans" font-size="150" fill="#fff">
                    "Hello " {self.title}
                </text>
            </svg>
        )
    }
}
```

`Duration` variants: `Seconds(f32)`, `Frames(usize)`, `FromAudio("file")`,
`FromVideo("file")` (uses the video's metadata), `Auto` (inferred from scenes or the audio map).
Durations support `+` and `-`. `AudioTimestamp` supports `Second`, `Frame`, `Time { minutes,
seconds }`, `Eof`, `DurationOfAudio("file")` and arithmetic.

Rules for `render_frame`:

- It runs once per frame on several threads. No panics, no I/O, no heavy allocation. Precompute
  in the constructor and read from `self`; memoise per-video data in a `std::sync::OnceLock`
  field (only store a measurement once it succeeded, see `examples/motion-graphics`).
- `svgr!` is SVG with Rust interpolation: attributes and text children accept `{expr}` where
  `expr` is a string, number, `Color`, `Transform`, another `Svgr`, or an iterator of `Svgr`
  (`.collect::<Vec<_>>()`). Comments are `// ...` lines inside the markup.
- Subtrees whose markup contains no `{}` get a compile-time `static_hash` and are cached by
  the renderers across frames (paths, paints, whole groups). Keep decorative geometry lexically
  static and put the animated values on a wrapping `<g transform={...} opacity={...}>`.
- Return `Svgr::empty()` for "nothing this frame".

### 4. Animate

`timeline!` builds a keyframe animation once; `frame.animate(&anim)` samples it at the frame's
time. Keyframes are `at <start> [=> <end> | , duration <d>], animate <from> => <to>, <easing>`.
Values can be `f32`/`f64`, `Color`, or `Transform`.

```rust
use fframes::{Transform, animation::Easing};

let slide = frame.animate(&fframes::timeline!(
    at 0.0, animate Transform::translate(0, 80) => Transform::translate(0, 0),
        Easing::Spring { mass: 1.0, stiffness: 300.0, damping: 26.0 },
    at 2.2 => 2.8, animate Transform::translate(0, 0) => Transform::translate(0, -80), Easing::EaseIn,
));
let opacity = frame.animate(&fframes::timeline!(at 0.0 => 0.3, animate 0.0_f32 => 1.0, Easing::EaseOut));
```

- Before the first keyframe the value is `from`; after the last it stays at `to`.
- `frame.animate_loop(&anim)` wraps the time around the animation's total duration.
- Easings: `Linear`, `EaseIn`, `EaseOut`, `EaseInOut`, `CubicBezier(x1, y1, x2, y2)`,
  `Spring { mass, stiffness, damping }`. A spring computes its own settle time; an explicit
  `=> end` shorter than that cuts it off, so leave the end out unless you want the cut.
- Store `timeline!` results that never change in `self` (see `KeyFramesAnimation<f32>` in
  `examples/teej-podcast`) instead of rebuilding them every frame.

### 5. Text

Fonts are measured from the font files, so layout is deterministic and identical in the
renderer. All three helpers return `None` when the font cannot be resolved (system fonts in the
editor); fall back to the raw text in that case. They take `&mut frame`.

```rust
use fframes::{BreakLinesOpts, FontQuery, TextAlign, TextOverflow};

let font = FontQuery { family: "DM Sans", size: 26, weight: 400, ..Default::default() };

// Width in pixels.
let width = frame.text_width(ctx, font, "Chapter title");

// Single line that must fit into a box: drops what does not fit and ends with "…"
// (or TextOverflow::Clip / TextOverflow::Marker("...")).
let title = frame
    .text_fit(ctx, font, chapter.title, 420, TextOverflow::Ellipsis)
    .map(std::borrow::Cow::into_owned)
    .unwrap_or_else(|| chapter.title.to_owned());

// Multi-line paragraph, returns a ready <text> element (or use
// text_break_lines_structure for the line data).
let paragraph = frame.text_break_lines(ctx, self.description, BreakLinesOpts {
    font,
    width: 800,
    x: 100,
    y: 200,
    align: TextAlign::Left,
    fill: "#fff",
    ..Default::default()
});
```

Use the same `FontQuery` values in the `<text>` attributes (`font-family`, `font-size`,
`font-weight`) so what is measured is what is drawn. `font-weight` must be numeric or
`normal`/`bold`.

### 6. Scenes (optional)

Split a long video into `Scene` implementations and let the framework place them on the timeline.

```rust
use fframes::{Scene, Scenes};

#[derive(Debug)]
struct Intro;

impl Scene for Intro {
    fn duration(&self) -> Duration<'_> { Duration::Seconds(3.0) }
    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        // `frame.index` / `frame.seconds()` are relative to the scene start
        fframes::svgr!(<g>...</g>)
    }
    // optional: fn audio(&self), fn overlap(&self) -> Overlap
}

impl Video for MyVideo<'_> {
    fn duration(&self) -> Duration<'_> { Duration::Auto } // sum of the scenes
    fn define_scenes(&self) -> Scenes<'_> {
        let scenes: Vec<&dyn Scene> = vec![&Intro, &self.main_scene];
        Scenes::from(scenes)
    }
    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        fframes::svgr!(<svg ...>{ctx.render_scenes(&frame)}</svg>)
    }
}
```

Scenes must be zero-sized or borrowed from `&self`. `Overlap::{Previous, Next, PreviousAndNext}`
lets scenes cross-fade; `ctx.get_scene_info(&scene)` gives the resolved frame range.

### 7. Video sources, audio visualisation, subtitles

```rust
// A frame of a video file synced to the timeline. Always give the editor a fallback image.
let Some(video_frame) = frame.get_synced_video_frame(ctx, "clip.mp4", &fframes::SyncVideoFrameInput {
    start_from: 0.,
    looping: false,
    editor_fallback_image: ctx.get_image("clip_poster.jpg"),
}) else { return Svgr::empty() };
let image = video_frame.into_image(); // <image href={image.href()} .../>

// Audio spectrum for the current frame (see examples/audio-announce).
let bars = frame.visualize_audio_frame(fframes::VisualizeFrameInput {
    audio: ctx.get_audio("track.mp3")?, // listed in `fn audio`, so it is loaded
    sample_size: fframes::SampleSize::S256,
    smooth_level: 4,
    window: Some(fframes::WindowFunction::Hann),
});

// Current subtitle cue.
let phrase = ctx.get_subtitles("subs.vtt").and_then(|s| frame.get_subtitle_phrase(s));
```

### 7b. GPU shaders (Skia backend)

`fframes::Shader` runs SkSL (or pasted Shadertoy GLSL) on the Skia backend's GPU surface
while it draws the frame. The pixels never leave the GPU, and SVG transforms, clips, masks
and opacity apply to the layer like to any element. See `examples/shaders` and
`examples/neon-triangle`.

```rust
use fframes::{Shader, ShaderUniforms};

// once, in the constructor (the renderer compiles and caches it by id)
let aurora = Shader::sksl(include_str!("shaders/aurora.sksl")); // half4 main(float2 coord)
let torus = Shader::shadertoy(include_str!("shaders/torus.glsl")); // void mainImage(out vec4, in vec2)

// in render_frame
let Some(photo) = ctx.get_image("photo.jpg") else { return Svgr::empty() };
let layer = self.aurora.draw(&frame, ShaderUniforms::new()
    .float("uSpeed", 0.6)
    .color("uTint", frame.animate(&tint)) // uniform float4 uTint;
    .image("iChannel0", photo));          // uniform shader iChannel0; sample with iChannel0.eval(px)
fframes::svgr!(<image href={layer.href()} x="0" y="0" width="1920" height="1080" />)
```

- Built-ins filled in when declared: `iResolution` (float3, the `<image>` width/height),
  `iTime`, `iTimeDelta`, `iFrame` (int). `coord` is in the element's units, origin top-left
  (Shadertoy's `fragCoord` is flipped to bottom-left for you).
- SkSL follows GLSL ES 2: constant loop bounds, no `while`, no dynamic array indexing, no
  preprocessor. `Shader::shadertoy` expands object-like `#define`s and drops `precision`;
  function-like macros and `texture()` must be rewritten.
- Compile errors are logged once and the layer is skipped. Catch them in a test with
  `fframes_skia_renderer::render::compile_shader(&shader)`.
- Only the Skia backend (GPU, or `SkiaCpuCtx`) executes shaders. The tiny-skia CPU backend
  draws nothing in their place, and the editor shows a placeholder.
- Requires the `compile-time-svgtree` feature (the default `renderer` feature of the examples).

### 7c. Audio mix

`fn audio` places files on the timeline. Tuples still work; `AudioTrack` adds mix settings:

```rust
use fframes::{AudioMap, AudioTrack, AudioTimestamp::*, FadeCurve};

AudioMap::from([
    AudioTrack::new("music.mp3", Second(0.)..Eof).gain_db(-16.).fade_in(1.).fade_out(2.).duck_under_voice(),
    AudioTrack::new("voice.wav", Second(0.5)..Eof).voice(),
    AudioTrack::new("whoosh.wav", Second(4.25)..Eof).gain_db(-6.).pan(-0.4),
    AudioTrack::new("take.wav", Second(10.)..Second(14.)).offset(3.2),  // plays 3.2s..7.2s of the file
])
```

- Tracks start at their exact sample (`Second(4.25)` is not rounded to a frame), overlapping
  tracks are summed linearly and the master bus has a -1 dBFS lookahead limiter
  (`RenderOptions::audio_mix`), output is stereo. Other sample rates are resampled with a
  windowed sinc.
- `duck_under_voice()` lowers a track by 12 dB while any `.voice()` track plays, ramping down
  before the voice starts (`Ducking { depth_db, attack, hold, release, merge_gap }` to tune).
- Fade curves: `EqualPower` (default), `Linear`, `SCurve`, `Exponential`. Cuts (offsets,
  range renders) get 5 ms de-click fades.
- Verify without listening: `audio analyze` (integrated/short-term loudness in LUFS, true
  peak, clipping, silent ranges, loudness per scene, `--waveform` PNG with scenes and cues)
  and `audio at 4.2s` (which files play, where in the file, at what level, ducked or not).
  Aim for about -14 LUFS integrated for web video and a true peak below -1 dBTP.

### 8. Render it: `fframes::cli`

`main.rs` hands the video to `fframes::cli` (feature `cli`), which gives every video the same
command line:

```rust
use fframes::cli;

fn main() -> std::process::ExitCode {
    let media = MyVideoMedia::prepare().expect("media");
    let video = MyVideo { media: &media, title: "World" };
    cli::new(&video, RenderOptions { media: Some(&media), ..Default::default() }).run()
}
```

The rest is optional and chains in any order:

```rust
cli::new(&video, options)
    // Render with Skia. Frame previews (frame, strip, onion, snapshot) use the backend's own
    // renderer, so they match the video; the CPU backend is the default.
    .backend(SkiaFFramesRenderer::new_metal(&gpu, SkiaPipelineConfig::default())?)
    // The `preview` command: a real-time window with audio (fframes_native_player).
    .preview(fframes_native_player::cli_preview)
    // What `render` writes without `-o`.
    .default_output("out.webm")
    .run()
```

Flags of your own go into a `#[derive(clap::Args)]` struct. Parse first when the video is built
from them:

```rust
use fframes::cli::{self, clap};

#[derive(Debug, clap::Args)]
struct Args {
    #[arg(long, default_value = "World", global = true)]
    title: String,
}

let args = cli::parse::<Args>();
let video = MyVideo { media: &media, title: &args.app.title.clone() };
cli::new(&video, options).args(args).run()
```

See `examples/teej-podcast/src/main.rs`; `cargo fframes new` generates the same setup.

| command | what |
| --- | --- |
| `render [RANGE] [-o out.mp4] [--draft]` | the video or a range of it (default command); `--draft` = half size + fastest preset |
| `frame 1s,50%,Intro@end [-o dir] [--svg]` | PNGs of single frames plus the problems found in them |
| `strip [RANGE] -n 12` | labelled contact sheet of evenly spaced frames, one image to review motion |
| `onion RANGE -n 6` | frames blended into one image, shows the path of a movement |
| `svg TIME` | the frame as SVG after conversion |
| `timeline` | size, fps, scenes with frame and second ranges, audio tracks |
| `inspect [RANGE] [--every 0.25s]` | missing media/fonts/glyphs, text cut by the canvas edge, NaN transforms, panics; exit code 2 on errors |
| `snapshot TIMES [--update]` | compare frames with approved PNGs, writes `.actual.png` and `.diff.png` |
| `audio render/analyze/at` | WAV of the mix, loudness report and waveform, tracks playing at a time |
| `preview [TIME] [--paused] [--mute]` | real-time GPU window (`fframes_native_player`), blocks until closed |

Global flags: `--json` (one JSON document on stdout, JSON progress events on stderr),
`--scale 0.5`. Times: frames `120`, `3.2s`, `1:05`, `50%`, `start`/`end`, scenes `Intro`
(`IntroScene` also matches), `#3`, `Intro[1]`, `Intro@1.5s`/`@50%`/`@end`; ranges `a..b`,
`a..`, `..b`, `all` or a scene name.

- Backends: `fframes::cpu::CpuRenderingBackend` (default, tiny-skia, multi-threaded) or the
  Skia backend from `fframes_skia_renderer` with `SkiaFFramesRenderer::new_vulkan(&SkiaVulkanCtx::new(W, H)?, SkiaPipelineConfig { .. })`
  (`features = ["vulkan"]`; `new_metal` with `metal`). The Skia backend walks the `svgr!` tree
  directly and caches static subtrees as pictures; it is roughly 10x faster than the CPU
  backend on 1080p (see `cargo run --release -p fframes_skia_renderer --features vulkan --example svgr_vs_skia`).
- macOS: request `hevc_videotoolbox` only with `fframes = { features = ["videotoolbox"] }`
  (see `examples/teej-podcast/Cargo.toml`), otherwise the encoder silently falls back.
- In code: `fframes::Previewer::new(&video, &options)` keeps fonts, images and caches between
  frames (`render`, `render_inspected`, `svg`, `inspect`, `timeline()`, `timeline_report()`);
  `RenderOptions::frame_range` renders part of a video with matching audio.
- A panic in `render_frame` fails the render with the frame, second and scene it happened in.
- `just render my-video` renders and opens the result; `just bench my-video` times it.

### 9. Editor preview

`editor/editor-bridge/lib.rs` is the only glue the editor needs:

```rust
#![cfg(target_arch = "wasm32")]
use fframes_editor_controller::prelude::*;
use my_video_example::{MyVideo, MyVideoMedia};

impl_wasm_bridge_for!(MyVideo<'static>, MyVideoMedia);

lazy_static! {
    static ref MEDIA: MyVideoMedia = MyVideoMedia::prepare().expect("static media");
}

#[wasm_bindgen]
pub fn create_wasm_bridge() -> WasmBridge {
    console_error_panic_hook::set_once();
    WasmBridge::new(MyVideo { media: &MEDIA, title: "World" }, &MEDIA)
}
```

- `just run my-video` builds the WASM bridge in watch mode, starts the Vite dev server and the
  editor; `just check-wasm my-video` type-checks the bridge without a browser.
- Anything renderer-only (`MediaDirectory`, `RenderOptions`, the Skia backend) must be behind
  `#[cfg(not(target_arch = "wasm32"))]`; the bridge builds the library with `default-features = false`.
- The editor cannot measure system fonts and decodes video frames through WebCodecs, so text
  helpers may return `None` and `get_synced_video_frame` falls back to `editor_fallback_image`.

### 10. Verify before you finish

Look at the video without playing it (`R` = `cargo run --release -p my-video --`):

```bash
$R timeline                         # the structure is what you intended
$R inspect                          # no errors or warnings in any frame
$R strip -n 12                      # overall flow; `strip Intro -n 8` for one scene
$R frame Intro@end,Outro@50%        # full size details, check text stays inside its boxes
$R audio analyze --waveform w.png   # levels, silence, cue positions
$R render                           # final file
cargo test -p my-video              # frame snapshots
just clippy && cargo fmt --all && just check-wasm my-video   # if it has an editor bridge
```

Snapshot the compile-time tree against the runtime parser with
`fframes_test_utils::assert_compile_time_svgr_eq_runtime(name, svgr)` (see
`examples/marketing/src/tests.rs`); the first run writes `_svgr_snapshots/`, later runs diff.
Pixel snapshots of frames: `fframes::snapshot::assert_frames(&mut previewer, &mut renderer, &["Intro@1s"], &Default::default())`.

## Important Notes

- Run `just clippy` before committing - warnings are errors
- Use `cargo fmt` for Rust formatting
- Use `yarn prettier --write .` for JS/TS/ReScript formatting
- Check WASM compilation with `just check-wasm <example>` when modifying editor bridge code
