## Requirements

[Rust](https://www.rust-lang.org/learn/get-started) and [NodeJS](https://nodejs.org/en/download/) (for local development) toolchains.

The only global dependency we have is libav software package that is available to be installed

```sh
sudo apt-get install libavformat-dev libavcodec-dev libavutil-dev libavfilter-dev libswscale-dev libavdevice-dev
```

But the easiest to get them is to have ffmpeg v4 installed, yes unfortunately the latest v5 is not supported yet. Install the 4.4.2 version from your favorite package manager or built it from source.

```bash
  git clone https://git.ffmpeg.org/ffmpeg.git ffmpeg
  cd ffmpeg
  git remote update
  git fetch --tags
  git checkout n{{4.4.2}}

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

## Usage

For the beta usage we provide a couple of examples you can use as a reference:

- hello-world - a simple hello world video example
- podcast – an audio visualization for a podcast placeholder video
- marketing – our marketing video example
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
