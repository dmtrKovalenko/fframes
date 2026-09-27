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
cp -r examples/hello-world examples/my-video
rm -rf examples/my-video/{out.mp4,test_render,editor/node_modules,Cargo.lock}
```

Then rename every occurrence of `hello-world` / `hello_world` / `HelloWorld` in:

- `examples/my-video/Cargo.toml` (package name, `[lib] name`, `[[bin]] name`)
- `examples/my-video/src/{lib.rs,main.rs}` and the video module
- `examples/my-video/editor/editor-bridge/{Cargo.toml,lib.rs}` and `editor/package.json`

Register both crates in the workspace `Cargo.toml` `members` list:

```toml
"examples/my-video",
"examples/my-video/editor/editor-bridge",
```

and add `just check-wasm my-video` to the `check-examples` recipe in the `justfile`. Run
`yarn install` at the repository root once so the editor package picks up the new workspace.

The crate layout that the tooling expects:

```
examples/my-video/
  Cargo.toml                 # lib (cdylib + rlib) + bin gated by the `renderer` feature
  media/                     # fonts, images, audio, subtitles embedded via include_media_dir!
  dynamic_media/             # optional: large files read at runtime (renderer only)
  src/lib.rs                 # `pub mod my_video; pub use my_video::*;`
  src/my_video.rs            # the Video impl
  src/main.rs                # CLI that calls fframes::render
  editor/editor-bridge/      # WASM crate, see step 9
  editor/{index.html,main.tsx,package.json,vite.config.ts}
```

`Cargo.toml` essentials (feature names are ffmpeg codecs; keep `libav-agree-gpl` when using
x264/x265):

```toml
[dependencies]
fframes = { workspace = true, features = ["h264", "h265", "libav-agree-gpl"] }
clap = { version = "4.3.4", features = ["derive"] }

[features]
default = ["renderer"]
renderer = ["fframes/compile-time-svgtree"]   # the editor bridge builds with default-features = false

[lib]
crate-type = ["cdylib", "rlib"]
```

### 2. Media

Embed static assets with `include_media_dir!`. The path is relative to the **workspace root**
and every file in the folder becomes a field of the generated struct; fonts are registered by
their family name.

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

### 8. Render it

`main.rs` calls `fframes::render(output, &video, backend, &options)`; keep the CLI flags of the
hello-world example (`--output`, `--video-codec`, `--concurrency`, `--verbose`).

```rust
let media = MyVideoMedia::prepare()?;
fframes::render(
    "out.mp4",
    &MyVideo { media: &media, title: "World" },
    fframes::cpu::CpuRenderingBackend { cache_capacity: 20, ..Default::default() },
    &RenderOptions {
        media: Some(&media),
        logger: fframes_logger::FFramesLoggerVariant::Compact,
        video_encoder_options: EncoderOptions {
            preferred_encoder: Some("libx264"),
            codec_params: Some(&[("crf", "23"), ("preset", "slow")]),
            ..Default::default()
        },
        ..Default::default()
    },
)?;
```

- Backends: `fframes::cpu::CpuRenderingBackend` (default, tiny-skia, multi-threaded) or the
  Skia backend from `fframes_skia_renderer` with `SkiaFFramesRenderer::new_vulkan(&SkiaVulkanCtx::new(W, H)?, SkiaPipelineConfig { .. })`
  (`features = ["vulkan"]`; `new_metal` with `metal`). The Skia backend walks the `svgr!` tree
  directly and caches static subtrees as pictures; it is roughly 10x faster than the CPU
  backend on 1080p (see `cargo run --release -p fframes_skia_renderer --features vulkan --example svgr_vs_skia`).
- macOS: request `hevc_videotoolbox` only with `fframes = { features = ["videotoolbox"] }`
  (see `examples/teej-podcast/Cargo.toml`), otherwise the encoder silently falls back.
- One frame to a PNG (fast iteration): `fframes::render_frame(index, &video, backend, &options)`
  returns RGBA bytes; `examples/teej-podcast` exposes it as `--preview --preview-frame N`.
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

```bash
just clippy                                   # -D warnings
cargo fmt --all
just check-wasm my-video
cd examples/my-video && cargo run --release   # render, then look at frames:
ffmpeg -ss 5 -i out.mp4 -frames:v 1 frame5.png
```

Snapshot the compile-time tree against the runtime parser with
`fframes_test_utils::assert_compile_time_svgr_eq_runtime(name, svgr)` (see
`examples/marketing/src/tests.rs`); the first run writes `_svgr_snapshots/`, later runs diff.
Inspect at least one rendered frame per distinct layout and confirm text stays inside its boxes.

## Important Notes

- Run `just clippy` before committing - warnings are errors
- Use `cargo fmt` for Rust formatting
- Use `yarn prettier --write .` for JS/TS/ReScript formatting
- Check WASM compilation with `just check-wasm <example>` when modifying editor bridge code
