## Requirements

[Rust](https://www.rust-lang.org/learn/get-started) and [NodeJS](https://nodejs.org/en/download/) (for local development) toolchains.

FFrames will install and compile ffmpeg during the development as we rely on the ffmpeg libav libraries, so there are dependencies on the system encoders required to build libav libraries. Here's what you'll need:

#### Linux

for debian based distros:
```sh
sudo apt-get install -y yasm nasm ffmpeg libx264-dev libx265-dev libopus-dev libclang-dev
```

for arch based distros:
```sh
sudo pacman -S ninja yasm nasm ffmpeg x264 x265 opus clang
```

#### MacOS:
```sh
brew install pkg-config ffmpeg x264 x265 opus nasm
```

## Beta testing

Once everything is installed please install the just command runner and init the repo.

```bash
  npm install --global yarn # the package manager for nodejs based editor
  cargo install --locked just cargo-watch wasm-bindgen-cli wasm-pack
  just init-repo
```
During the build fframes will automatically download and compile required libraries. You can control which libraries will be tried to link (usually codecs or hw acceslleration librareies) by using cargo features of the `fframes_renderer`(for encoding) and `fframes`(for decoding) crates.

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
