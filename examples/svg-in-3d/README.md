# svg-in-3d

An svgr UI on the screen of a turning 3D monitor, showing a 3D cube with the svgr logo: four nested levels of SVG and 3D that never leave the GPU.

https://github.com/user-attachments/assets/09432a37-4bd9-42ac-b068-ce92f59695af

```sh
cargo run --release -p svg-in-3d-example -- render          # 3D and Skia on the GPU -> out.mp4
cargo run --release -p svg-in-3d-example -- --cpu render    # everything on the CPU
```
