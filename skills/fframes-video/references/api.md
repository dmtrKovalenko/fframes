# fframes API cheat sheet

Examples in the fframes repository worth reading: [motion-graphics](https://github.com/dmtrKovalenko/fframes/tree/main/examples/motion-graphics)
(springs, text fitting), [beta](https://github.com/dmtrKovalenko/fframes/tree/main/examples/beta) (scenes, images), [audio-announce](https://github.com/dmtrKovalenko/fframes/tree/main/examples/audio-announce)
(audio visualisation, video sources), [teej-podcast](https://github.com/dmtrKovalenko/fframes/tree/main/examples/teej-podcast) (Skia backend, stereo
media loaded at runtime), [shaders](https://github.com/dmtrKovalenko/fframes/tree/main/examples/shaders) and [neon-triangle](https://github.com/dmtrKovalenko/fframes/tree/main/examples/neon-triangle) (GPU
shaders).

## svgr!

SVG markup with Rust in braces. Attributes and text children take strings, numbers, `Color`,
`Transform`, another `Svgr`, or a `Vec<Svgr>` / iterator collected to a `Vec`.

```rust
fframes::svgr!(
    <g transform={Transform::translate(x, y)} opacity={opacity}>
        // comments are `//` lines
        <rect x="0" y="0" width={w} height="80" rx="16" fill={ACCENT} />
        <text x="24" y="54" font-family="Inter" font-size="40" font-weight="600" fill="#fff">
            "Label: " {value.to_string()}
        </text>
        {items}   // Vec<Svgr>
    </g>
)
```

- Literal text children are quoted strings. `Svgr::empty()` renders nothing.
- Subtrees without `{}` are hashed at compile time and cached by the renderers: keep static
  decoration static, wrap animated values around it.
- Gradients, clip paths, masks, filters go into `<defs>` like normal SVG and are referenced
  with `url(#id)`. Give ids unique names per component instance if repeated.
- Images: `href={self.media.logo_png.href()}` (field named after the file in `media/`) or
  `ctx.get_image("logo.png").map(|i| i.href())`.

## Video and Scene

```rust
impl Video for MyVideo<'_> {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;
    const BACKGROUND_COLOR: Color = Color::BLACK;   // painted under every frame
    fn duration(&self) -> Duration<'_>;             // Seconds(f32), Frames(usize), FromAudio("f"), FromVideo("f"), Auto, a + b
    fn audio(&self) -> AudioMap<'_>;                // AudioMap::none() or tracks
    fn define_scenes(&self) -> Scenes<'_>;          // optional
    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a>;
}

impl Scene for Intro<'_> {
    fn duration(&self) -> Duration<'_>;
    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a>;
    fn overlap(&self) -> Overlap { Overlap::Previous(0.4) }  // optional cross-fade window
    fn audio(&self) -> AudioMap<'_> { .. }                   // optional, relative to the scene
}
```

- Scenes must be zero sized or fields of the video (`&self.intro`). Scene names in the CLI
  are the struct names (`Intro`, `IntroScene` both match `IntroScene`).
- `ctx.render_scenes(&frame)` in the video's `render_frame` renders the active scene(s).
- `frame.index` / `frame.seconds()` are scene relative inside a scene; `frame.global_index`
  is the video frame. `ctx.get_scene_info(&self.intro)` gives a scene's resolved range.
- `ctx.current_video_size` is the output size (scaled by `--scale`); trees drawn at
  `WIDTH`x`HEIGHT` are scaled automatically.

## Animation

```rust
use fframes::{Transform, animation::Easing};

// keyframes: at START [=> END | , duration D], animate FROM => TO, EASING
let x = frame.animate(&fframes::timeline!(
    at 0.0, animate -80.0_f32 => 0.0, Easing::Spring { mass: 1.0, stiffness: 180.0, damping: 20.0 },
    at 2.5 => 2.8, animate 0.0_f32 => 60.0, Easing::EaseIn,
));
let color = frame.animate(&fframes::timeline!(at 0.0 => 1.0, animate Color::hex("#fff") => Color::hex("#0ea5e9"), Easing::EaseInOut));
let t = frame.animate(&fframes::timeline!(at 0.0 => 0.5, animate Transform::scale(0.9) => Transform::scale(1.0), Easing::EaseOut));
let drift = frame.animate_loop(&fframes::timeline!(at 0.0 => 8.0, animate 0.0_f32 => 1.0, Easing::Linear));
```

- Before the first keyframe the value is `from`, after the last it stays at `to`.
- Values: `f32`, `f64`, `Color`, `Transform`.
- Runtime start times (staggering, data driven): `frame.animate_runtime(AnimateRuntimeInput {
  on_second, from, to, animation_runtime: &AnimationRuntime::new(duration, &easing) })`.
  For springs pass a generous duration (3.0): the spring stops at its own settle time.
- Easing: `Linear`, `EaseIn`, `EaseOut`, `EaseInOut`, `CubicBezier(x1, y1, x2, y2)`, `Spring {..}`.
- `Transform { translate_x, translate_y, scale: Scale { x, y }, rotate, .. }`,
  `Transform::translate(x, y)`. Scale around a point by translating by `p * (1 - s)`.

## Text

```rust
use fframes::{BreakLinesOpts, FontQuery, TextAlign, TextOverflow};
let font = FontQuery { family: "Inter", size: 48, weight: 500, ..Default::default() };
let w: Option<usize> = frame.text_width(ctx, font, "Title");
// text_fit returns Option<Cow<str>>; svgr! needs a String or &str, so convert it
let fitted: String = frame
    .text_fit(ctx, font, long_title, 900, TextOverflow::Ellipsis)
    .map(|text| text.into_owned())
    .unwrap_or_else(|| long_title.to_owned());
// text_break_lines returns Option<Svgr>: a ready <text> with one <tspan> per line
let paragraph: Svgr = frame
    .text_break_lines(ctx, text, BreakLinesOpts {
        font, width: 900, x: 160, y: 400, align: TextAlign::Left, fill: "#0f172a", ..Default::default()
    })
    .unwrap_or_default();
```

The helpers take `&mut frame` and return `None` when the font is not loaded; use the same
family, size and weight in the `<text>` attributes. `text-anchor="middle"` centers on `x`.

## Media

- `include_media_dir!(pub struct MyMedia, "media")` embeds a folder; the path is relative to
  the Cargo workspace root, which in a generated project is the project folder. Call
  `MyMedia::prepare()` in `main.rs`. Fonts register by family name, audio decodes to mono.
- Large or stereo files: `let dir = MediaDirectory::read_folder("assets")?; let media = dir.process_media_source()?;`
  and combine providers with `CombinedMediaProvider::from([&static_media as &dyn MediaProvider, &media])`.
- Video frames: `frame.get_synced_video_frame(ctx, "clip.mp4", &SyncVideoFrameInput { start_from, looping, editor_fallback_image })`
  then `<image href={video_frame.href()} .../>` (needs the `compile-time-svgtree` feature).
- Subtitles: `ctx.get_subtitles("subs.vtt").and_then(|s| frame.get_subtitle_phrase(s))`.
- Audio spectrum: `frame.visualize_audio_frame(VisualizeFrameInput { audio, sample_size: SampleSize::S256, smooth_level: 4, window: Some(WindowFunction::Hann) })`.

## GPU shaders (Skia backend)

`fframes::Shader` runs SkSL, or pasted Shadertoy GLSL, on the Skia GPU surface while the frame
is drawn. SVG transforms, clips, masks and opacity apply to the layer like to any element.

```rust
use fframes::{Shader, ShaderUniforms};

// once, in the constructor (the renderer compiles and caches it)
let aurora = Shader::sksl(include_str!("shaders/aurora.sksl"));   // half4 main(float2 coord)
let torus = Shader::shadertoy(include_str!("shaders/torus.glsl")); // void mainImage(out vec4, in vec2)

// in render_frame
let layer = self.aurora.draw(&frame, ShaderUniforms::new()
    .float("uSpeed", 0.6)
    .color("uTint", frame.animate(&tint)));
fframes::svgr!(<image href={layer.href()} x="0" y="0" width="1920" height="1080" />)
```

- Built-in uniforms when declared: `iResolution` (the `<image>` size), `iTime`, `iTimeDelta`,
  `iFrame`. Pass images with `.image("iChannel0", photo)` where `photo` comes from
  `ctx.get_image("photo.jpg")` (an `Option`: return `Svgr::empty()` when it is missing).
- SkSL follows GLSL ES 2: constant loop bounds, no `while`, no dynamic array indexing, no
  preprocessor. `Shader::shadertoy` expands simple `#define`s; rewrite macros with arguments
  and `texture()` calls.
- A compile error is logged once and the layer is skipped. Test compilation with
  `fframes_skia_renderer::render::compile_shader(&shader)`.
- Only the Skia backend runs shaders; the CPU backend draws nothing in their place.

## Rendering from code

```rust
let options = RenderOptions {
    media: Some(&media),
    frame_range: Some(10 * 30..20 * 30),     // optional: part of the video, audio cut to match
    scale_resolution: 0.5,                    // optional: half size
    audio_mix: AudioMixOptions::default(),    // limiter -1 dBFS, de-click fades
    default_font: "Inter",
    ..Default::default()
};
fframes::render("out.mp4", &video, fframes::cpu::CpuRenderingBackend::default(), &options)?;

// many frames quickly, and checks:
let mut previewer = fframes::Previewer::new(&video, &options)?;
let frame = previewer.timeline().resolve_frame("Intro@1.5s")?;
let (png, report) = previewer.render_inspected(frame, &mut fframes::CpuFrameRenderer::default())?;
png.save_png("intro.png")?;
let svg = previewer.svg(frame)?;
let problems = previewer.inspect(frame)?.diagnostics;
fframes::snapshot::assert_frames(&mut previewer, &mut renderer, &["Intro@end"], &Default::default());
```

Skia: `SkiaFFramesRenderer::new_metal(&SkiaMetalCtx::new(W, H)?, SkiaPipelineConfig::default())`
(`fframes_skia_renderer` feature `metal`) or `new_vulkan` with `SkiaVulkanCtx` (feature
`vulkan`). `backend.frame_renderer()`
returns a renderer for single frames that matches the backend (Skia on the GPU for Skia).

## The project's command line

`main.rs` builds it with `fframes::cli::new`; everything after it is optional:

```rust
let args = fframes::cli::parse::<MyArgs>();   // only when the video uses flags of its own
let video = MyVideo::new(&media, &args.app.title);
fframes::cli::new(&video, options)
    .args(args)
    .backend(SkiaFFramesRenderer::new_metal(&gpu, SkiaPipelineConfig::default())?)
    .preview(fframes_native_player::cli_preview)
    .default_output("out.webm")
    .run()
```

## Gotchas

- Zero width/height rects and zero radius circles are invalid SVG and are skipped (inspect
  reports them): animate from `0.5`, not `0`.
- A panic in `render_frame` stops the render; the error names frame, second and scene.
- `font-weight` must be numeric (`600`) or `normal`/`bold`. Unknown families fall back
  silently in the renderer, `inspect` reports "No match for ... font-family".
- `Color::TRANSPARENT` backgrounds need an alpha capable encoder and pixel format.
- Heavy filters (`feGaussianBlur`, `feDropShadow`) on animated subtrees are slow on the CPU
  backend: keep them static or use Skia.
- Durations from audio (`Duration::FromAudio`, `Eof`) need the file in the media provider;
  missing files fail early with `RequiredAudioNotFound`.
