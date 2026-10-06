# Shader mode

A native Skia GPU example promoting fframes shader mode. The edit follows the
supplied reference's 932 source frames: material cuts, panel-wall camera,
product sequence, black breaks and closing tail. The reference soundtrack is
used unchanged. The typography and shader compositions retain fframes branding;
this is a reconstruction, not a pixel-identical copy of the original artwork.

`src/edit.rs` is the edit list. `reference-edit.json` documents the source intervals
and requested changes: the amber shot stays level, the open-source shot keeps its
colors, and the two native-window shots are combined. The resulting edit has 41
segments, including four black intervals and the short material/camera cuts.

## Prepare and preview

```sh
python3 examples/shader-mode/tools/prepare_media.py /path/to/reference.mp4 /path/to/native-recording.mov
cargo build --release -p shader-mode
cargo run --release -p shader-mode -- preview
```

The preparation defaults select **24.1–26.5 seconds** of the manually scrubbed
recording, after the window was resized. Its window bounds were checked every
0.1 seconds throughout that selection. The crop is 2824×1652 at (68,54), keeping
the title bar, cursor and playback controls. The recording plays at normal speed;
the visible playhead moves from about 00:06 to 00:12, then back to 00:09. The
native insert is one continuous 71-frame shot (about 2.37 seconds) with no camera
whip between its former two segments. The window turns slowly in 3D with real
perspective; its corners and scrub controls stay inside the frame.
Supply `--start`, `--end` and `--crop`
when using a different recording. External audio and recordings stay in the
ignored `dynamic_media/` directory. The bundled poster supplies a fallback when the
recording is unavailable.

The original AAC packets are copied into `reference-audio.m4a` and verified by
SHA-256. A stereo float PCM decode is used for native preview because this
checkout's `MediaDirectory` does not recognize M4A. No gain, fades or limiter
are applied. Final export copies the untouched AAC stream directly.

The framework's `Video::FPS` is an integer, so native preview uses 30 fps. Each
source frame is retained and shader animation uses the source's rational clock.
The optional export command below restores 30000/1001 fps exactly. The 30 fps
preview is approximately 31 ms shorter than the source's video stream.

## Review without rendering a video

```sh
cargo run --release -p shader-mode -- timeline
cargo run --release -p shader-mode -- inspect --all-frames --fail-on warning
cargo run --release -p shader-mode -- frame ShaderLibrary@0.4s,NativePreview@1s,Eclipse@2.5s -o examples/shader-mode/output/review
cargo run --release -p shader-mode -- strip -n 40 -o examples/shader-mode/output/strip.png
```

The native player, stills and contact sheets all use Metal on macOS and Vulkan
elsewhere. This example has no browser bridge. The prepared preview can be opened
at `preview NativePreview --paused`, or `preview ShaderLibrary` for the panel wall.

## Motion around the native recording

| Source time | Motion |
| --- | --- |
| 16.116–16.550 | Blue pearlescent relief, continuing its previous motion |
| 16.550 | Hard cut into the native preview window |
| 16.550–18.919 | Continuous recorded forward/backward scrub with a slow 3D turn |
| 18.919 | Hard cut to a framed shader-title composition |
| 19.553 | Punch into the title composition |
| 20.287–20.354 | Brief horizontal glitch into the full composition |
| 21.221–21.355 | Horizontal breakup into the black pause |

## Binary opening

The opening has a separate shader from the later ASCII shot. Reference frames
16–25 have a measured 30-pixel horizontal and vertical glyph pitch: a 64×36 grid
at 1080p, using `0` for bright cells and `1` for dark cells. The previous version's
13-pixel, mixed-character ring and whole-frame skew did not reproduce that image.

Frames 6–15 reveal cyan and violet patches through displaced horizontal bands
and black cutouts. Frames 16–25 hold a square grid while the lit blocks move.
Frames 26–32 expand the binary field into the glass shot with radial shutter
sampling, a brief brightness rise and an overlap. The glass title enters after
that overlap. The light-field geometry is a procedural reconstruction from the
reference frames; the upstream repository does not include this video's scene
configuration.

## Panel wall and outro

The panel wall uses the reference's 8.542–10.244-second interval: rapid pullback,
frontal hold, individual cards coming apart in 3D, then an accelerating push
through a gradient icon. The separation begins around source frame 280 (9.34s).
Each card has its own rotation, depth and downward drift, producing changing
gaps and overlaps. Card widths vary with their labels, rows are staggered, and
each card has a small material swatch.

