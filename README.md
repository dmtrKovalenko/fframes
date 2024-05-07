## Requirements

[Rust](https://www.rust-lang.org/learn/get-started) and [NodeJS](https://nodejs.org/en/download/) (for local development) toolchains.

The only global dependency we have is libav software package that is available to be installed on Ubuntu:

```sh
sudo apt-get install libavformat-dev libavcodec-dev libavutil-dev libavfilter-dev libswscale-dev libavdevice-dev
```

But the easiest to get them is to have ffmpeg v5 installed. On MacOs:

```sh
brew install ffmpeg
```

Or clone and compile ffmpeg from source, [here is the complete guide](https://trac.ffmpeg.org/wiki/CompilationGuide) and this is a minimal example:

```bash
  git clone https://git.ffmpeg.org/ffmpeg.git ffmpeg
  cd ffmpeg
  git remote update
  git fetch --tags
  git checkout n6.0.0

  # this is a minimum set of options to build ffmpeg for fframes, you will likely need more options
  ./configure --enable-shared --enable-libx264 --enable-libx265 --enable-gpl
  make
  make install
```

## Beta testing

Once everything is installed please install the just command runner and init the repo.

```bash
  npm install --global yarn # the package manager for nodejs based editor
  cargo install just
  just init-repo
```

## Linking libav aka ffmpeg

If during the build you are getting an error that `libavformat`.h` or some other C header from libav is missing it means that either:

- You don't have ffmpeg installed 
- Your linker is not able to find the ffmpeg headers

### Troubleshooting

Run the following command and if you are seeing errors it means that you might need to populate the LDFLAGS and CFLAGS with the path to the ffmpeg lib and include folders.

```bash
pkg-config --libs libavutil libavcodec libavformat libswscale libswresample
```

### Homebrew and MacOS

On MacOS if you installed ffmpeg via homebrew you might need to provide the path to the headers manually. Usually homebrew prints how to do this after the installation.

```bash
## Ideally refer to the comment after ffmpeg installation 
export LDFLAGS="-L/opt/homebrew/opt/ffmpeg@6/lib"
export CPPFLAGS="-I/opt/homebrew/opt/ffmpeg@6/include"
```

### Manual linking

You can also manually link ffmpeg during the build using `build.rs` file and populate the linker using rust build system.

Add this to the `build.rs` file:

```rust
fn main() {
    cc::Build::new()
        .file("ffmpeg_helper.c")
        .include("/opt/homebrew/opt/ffmpeg@5/include")
        .compile("ffmpeg_helper");
}
```

### Manual compilation

Usually, you can always install precompiled ffmpeg binaries for any platform but for some reason, you can always compile sources and link them manually. Here is a minimal build required for fframes. 

```bash
  git clone https://git.ffmpeg.org/ffmpeg.git ffmpeg
  cd ffmpeg
  git remote update
  git fetch --tags
  git checkout n6.0.0

  # this is a minimum set of options to build ffmpeg for fframes, you will likely need more options
  ./configure --enable-shared --enable-libx264 --enable-libx265 --enable-gpl
  make
  make install

```

More information about compiling from sources https://trac.ffmpeg.org/wiki/CompilationGuide

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

## Contributing

To change something in the editor please run this command in the separate terminal:

```bash
just watch-editor
```

## License

Please make sure that this project is under GPLv3 license while in beta. So you are not permitted to modify and redistribute it.
This will likely be changed once the project will be released.
