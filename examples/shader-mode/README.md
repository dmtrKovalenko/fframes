# Shader mode

A 932-frame, 1920×1080 Skia GPU example with 18 shaders, a 3D panel wall,
native preview footage and an animated fframes wordmark. Preview and rendering
run at 30 fps, using Metal on macOS and Vulkan elsewhere.

## Run

Place the soundtrack at `dynamic_media/reference-audio.wav`. To show the recorded
scrubbing, also provide `dynamic_media/native-scrubbing.mp4` (1920×1124 at 30 fps,
at least 71 frames). When the recording is absent, the bundled poster is used.
These runtime media files are kept outside Git.

From the repository root:

```sh
cargo run --release -p shader-mode -- preview
cargo run --release -p shader-mode -- preview 8.5s
```

The second command starts near the panel-wall animation. To export a video:

```sh
cargo run --release -p shader-mode -- render -o examples/shader-mode/output/shader-mode.mp4
```

`src/edit.rs` defines the scenes. `media/` contains the fonts, shader textures and
fallback poster used by the renderer. No Python scripts are needed to build or run.

## Upstream license

Parts of the shaders are adapted from
[Shader Effects Inc.](https://github.com/shader-effects-inc/shaders/tree/935f71a7789f0e07811dfe6fd0d8f707e9848238)
under the following license:

MIT License

Copyright (c) 2026 Shader Effects Inc.

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
