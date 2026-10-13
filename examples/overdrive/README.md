# OVERDRIVE

A 13 second, 60 fps 3D film. A sports car races down a wet neon road and jumps. In the air it
flips forward and transforms into a robot. The robot lands on the drop, powers up and fires an
arm cannon. Then it dives back into car form and passes the camera at full speed.

All the 3D is a [`fframes-3d`](../../fframes-3d) scene. Its software rasterizer renders each
frame, so the CPU and the Skia GPU backends give the same pixels. The titles and the HUD are
SVG on top. The scene camera pins them to parts of the robot.

| file | what |
| --- | --- |
| `src/robot.rs` | the skeleton (16 bones, dark frame parts, glowing seams) and the 22 car body panels that become its armor. Each panel has a place on the car, a place on a bone, a delay, an arc and a spin |
| `src/story.rs` | the choreography on a 120 BPM grid: the car, the ballistic flip, the landing, the poses, the cut list and the event list |
| `src/stage.rs` | the road, neon pylons with wet reflections, the skyline, the lights and the environment |
| `src/hud.rs` | letterbox, flashes, readouts, callouts and the title |
| `src/bin/soundtrack.rs` | synthesizes the music bed and every sound effect into `dynamic_media/` |

`Video::audio` places the sound effects from the same event list that drives the animation.
Each armor panel that locks in is a clank. Thus the sound stays in sync when the timing changes.

```sh
cargo run --release -p overdrive-example --bin overdrive-soundtrack  # writes dynamic_media/*.wav
cargo run --release -p overdrive-example --bin overdrive -- render    # Skia on the GPU -> out.mp4
cargo run --release -p overdrive-example --bin overdrive -- --cpu render
cargo run --release -p overdrive-example --bin overdrive -- strip -n 20
cargo test --release -p overdrive-example --test look -- --ignored     # car/robot poses as PNGs
```

A full render takes about two minutes on a 28 thread machine. Fast shots render three sub
frames for motion blur (`Renderer::render_motion_blur`).
