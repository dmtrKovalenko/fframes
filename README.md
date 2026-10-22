<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="landing/brand/fframes-wordmark-dark.svg" />
    <img alt="fframes" src="landing/brand/fframes-wordmark.svg" width="360" />
  </picture>
</p>

<p align="center">
  <b>Video vibe coding framework that is actually fast.</b><br />
  Write your video in Rust and SVG, render it on the GPU.
</p>

<p align="center">
  <a href="https://crates.io/crates/fframes"><img alt="crates.io" src="https://img.shields.io/crates/v/fframes.svg" /></a>
  <a href="https://docs.rs/fframes"><img alt="docs.rs" src="https://img.shields.io/docsrs/fframes" /></a>
  <a href="./LICENSE.txt"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-blue.svg" /></a>
  <a href="https://x.com/fframes_rust"><img alt="@fframes_rust on X" src="https://img.shields.io/badge/X-@fframes__rust-black.svg" /></a>
</p>

https://github.com/user-attachments/assets/9fdfd7b6-121b-4b49-bbf4-6f34d240f3a5

<p align="center">
  <sub>This 128 second video was <a href="https://x.com/neogoose_btw/status/2104432561200279774">vibed in 48 minutes and rendered in 36 seconds</a>.<br />
  It is an fframes project: <a href="examples/fframes-intro">examples/fframes-intro</a>.</sub>
</p>

---

## Ask your agent for a video

fframes ships a skill for coding agents. Add it once:

```sh
npx skills add https://fframes.studio
```

Then tell the agent what video you want. The skill walks it from an empty folder to a rendered `.mp4`: install
fframes, create a project, design the motion, place the sound, check the result.

