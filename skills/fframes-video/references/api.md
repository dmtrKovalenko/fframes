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
  are the struct names (`Intro`, `IntroScene` both match `IntroScene`). Override
  `fn name(&self) -> &'static str` to name scenes yourself, e.g. one `Shot { kind }` struct
  used for every scene and still addressed as `Smooch@2s` in the CLI.
- `Overlap` is added on top of `duration()`: with `Overlap::Previous(0.5)` the scene starts
  0.5 s before the previous one ends and runs `duration() + 0.5` s. To land scene `i` on a cue
  (a lyric, a beat) after a 0.5 s transition, use `duration = cue[i + 1] - cue[i]` and
  `Overlap::Previous(0.5)`; the scene then covers `cue[i] - 0.5 .. cue[i + 1]`. Check with
  `timeline`.
- `ctx.render_scenes(&frame)` in the video's `render_frame` renders the active scene(s).
- `frame.index` / `frame.seconds()` are scene relative inside a scene; `frame.global_index`
  is the video frame. `ctx.get_scene_info(&self.intro)` gives a scene's resolved range.
- `ctx.current_video_size` is the output size (scaled by `--scale`); trees drawn at
  `WIDTH`x`HEIGHT` are scaled automatically.

## Animation

For motion with fixed values, put `frame.animate(fframes::timeline!(...))` directly in the
`svgr!` attribute. The SVG macro lifts the timeline into a `lazy_static` animation, resolving
its easing runtimes once. It is fine to define the easing directly in the `timeline!` block;
named constants are optional.

```rust
use fframes::{Color, Transform, animation::Easing};

// keyframes: at START [=> END | , duration D], animate FROM => TO, EASING
fframes::svgr!(
    <g opacity={frame.animate(fframes::timeline!(
        at 0.0 => 0.3, animate 0.0 => 1.0, Easing::EaseOut,
    ))}
    transform={frame.animate(fframes::timeline!(
        at 0.0, animate Transform::translate(0, 60) => Transform::translate(0, 0),
            Easing::Spring { mass: 1.0, stiffness: 180.0, damping: 20.0 },
        at 2.5 => 2.8, animate Transform::translate(0, 0) => Transform::translate(0, -40),
            Easing::EaseIn,
    ))}>
        <rect x="160" y="240" width="600" height="120" rx="24"
            fill={frame.animate(fframes::timeline!(
                at 0.0 => 1.0, animate Color::hex("#fff") => Color::hex("#0ea5e9"),
                    Easing::CubicBezier(0.65, 0.0, 0.35, 1.0),
            ))}
        />
    </g>
)
```

- The optimizer recognizes an attribute block whose sole expression is
  `frame.animate(timeline!(...))` (also with `&timeline!(...)`). Use `f32` literals or
  imported `Color` / `Transform` constructors for the animated values; the cached timeline
  cannot capture locals such as `self`, a loop index, or a runtime start time.
- This caching belongs to `svgr!`, not `timeline!` alone. Calls outside the macro, nested
  inside other expressions, and `animate_loop` do not use this optimization. For those,
  initialize `KeyFramesAnimation` fields once in the constructor and sample with
  `frame.animate(&self.animation)` / `frame.animate_loop(&self.animation)`. Data-dependent
  staggering can also build one timeline per item in the constructor. Camera parameters,
  shader uniforms and custom projection can consume these cached animations.
- Before the first keyframe the value is `from`, after the last it stays at `to`.
- Stored timelines also support `f64` and other types implementing `Animatable` with the
  required bounds; the inline optimizer supports the built-in forms listed above.
- When a start time changes after construction, use `frame.animate_runtime(AnimateRuntimeInput {
  on_second, from, to, animation_runtime: &self.runtime })`, with `self.runtime` initialized
  once using `AnimationRuntime::new(duration, &easing)`. Known item offsets belong in cached
  `timeline!` values, not per-frame runtime construction. For springs, an explicit duration
  caps the natural settle time; omit the end in `timeline!` to let the spring settle.
- For precomputed geometry, motion-blur subframes or a custom clock,
  `runtime.solve(&elapsed_seconds)` evaluates the same easing
  directly. Handle times before the start and after `get_duration()` explicitly: `solve`
  does not clamp them. Use a custom curve only when the required motion is not represented
  by the available easings.