`catalog.rs` prepares the 51 frames of camera-space geometry once, sorts cards
from back to front, and clips their projected bounds to the viewport. The GPU
intersects each card's plane for perspective-correct atlas sampling and a shallow
edge. The icon's projected outline also clips the vector transition title.
Sixteen shutter samples apply only during the fast camera moves; settled text
uses a single sharp sample. The artwork and individual trajectories are original;
their release and camera timing are reconstructed from the reference.

The closing wordmark uses real Instrument Serif glyphs and the staggered orange
letter animation from `examples/fframes-intro`, followed by the reference's shrink
and black tail. It is not a bitmap logo.

## Optional final export

Only run this when a final video is wanted:

```sh
python3 examples/shader-mode/tools/render_reference.py --binary target/release/shader-mode -o examples/shader-mode/output/shader-mode.mp4
```

This explicitly renders the intermediate 30 fps sequence, restores the source
clock, copies the original AAC stream, and verifies frame count, frame rate and
audio packet equality. Ordinary `preview`, `frame`, `strip` and inspection
commands do not run it.

## Source adaptations

Upstream is pinned to commit `935f71a7789f0e07811dfe6fd0d8f707e9848238`.
All paths below are relative to its `packages/core/src/` directory.
The MIT copyright and permission notice is retained in
[THIRD_PARTY_NOTICES](THIRD_PARTY_NOTICES).

| Local implementation | Upstream source and preserved behavior |
| --- | --- |
| `glass.sksl` | `gpu/kit/effects/glass.ts`: chord-gradient lens offset, slope cap, depth envelope, RGB dispersion, highlight and Fresnel terms |
| `source-effects.sksl`, `material.sksl` | `shaders/Chrome/index.ts`: rectangular studio key, flags, fringe and floor card; `gpu/kit/effects/bevel.ts`: round/machined profile; `gpu/kit/tonemap.ts`: neutral shoulder |
| `material.sksl` | `shaders/CarbonFiber/index.ts`, `gpu/kit/materialParts.ts`: twill parity, tow relief, anisotropic sheen and five-softbox clearcoat |
| `source-effects.sksl`, `glass.sksl`, `eclipse.sksl` | `shaders/FlowingGradient/index.ts`: rotating noise field, sequential shear, two domain-warp levels, variable edge width and fold lighting |
| `sculpture.sksl`, `liquid-chrome.sksl`, `obsidian.sksl` | The shared `Chrome` studio above, with an added strip light, illuminates new raymarched geometry |
| `obsidian.sksl` | `shaders/Obsidian/index.ts`: grazing-angle iridescence and Fresnel-weighted reflection inspire a new cobalt/lavender/petrol coating; geometry, optical-path palette and occlusion are custom |
| `emboss.sksl` | `gpu/kit/effects/emboss.ts`: relief-height normal, directional edge light and two cast-shadow samples |
| `rays.sksl` | `gpu/kit/gradientPaints.ts`, `gpu/kit/lightfields.ts`: seamless angular coordinates and two layers of multiplicative noise rays |
| `ascii.sksl` | `gpu/kit/stylizePaints.ts`: cell-center sampling, gamma-corrected luminance-to-glyph selection and glyph tint |
| `binary-intro.sksl` | The same ASCII grid and luminance lookup with a separate `01` atlas; `gpu/kit/motionBlur.ts`: multi-scale horizontal band displacement and localized RGB split |

The SkSL ports use a floating-point hash in place of the WebGPU integer noise
hash and blend the selected colors in RGB. The source's feedback particle
simulations depend on WebGPU storage buffers and compute passes; the particle
and flow shots here are stateless SkSL reconstructions, not ports of those
simulators. Tunnel, halftone, camera placement and the eclipse composition are
also reconstructions. Liquid-chrome geometry, the twisted ribbon, aurora and
spectral satin are new compositions. The source material and the existing
`examples/shaders` raymarcher informed these effects; they are not exports of
upstream presets.

Rebuild the bundled textures with `python3 examples/shader-mode/tools/assets.py`
(Pillow, NumPy, SciPy and fontTools). Python is not needed for Cargo builds.
All titles remain vector SVG text. Skia compiles and caches each shader once;
`ShaderUniforms::image` binds the glyph atlas, distance field, card atlas and
actual decoded recording frames. `uClock` carries rational source time while
`iResolution` remains the built-in layer size.

Use CLI inspection to check all 932 visual frames, then review rendered frames,
contact sheets and the native preview. Inspection checks the SVG tree without
executing shaders; rendered frames and runtime logs reveal shader compilation
or uniform problems. The native preview uses the prepared audio and recording.