The fframes API is explicit and verbose, which is [why it stayed unreleased for years](https://x.com/neogoose_btw/status/2104432563146444994).
An agent with a good skill file writes that kind of code without complaint.

## Your agent cannot watch the video

A coding agent cannot play a video or listen to a soundtrack, so every generated project comes with a command line
that turns the video into PNGs and text.

<table>
  <tr>
    <th width="50%">Your agent</th>
    <th width="50%">You</th>
  </tr>
  <tr>
    <td valign="top">
      <code>inspect</code> checks every frame without rendering pixels. It reports missing fonts or images, text cut off by the canvas, invalid SVG and panics, each with its time and scene.<br /><br />
      <code>strip</code> lays evenly spaced frames out on one contact sheet, <code>onion</code> blends frames to show the path and easing of a movement, <code>frame</code> writes full-size PNGs.<br /><br />
      <code>audio analyze</code> reports loudness (LUFS), true peak, clipping and silence per scene.
    </td>
    <td valign="top">
      <code>preview</code> opens a GPU window with sound to play, pause, seek and step frame by frame.<br /><br />
      The editor runs the video compiled to WebAssembly in the browser, with a timeline to scrub.<br /><br />
      <code>render</code> writes the final file. <code>render --draft</code> encodes one scene at half size in about a second.
    </td>
  </tr>
</table>

Every command runs as `cargo run --release -- <command>` inside the project:

| command | output |
| --- | --- |
| `timeline` | the scenes with their frame and second ranges, and every audio track with its mix settings |
| `inspect` | problems in a frame every 0.25 s and the first and last frame of every scene; exit code 2 on errors |
| `strip <scene> -n 12` | `strip.png`, a labelled contact sheet |
| `frame <scene>@end,<scene>@50%` | full-size PNGs in `frames/` |
| `onion "<scene>@0..<scene>@1s" -n 6` | `onion.png`, the blended movement |
| `audio analyze --waveform w.png` | the loudness report and a waveform with scene lines and cue ticks |
| `preview` | the real-time window, for a human |
| `render [--draft]` | the video, or a part of it |

`<scene>` is the name of a scene struct in your video. Times read as `120` (frame), `3.2s`, `50%`, `<scene>@1.2s`, and
ranges like `<scene>@0..<scene>@1s`. Add `--json` to parse the output.

## Why it is fast

- The Skia backend draws on the GPU, Metal on macOS and Vulkan on Linux and Windows, about 10x faster than the built-in CPU backend.
- Parts of a frame without `{expressions}` are hashed at compile time, so the renderers draw them once and reuse them.
- fframes links ffmpeg's libav libraries instead of piping frames to a separate process.
- When SVG is not enough, SkSL or pasted Shadertoy GLSL runs as a layer of any frame.

## What your agent writes

Every frame is an SVG tree returned by a Rust function, written with the `svgr!` macro. From
[examples/hello-world](examples/hello-world/src/hello_world.rs), shortened: a square moves along a timeline and a line
of text prints the current frame.

<details>
<summary><b>Show the code</b></summary>

```rust
impl Video for HelloWorldVideo<'_> {
    const FPS: usize = 30;
    const WIDTH: usize = 1920;
    const HEIGHT: usize = 1080;

    fn duration(&self) -> fframes::Duration<'_> {
        fframes::Duration::Seconds(30.)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame(&self, frame: Frame, ctx: &FFramesContext) -> fframes::Svgr<'_> {
        fframes::svgr!(
            <svg
                xmlns="http://www.w3.org/2000/svg"
                viewBox="0 0 1920 1080"
                width={ctx.current_video_size.width}
                height={ctx.current_video_size.height}
            >
                <rect
                    x="400"
                    y="400"
                    width="200"
                    height="200"
                    fill="blue"
                    transform={frame.animate(fframes::timeline!(
                        at 0., animate Transform::translate(0, 0) => Transform::translate(200, 480), Easing::Linear,
                        at 2. => 6.0, animate Transform::translate(200, 480) => Transform::translate(750, -400), Easing::Linear,
                        at 6.0 => 10.0, animate Transform::translate(750, -400) => Transform::translate(1310, 480), Easing::Linear,
                    ))}
                />

                <text
                    x="100"
                    y="440"
                    font-family="JetBrains Mono"
                    font-size="74"
                    font-weight="500"
                    fill="#3b5563"
                >
                    {format!("This frame index: {}, second: {:.2}", frame.index, frame.seconds())}
                </text>
            </svg>
        )
    }
}
```

</details>

The skill's guides are plain Markdown, so you can read them too:
[the workflow](skills/fframes-video/SKILL.md), [the API cheat sheet](skills/fframes-video/references/api.md),
[design](skills/fframes-video/references/design.md) and [sound](skills/fframes-video/references/audio.md).
The full API reference is on [docs.rs/fframes](https://docs.rs/fframes).

## Examples

Click a poster to watch the example. The README in each example folder has the video inline, and
`just render <example>` renders the full one.

<table>
  <tr>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/9fdfd7b6-121b-4b49-bbf4-6f34d240f3a5"><img alt="fframes-intro" src="examples/_previews/fframes-intro.jpg" width="100%" /></a><br />
      <b><a href="examples/fframes-intro">fframes-intro</a></b><br />
      <sub>the 128 second <a href="https://x.com/neogoose_btw/status/2104432561200279774">launch video</a>, everything placed on the beat grid of its soundtrack</sub>
    </td>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/d03e5fe7-4782-4a67-9571-e435ed367477"><img alt="signal-lab" src="examples/_previews/signal-lab.jpg" width="100%" /></a><br />
      <b><a href="examples/signal-lab">signal-lab</a></b><br />
      <sub>a 24 second motion study: product UI, data storytelling and a technical explainer</sub>
    </td>
  </tr>
  <tr>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/2a97f8d0-abb9-4b24-9149-cde560b1efcb"><img alt="shaders" src="examples/_previews/shaders.jpg" width="100%" /></a><br />
      <b><a href="examples/shaders">shaders</a></b><br />
      <sub>GPU shader layers composed with SVG: an SkSL background and a Shadertoy raymarcher clipped into a card</sub>
    </td>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/53bed61f-6478-471b-81c1-125a3054ee18"><img alt="shader-mode" src="examples/_previews/shader-mode.jpg" width="100%" /></a><br />
      <b><a href="examples/shader-mode">shader-mode</a></b><br />
      <sub>a 31-second shader promo with reference audio, a moving panel wall, native preview footage and 18 GPU shader programs, rendered with Skia on Metal or Vulkan.<br />The preview is silent: its soundtrack is not in the repository.</sub>
    </td>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/8f15791f-0dc4-4589-bec4-c63e4ec0f912"><img alt="neon-triangle" src="examples/_previews/neon-triangle.jpg" width="100%" /></a><br />
      <b><a href="examples/neon-triangle">neon-triangle</a></b><br />
      <sub>a minimal shader clip built to make banding and motion artifacts easy to spot</sub>
    </td>
  </tr>
  <tr>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/4731c533-c120-44d0-8022-e09600d0c52f"><img alt="hello-world" src="examples/_previews/hello-world.jpg" width="100%" /></a><br />
      <b><a href="examples/hello-world">hello-world</a></b><br />
      <sub>a simple "hello world" video</sub>
    </td>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/fee736cc-e6e4-4ad6-b2a8-ca2ad287ab53"><img alt="beta" src="examples/_previews/beta.jpg" width="100%" /></a><br />
      <b><a href="examples/beta">beta</a></b><br />
      <sub>a complicated multi-scene example (our beta announce video)</sub>
    </td>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/963e7f80-e13a-44a2-95de-63684e175006"><img alt="marketing" src="examples/_previews/marketing.jpg" width="100%" /></a><br />
      <b><a href="examples/marketing">marketing</a></b><br />
      <sub>our marketing video</sub>
    </td>
  </tr>
  <tr>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/2b6706b8-562d-41fe-97aa-cc8198980001"><img alt="audio-announce" src="examples/_previews/audio-announce.jpg" width="100%" /></a><br />
      <b><a href="examples/audio-announce">audio-announce</a></b><br />
      <sub>an automated workflow to create audio visualisation with automated subtitles</sub>
    </td>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/602f32c6-4c4c-4d07-b5e9-da5e9dfe80ad"><img alt="podcast" src="examples/_previews/podcast.jpg" width="100%" /></a><br />
      <b><a href="examples/podcast">podcast</a></b><br />
      <sub>an audio visualization for a podcast placeholder video</sub>
    </td>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/fb57ae04-f908-4787-9ef0-bd440d36b1a4"><img alt="teej-podcast" src="examples/_previews/teej-podcast.jpg" width="100%" /></a><br />
      <b><a href="examples/teej-podcast">teej-podcast</a></b><br />
      <sub>teej's podcast with video and chapters visualization</sub>
    </td>
  </tr>
  <tr>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/20dda3c9-8c44-4ea9-9663-860527d635be"><img alt="tiktok" src="examples/_previews/tiktok.jpg" width="56%" /></a><br />
      <b><a href="examples/tiktok">tiktok</a></b><br />
      <sub>a TikTok-like vertical video</sub>
    </td>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/066d349e-aad6-4d03-b76b-25ad8ccdf8eb"><img alt="conference-splash-screen" src="examples/_previews/conference-splash-screen.jpg" width="100%" /></a><br />
      <b><a href="examples/conference-splash-screen">conference-splash-screen</a></b><br />
      <sub>one splash screen per conference talk, rendered in batch</sub>
    </td>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/71f0b506-e37b-4e56-8064-b9bbc57cdfad"><img alt="low-poly-art" src="examples/_previews/low-poly-art.jpg" width="100%" /></a><br />
      <b><a href="examples/low-poly-art">low-poly-art</a></b><br />
      <sub>a renderer stress test: animal art with a lot of polygons</sub>
    </td>
  </tr>
  <tr>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/fdddfc5e-72f2-4e5f-8ebe-892b9025ee1f"><img alt="pixel-memory" src="examples/_previews/pixel-memory.jpg" width="100%" /></a><br />
      <b><a href="examples/pixel-memory">pixel-memory</a></b><br />
      <sub>my dog's memorial video generator (totally randomized)</sub>
    </td>
  </tr>
  <tr>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/3736d334-624b-42b5-870d-0afd39cfe3ff"><img alt="lottie" src="examples/_previews/lottie.jpg" width="56%" /></a><br />
      <b><a href="examples/lottie">lottie</a></b><br />
      <sub>an 18 second vertical launch video for Lottie support: every animation is a Lottie file and the music is synthesized in Rust</sub>
    </td>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/3c51ccae-3541-4c79-817c-f544cf83e415"><img alt="made-of-motion" src="examples/_previews/made-of-motion.jpg" width="100%" /></a><br />
      <b><a href="examples/made-of-motion">made-of-motion</a></b><br />
      <sub>a 20 second promo with pixel objects, a perspective carousel, thermal shaders and per-exposure vector ink, an adaptation of a reference film</sub>
    </td>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/f752ebc6-6d7c-4304-abb6-27becc64801c"><img alt="arm" src="examples/_previews/arm.jpg" width="100%" /></a><br />
      <b><a href="examples/arm">arm</a></b><br />
      <sub>a 37 second 3D short: a lab robot with a camera eye spells fframes, drops a letter and picks it up again, rendered with fframes-3d on the GPU</sub>
    </td>
    <td width="33%" align="center" valign="top">
      <a href="https://github.com/user-attachments/assets/09432a37-4bd9-42ac-b068-ce92f59695af"><img alt="svg-in-3d" src="examples/_previews/svg-in-3d.jpg" width="100%" /></a><br />
      <b><a href="examples/svg-in-3d">svg-in-3d</a></b><br />
      <sub>an svgr UI on the screen of a turning 3D monitor, showing a 3D cube with the svgr logo: four nested levels of SVG and 3D that never leave the GPU</sub>
    </td>
  </tr>
</table>

From the repository root, `just run podcast` opens an example in the editor with live reload and `just render podcast`
writes it to a file.

## Without an agent

```sh
cargo install --locked cargo-fframes
cargo fframes new my-video
cd my-video && cargo run --release -- preview
```

`cargo fframes new` takes `--template single-scene|multi-scene`, `--format landscape|portrait|square|uhd`, `--fps`,
`--title` and `--backend`. On macOS and Linux (arm64, x86_64) the first build downloads prebuilt Skia and ffmpeg
libraries and takes under a minute on a fast machine; other targets and feature combinations
compile them from source (up to ~20 minutes). Later builds take seconds. `--backend cpu` skips Skia (no preview window, slower renders). To track `main`, install with
`--git https://github.com/dmtrKovalenko/fframes`.

## Requirements

[Rust](https://www.rust-lang.org/learn/get-started) and, for working on the editor, [NodeJS](https://nodejs.org/en/download/).
fframes links ffmpeg's libav libraries statically. A prebuilt build is downloaded for macOS and Linux (arm64 and x86_64) and
compiled from source for other targets or `FFMPEG_FORCE_BUILD=1`; either way the system encoders they link against have to be installed.

The examples enable `fframes/build-portable` on native non-Windows targets. CI builds the
whole workspace, combining the examples' x264, x265, VPX and Opus features; no published
FFmpeg archive currently matches that combination, so it falls back to a source build.
Without `build-portable`, that build uses `-march=native -mtune=native`. Caching it and
restoring it on a runner with a different CPU can cause `SIGILL` (illegal instruction).
The feature omits those flags for source builds while keeping matching prebuilt downloads
enabled, and also applies to FFmpeg used by the compile-time media macro.

Within this workspace, use the same setting for native builds that will be cached or run on
other machines:

```toml
[target.'cfg(not(any(target_arch = "wasm32", windows)))'.dependencies]
fframes = { workspace = true, features = ["build-portable"] }
```

`build-portable` also enables FFmpeg's source-build support, so the examples exclude Windows
(which links a shared FFmpeg installation) and browser targets. Setting `FFMPEG_MARCH` or
`FFMPEG_MTUNE`, even to an empty string, bypasses prebuilt downloads; prefer the feature when
you want portable source fallbacks and prebuilt downloads.

<details>
<summary><b>macOS</b></summary>

```sh
brew install pkg-config ffmpeg x264 x265 opus nasm ninja
```

</details>

<details>
<summary><b>Linux</b></summary>

for debian based distros:
```sh
sudo apt-get install -y yasm nasm ffmpeg libx264-dev libx265-dev libopus-dev libclang-dev clang ninja-build libvpx-dev libasound2-dev
```

for arch based distros:
```sh
sudo pacman -S ninja yasm nasm ffmpeg x264 x265 opus clang
```

for nix users:
```sh
nix-shell
```

</details>

<details>
<summary><b>Windows</b></summary>

On Windows ffmpeg is not compiled from source. Instead fframes links a prebuilt **FFmpeg 9.0** shared build
(for example `ffmpeg-n9.0-latest-win64-gpl-shared-9.0.zip` from [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds/releases/tag/latest))
and needs LLVM for bindgen:

```powershell
winget install LLVM.LLVM
# unzip the ffmpeg build somewhere, then point the build to it:
$env:FFMPEG_DIR = "C:\ffmpeg-n9.0-latest-win64-gpl-shared-9.0"
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
# the ffmpeg DLLs must be reachable when building (proc macros load them) and running
$env:PATH = "$env:FFMPEG_DIR\bin;$env:PATH"
```

`vcpkg install ffmpeg` works as well instead of `FFMPEG_DIR`. Codecs come from the prebuilt build, so leave the
codec features (`h264`, `h265`, ...) off: they request a from-source ffmpeg build which is not supported on Windows.

</details>

<details>
<summary><b>Codecs</b></summary>

Cargo features of the `fframes` crate choose which codecs and hardware acceleration libraries are linked:

```toml
[dependencies]
# this will enable and try to link libx264 during the build
fframes = { version = "1", features = ["h264", "libav-agree-gpl"] }
```

All the build and linking of codecs and other system libs are leveraging the ffmpeg build system, so for
troubleshooting please refer the [ffmpeg compilation guide](https://trac.ffmpeg.org/wiki/CompilationGuide).

</details>

<details>
<summary><b>Troubleshooting</b></summary>

- **Build fails in `ffmpeg-sys-fframes`:** a system library from the list above is missing (`nasm`, `pkg-config`, the codec packages).
- **Build fails in the Skia bindings with a bindgen or libclang error** (only when Skia is compiled from source, e.g. with
  both `metal` and `vulkan` enabled): point `LIBCLANG_PATH` at a working libclang,
  on macOS Xcode's: `export LIBCLANG_PATH=$(xcode-select -p)/Toolchains/XcodeDefault.xctoolchain/usr/lib`.
- **Text renders in the wrong font:** `inspect` reports `No match for ... font-family`; put the font file in the
  project's `media/` folder and use its exact family name.

</details>

## Working on fframes

Agents working on fframes itself should start from [AGENTS.md](AGENTS.md). For humans, install the
[just](https://github.com/casey/just) command runner and init the repo:

```bash
npm install --global pnpm # the package manager for the nodejs based editor
cargo install --locked just cargo-watch wasm-bindgen-cli wasm-pack
just init-repo
just watch-editor # in a separate terminal, to work on the editor
```

## License

fframes is released under the [MIT License](./LICENSE.txt).
