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

## Get started

With your coding agent, add the fframes skill and ask for a video:

```sh
npx skills add dmtrKovalenko/fframes
```

With cargo:

```sh
cargo install --locked cargo-fframes --git https://github.com/dmtrKovalenko/fframes
cargo fframes new my-video
```

The API reference is on [docs.rs/fframes](https://docs.rs/fframes).

## Requirements

[Rust](https://www.rust-lang.org/learn/get-started) and [NodeJS](https://nodejs.org/en/download/) (for local development) toolchains.

FFrames will install and compile ffmpeg during the development as we rely on the ffmpeg libav libraries, so there are dependencies on the system encoders required to build libav libraries. Here's what you'll need:

#### Linux

for debian based distros:
```sh
sudo apt-get install -y yasm nasm ffmpeg libx264-dev libx265-dev libopus-dev libclang-dev clang ninja libvpx-dev libasound2-dev
```

for arch based distros:
```sh
sudo pacman -S ninja yasm nasm ffmpeg x264 x265 opus clang
```
for nix users:
```sh
nix-shell
```

#### MacOS:
```sh
brew install pkg-config ffmpeg x264 x265 opus nasm ninja
```

#### Windows:

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

## Beta testing

Once everything is installed please install the just command runner and init the repo.

```bash
  npm install --global yarn # the package manager for nodejs based editor
  cargo install --locked just cargo-watch wasm-bindgen-cli wasm-pack
  just init-repo
```
During the build fframes will automatically download and compile required libraries. You can control which libraries will be tried to link (usually codecs or hw acceleration libraries) by using cargo features of the `fframes_renderer`(for encoding) and `fframes`(for decoding) crates.

```toml
[dependencies]
# this will enable and try to link libx264 during the build
fframes_renderer = { version = "0.1.0", features = ["h264", "libav-agree-gpl"] }
```

All the build and linking of codecs and other system libs are leveraging the ffmpeg build system, so for troubleshooting please refer the [ffmpeg compilation guide](https://trac.ffmpeg.org/wiki/CompilationGuide).

## Usage

For the beta testing we provide a couple of examples you can use as a reference:

- audio-announce - an automated workflow to create audio visualisation with automated subtitles
- beta - a complicated example of the multi-scene (our beta announce video)
- hello-world - a simple "hello world" video example
- low-poly-art - stress-test of the renderer performance. Animal art with a lot of polygons
- marketing – our marketing video example
- podcast – an audio visualization for a podcast placeholder video
- tiktok – displaying tiktok like vertical video
- teej-podcast - teej's podcast with video and chapters visualization
- pixel-memory - my dog's memorial video generator (totally randomized)

To display the video editor for example you can use the following command:

```bash
just run {{example}} # just run podcast
```

In order to render the example to file run

```bash
just render {{example}} # just render podcast
```

In order to create your custom video just copy an example. It is not recommended though to use this framework in the production, as it may panic. The project is still under hard development.

Please provide any of your feedback and ideas as issues, and feel free to contribute, but ideally, start from the issue.

## Installation

All the libraries are already published to the crates io including the most recent nightly version. So you can already install and use them independently.

## Contributing

To change something in the editor please run this command in the separate terminal:

```bash
just watch-editor
```

## License

Please make sure that this project is under GPLv3 license while in beta. So you are not permitted to modify and redistribute it.
This will likely be changed once the project will be released.