- Easing: `Linear`, `EaseIn`, `EaseOut`, `EaseInOut`, `CubicBezier(x1, y1, x2, y2)`, `Spring {..}`.
- `Transform { translate_x, translate_y, scale: Scale { x, y }, rotate, .. }`,
  `Transform::translate(x, y)`. Scale around a point by translating by `p * (1 - s)`.

### Matching a fractional-rate reference

`Video::FPS` is currently an integer. A 30000/1001 fps source played at 30 fps has different
timing even when every frame is retained. Keep edit boundaries as integer source frames;
for source-time effects, compute `source_frame * fps_den / fps_num` and sample an easing
runtime or bind a custom shader clock. Ordinary `frame.animate` follows the frame's integer
fps; placing keyframes at `source_frame / Video::FPS` preserves their frame positions.

When exact source timing is required, retime the final export and verify every frame is
retained. For the specific 30000/1001 case, FFmpeg video options
`-vf 'settb=1/30000,setpts=N*1001' -r 30000/1001 -fps_mode cfr -video_track_timescale 30000`
give a 1001-tick frame interval. Do not combine output `-r` with `-fps_mode passthrough`.
This export step does not change the native preview's clock. See `audio.md` for preserving
the original soundtrack during muxing.

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
- Built-in names take precedence over user uniforms: `.float("iTime", t)` does not override
  the renderer's time. Declare a different name such as `uClock` for a continuous clock
  across cuts, source time, or time remapping. `iTime` uses the `Frame` passed to `draw`.
- `coord` starts at the shader image's top-left in its own units. `iResolution` is that
  image's width and height, not the full video or a bound texture's size. For a cropped
  card/window layer, pass its origin and the full viewport size separately when projecting
  in screen space. Child shaders sample source-image pixels with `.eval(...)`.
- A synced video frame is an image too, so shaders can process footage on the GPU, e.g. a
  green-screen key (bind the `SyncVideoFrameInput` to a variable first, the frame borrows it):

```rust
let input = SyncVideoFrameInput { start_from: 0., looping: false, editor_fallback_image: None };
let Some(clip) = frame.get_synced_video_frame(ctx, "clip.mp4", &input) else { return Svgr::empty() };
let layer = self.chroma.draw(&frame, ShaderUniforms::new()
    .image("uSrc", &clip.into_image())
    .float2("uSrcSize", clip.width() as f32, clip.height() as f32));
```

```glsl
uniform float3 iResolution; uniform shader uSrc; uniform float2 uSrcSize;
half4 main(float2 coord) {
    half4 c = uSrc.eval(coord / iResolution.xy * uSrcSize);   // source pixels
    float a = 1.0 - smoothstep(0.06, 0.2, c.g - max(c.r, c.b)); // green dominance
    return half4(c.rgb * a, a);                                 // premultiplied
}
```
- SkSL follows GLSL ES 2: constant loop bounds, no `while`, no dynamic array indexing, no
  preprocessor. `Shader::shadertoy` expands simple `#define`s; rewrite macros with arguments
  and `texture()` calls.
- A compile error is logged once and the layer is skipped. Validate compilation with
  `fframes_skia_renderer::render::compile_shader(&shader)` when needed; do not add a test
  suite or test module for it.
- `inspect` checks the SVG tree without executing shaders. Also compile the effects, look
  at rendered frames, and check runtime logs for unset uniforms; a clean inspection alone
  does not establish that a shader ran correctly.
- Both Skia GPU and `SkiaCpuCtx` execute runtime shaders. The separate tiny-skia CPU backend
  draws nothing in their place. Use the GPU backend for the native preview and GPU validation.

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
```

Skia: `SkiaFFramesRenderer::new_metal(&SkiaMetalCtx::new(W, H)?, SkiaPipelineConfig::default())`
(`fframes_skia_renderer` feature `metal`) or `new_vulkan` with `SkiaVulkanCtx` (feature
`vulkan`). `backend.frame_renderer()`
returns a renderer for single frames that matches the backend (Skia on the GPU for Skia).

The Skia backend picks the fastest way to hand frames to the encoder on its own: hardware
frames (`*_videotoolbox` encoders with `new_metal`; `h264_vulkan`/`hevc_vulkan` with
`SkiaVulkanCtx::new_shared_with_encoder` and the `vulkan-video` feature), else conversion to
the encoder's YUV format on the GPU, else on the CPU. `.frame_export(SkiaFrameExport::CpuConversion)`
on the renderer turns that off.

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
