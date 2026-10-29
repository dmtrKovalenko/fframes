# arm

A 37 second 3D short: a lab robot with a camera eye spells fframes, drops a letter and picks it up again, rendered with fframes-3d on the GPU.

https://github.com/user-attachments/assets/f752ebc6-6d7c-4304-abb6-27becc64801c

The soundtrack is synthesized by a second binary, so run it once before the video:

```sh
cargo run --release -p arm-example --bin arm-soundtrack             # writes dynamic_media/*.wav
cargo run --release -p arm-example --bin arm -- render              # 3D and Skia on the GPU -> out.mp4
cargo run --release -p arm-example --bin arm -- --cpu render        # everything on the CPU
```
