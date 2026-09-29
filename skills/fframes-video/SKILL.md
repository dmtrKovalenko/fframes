---
name: fframes-video
description: >-
  Create, animate, review and render videos in code with fframes (Rust, SVG scenes, ffmpeg
  encoding, Skia GPU rendering). Use whenever the user wants a video, animation, motion graphic,
  explainer, promo, social clip, title card, podcast visual, lyric/caption video or any rendered
  .mp4/.webm made programmatically, or asks to change, fix, speed up or check an fframes video.
  Covers installing fframes, creating a project, designing good-looking motion, sound (music,
  voice, SFX, loudness), watching it in a real-time preview window, and checking the result
  without watching it: frames and contact sheets as PNG, automatic problem detection, loudness
  numbers, fast draft and range renders.
---

# Making videos with fframes

[fframes](https://github.com/dmtrKovalenko/fframes) is a Rust library that renders video from
code. A video is a Rust struct with a `render_frame(frame) -> Svgr` method: for every frame it
returns an SVG tree, built with the `svgr!` macro (SVG markup with `{rust expressions}`).
Scenes split the timeline, `timeline!` and springs animate values, an audio map places sound,
and ffmpeg encodes the result.

Each project gets a command line (from `fframes::cli`). Use it for everything: render a video,
open the real-time preview window, and look at frames, contact sheets and loudness numbers as
files you can read. Read `references/design.md` before designing, `references/api.md` while
writing code and `references/audio.md` for sound.

## 1. Install

Rust from <https://rustup.rs>, then the system libraries ffmpeg is built with:

```bash
# macOS
brew install pkg-config ffmpeg x264 x265 opus nasm ninja
# Debian / Ubuntu
sudo apt-get install -y yasm nasm ffmpeg libx264-dev libx265-dev libopus-dev libclang-dev clang ninja-build libvpx-dev libasound2-dev
# Arch
sudo pacman -S ninja yasm nasm ffmpeg x264 x265 opus clang
```

Windows links a prebuilt FFmpeg 9 shared build instead (set `FFMPEG_DIR` to the unzipped
`ffmpeg-n9.0-latest-win64-gpl-shared` build from BtbN/FFmpeg-Builds, add its `bin` to `PATH`,
install LLVM with `winget install LLVM.LLVM` and set `LIBCLANG_PATH`). Details are in the
fframes README.

## 2. Create a project

```bash
curl -fsSL https://raw.githubusercontent.com/dmtrKovalenko/fframes/main/scripts/new-video.sh | bash -s -- my-video --yes
# or install the generator once and use it directly:
cargo install --locked cargo-fframes
cargo fframes new my-video --format landscape --fps 30 --yes
```

Options: `--template single-scene|multi-scene`, `--format landscape|portrait|square|uhd`,
`--fps`, `--title`, `--backend`, `--dir`. The project depends on the fframes release that
matches the installed `cargo-fframes`; `--git` uses the repository's `main` branch instead. Always pass `--yes` so nothing waits for input.

Templates:
- `single-scene` (default): one scene. It adapts to every format; start here for single-shot
  clips, title cards and portrait video.
- `multi-scene`: two scenes. Start here for anything with several scenes; it only accepts
  `landscape` and `uhd`.

The template content is placeholder code that shows the API. Replace its layout, colors,
fonts and decorations with a design made for the video the user asked for.

**Use the Skia GPU backend (the default).** `cargo fframes new` picks Skia on Metal (macOS) or
Vulkan (Linux, Windows). It renders about 10x faster than the CPU backend and gives you the
real-time `preview` window. On macOS and Linux the first build downloads prebuilt Skia and ffmpeg
and only compiles the Rust dependencies (about a minute; other targets, or `metal` and `vulkan`
together, compile Skia from source for ~20 minutes). Start it right away and write the video
while it runs:

```bash
cd my-video && cargo build --release        # run in the background, the first build is the slow one
```

`--backend cpu` needs no Skia but has no preview window; pick it only when there is no GPU.

The project renders as generated:

```
my-video/
  src/lib.rs        # the video (from the template)
  src/main.rs       # the command line
  media/            # fonts, images and audio compiled into the binary (one starter font)
  tests/frames.rs   # frame snapshots and a check of every frame for problems
  README.md         # the commands below
```

Every command below is `cargo run --release -- <command>`. Define a short shell function (a
variable like `R="cargo run --release --"` does not split into words in zsh, the macOS shell):

```bash
R() { cargo run --release -- "$@"; }
```

## 3. The loop

After every change:

1. `$R timeline` lists the scenes with their frame and second ranges, and every audio track.
   Check structure and pacing here before looking at pixels.
2. `$R inspect` checks a frame every 0.25 s plus the first and last frame of every scene. It
   reports missing images, fonts or glyphs, text cut off by the edge of the canvas, invalid
   SVG (zero-sized rectangles, bad radii), broken transforms and panics, each with its time
   and scene. It exits with code 2 on errors. Fix everything it reports. A warning that only
   appears on a scene's first frames is usually an entrance; check it with a strip.
3. `$R strip <scene or range> -n 12` writes a contact sheet (`strip.png`): evenly spaced frames
   in one labelled image. Open it with your image tool. It is the fastest way to judge layout,
   rhythm and motion.
4. `$R frame Intro@end,Outro@50%` writes full-size PNGs into `frames/` for detail checks
   (typography, alignment, contrast) and prints problems found in those frames.
5. `$R onion "Intro@0..Intro@1s" -n 6` blends frames into one image (`onion.png`) to show the
   path and spacing of a movement: easing, overshoot, stagger.
6. `$R preview Intro` opens a real-time window with sound for the user to watch (space
   play/pause, h/l seek a second, j/k step a frame, q quit). It blocks until closed, so start
   it in the background or ask the user to run it. Offer it whenever the user wants to see
   the video; you review with strips and frames, the user watches in the preview.
7. `$R render Intro --draft` encodes one scene at half resolution in about a second;
   `$R render` writes the final `out.mp4`.

Rules:
- Look at the PNGs before saying anything looks good.
- Prefer `strip` to many `frame` calls; add `--scale 0.5` when composition is all you need.
- Add `--json` when parsing output: the result goes to stdout, progress to stderr.
- Keep `--release`: debug builds render many times slower.
- `cargo test` compares settled frames with approved snapshots in `_frame_snapshots/` and
  fails if any frame has warnings. The first run only creates the snapshots: look at the PNGs
  before committing them, a created baseline is not a reviewed one.
  `FFRAMES_UPDATE_SNAPSHOTS=1 cargo test` accepts intended changes. Snapshot the middle of a
  scene (`Intro@3s`), not its end where it fades out.

### Addressing time

`120` (frame), `3.2s`, `500ms`, `1:05.5`, `50%`, `start`, `end`; scenes by struct name, `Intro`
(also matches `IntroScene`, case-insensitive), `#3` (scene index), `Intro[1]` (second scene of
that type); inside a scene `Intro@1.2s`, `Intro@12`, `Intro@50%`, `Intro@end`. Ranges: `a..b`
(end exclusive), `a..`, `..b`, `all`, or a scene name for the whole scene. Separate several
times with commas.

### All commands

| command | use it to |
| --- | --- |
| `timeline` | see scenes, durations, audio tracks and their mix settings |
| `inspect [RANGE] [--every 0.1s \| --all-frames] [--fail-on warning]` | find problems without rendering pixels |
| `strip [RANGE] -n N [--columns 4] [--width 480]` | review flow and motion in one image |
| `frame TIMES [-o dir] [--svg]` | full-size PNGs (and the laid-out SVG) of chosen frames |
| `onion RANGE -n N` | see a movement's trajectory and easing |
| `svg TIME` | read a frame's final SVG as text (positions, text, colors) |
| `preview [TIME] [--paused] [--mute]` | real-time window with sound, for the user |
| `render [RANGE] [-o out.mp4] [--draft]` | encode the video or a part of it (audio cut to match) |
| `snapshot TIMES [--update]` | compare frames with approved PNGs, `.diff.png` marks changes |
| `audio analyze [RANGE] [--waveform w.png]` | loudness (LUFS), true peak, clipping, silence, per scene |
| `audio at TIMES` | which sounds play at a moment, where in their file and how loud |
| `audio render [RANGE] -o a.wav` | the mix as a WAV file |

### Browser editor

fframes also has a web editor with a timeline and scrubbing, useful when a person wants to
tweak a video interactively. It runs the video compiled to WebAssembly and needs Node.js plus
`wasm-pack`; the setup is the `editor/` folder of the hello-world example in the fframes
repository (<https://github.com/dmtrKovalenko/fframes/tree/main/examples/hello-world>).
Mention it when the user asks for a GUI editor. For watching use `preview`, and for your own
review use the CLI.

## 4. Writing the video

```rust
impl Video for MyVideo<'_> {
    const FPS: usize = 30; const WIDTH: usize = 1920; const HEIGHT: usize = 1080;
    fn duration(&self) -> Duration<'_> { Duration::Auto }            // sum of the scenes
    fn audio(&self) -> AudioMap<'_> { AudioMap::none() }              // see references/audio.md
    fn define_scenes(&self) -> Scenes<'_> { Scenes::from(vec![&self.intro as &dyn Scene, &self.main]) }
    fn render_frame<'a>(&'a self, frame: Frame, ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        fframes::svgr!(<svg xmlns="http://www.w3.org/2000/svg" width={Self::WIDTH} height={Self::HEIGHT}>
            <rect width={Self::WIDTH} height={Self::HEIGHT} fill={BACKGROUND} />
            {ctx.render_scenes(&frame)}
        </svg>)
    }
}
```

- One scene per idea, 2-6 s each. Inside a scene `frame.seconds()` counts from the scene start.
- Animate with `frame.animate(&fframes::timeline!(at 0.2 => 0.8, animate 0.0_f32 => 1.0, Easing::EaseOut))`
  and springs, `Easing::Spring { mass: 1.0, stiffness: 180.0, damping: 20.0 }`. Leave the end
  time off a spring so it settles on its own.
- Markup without `{}` is cached across frames. Keep decoration literal and put animated values
  on a wrapping `<g transform={..} opacity={..}>`.
- `render_frame` runs for every frame on several threads: no panics, file reads or heavy work
  in it. Prepare data in the constructor or a `OnceLock`. Media lookups return `Option`; fall
  back to `Svgr::empty()` instead of `expect`.
- Put every font file in `media/` and refer to it by family name, with a numeric
  `font-weight`.
- Measure text rather than guess: `frame.text_width`, `frame.text_fit(.., TextOverflow::Ellipsis)`,
  `frame.text_break_lines` for paragraphs. `inspect` catches text leaving the canvas, not text
  leaving its own box, so check boxes in a `frame` PNG.
- GPU shaders (SkSL or Shadertoy GLSL) run on the Skia backend through `fframes::Shader`; see
  `references/api.md`.

## 5. Making it look good

The short version of `references/design.md`:

- One idea per scene, large type, margins of 8-10% of the width, two or three colors plus
  neutrals and one accent for emphasis.
- Elements enter with a spring or ease-out over 300-600 ms and leave faster, with ease-in over
  200-300 ms. Related items stagger by 60-120 ms. Give the viewer 1-2 s to read after the
  motion settles, and never move everything at once.
- Cross-fade scenes (`fn overlap(&self) -> Overlap { Overlap::Previous(0.4) }`) or carry an
  element across the cut. Keep a little motion during holds so they do not look frozen.
- At 1920x1080: titles 96-140 px, body 44-60 px, at most about 8 words per line and 3 lines per
  card. Portrait 1080x1920 is watched on a phone: same pixel sizes or larger, content inside
  the middle 80% because platform UI covers the top and bottom.
- Check every scene against the checklist in `references/design.md` with a strip.

## 6. Sound

Put audio files in `media/` (compiled in, mono) or load a folder at runtime with
`MediaDirectory` for stereo, then place them:

```rust
AudioMap::from([
    AudioTrack::new("music.mp3", Second(0.)..Eof).gain_db(-18.).fade_in(1.).fade_out(2.).duck_under_voice(),
    AudioTrack::new("vo.wav", Second(0.6)..Eof).voice(),
    AudioTrack::new("whoosh.wav", Second(3.1)..Eof).gain_db(-8.),
])
```

Time sound effects from the same constants that drive the animation. Check levels with
`$R audio analyze --waveform w.png` (about -14 LUFS integrated for web video, true peak below
-1 dBTP, no unintended silence) and `$R audio at 3.1s` for what plays at an event. Then let
the user listen in `$R preview`.

## 7. Finish

1. `$R inspect --fail-on warning` passes, or the remaining warnings are understood entrances.
2. A strip of every scene looks right and key frames are checked at full size.
3. `$R audio analyze` shows sensible levels.
4. `$R render -o out.mp4`, then confirm size, frame count and audio with
   `ffprobe -v error -show_entries stream=codec_type,width,height,nb_frames,duration out.mp4`.
5. Run `cargo test` if the project keeps snapshots, and commit `_frame_snapshots/*.png`.

Tell the user the output path, the duration, the path of a strip image and the `preview`
command to watch it.

## Troubleshooting

- Build fails in `ffmpeg-sys-fframes`: a system library from step 1 is missing (`nasm`,
  `pkg-config`, the codec `-dev` packages).
- Build fails in the Skia bindings (`skia-bindings`) with bindgen or libclang errors (Skia
  is only compiled from source when no prebuilt matches, e.g. `metal` and `vulkan` together): point `LIBCLANG_PATH` at a
  working libclang (on macOS Xcode's:
  `export LIBCLANG_PATH=$(xcode-select -p)/Toolchains/XcodeDefault.xctoolchain/usr/lib`).
- Skia bindings fail to compile on macOS 27 / Xcode 27 with ``cannot find type `_Traits` ``
  in `std___hash_table___node_allocator`: only happens when Skia is compiled from source (the
  prebuilt binaries ship their bindings). `skia-bindings` 0.153.3 lacks the bindgen rule
  (rust-skia [#1335](https://github.com/rust-skia/rust-skia/pull/1335)); use one GPU backend so
  the prebuilt download matches, or patch a copy of `skia-bindings-0.153.3` (add
  `"std::__hash_table.*",` after `"std::__tree.*",` in `OPAQUE_TYPES` in
  `build_support/skia_bindgen.rs`) through `[patch.crates-io]`.
- ``can't find crate for `fframes_media_dir_macro` `` on macOS 27 while the file exists: the
  proc macro was linked by an older Rust whose output the macOS 27 loader rejects (a "LINKEDIT
  string pool" error). Update Rust (`rustup update`; 1.98 works) and rebuild.
- Text renders in the wrong font or not at all: `inspect` shows "No match for ... font-family";
  add the font file to `media/` and use its exact family name.
