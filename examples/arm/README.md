# FFRAMES, ASSEMBLED

A 36 second short film about a lab robot with a camera eye. The lab lights flicker on and the
robot wakes up. It sees the camera, then spells `fframes` with letter pieces from a rack. It
gets faster and too confident. In a spin it throws the "a" across the floor. The music stops
like a tape, and crickets fill the silence. The robot looks at the letter, then into the
camera. It sighs, moves over carefully and picks up the letter from the floor. It puts the
letter in its place, pats it, completes the word and bows.

| file | what |
| --- | --- |
| `src/arm.rs` | the six axis arm and its analytic inverse kinematics for any tool pose. The arm has finned motors, bolt circles, a gas spring, a corrugated hose, a drag chain, three wrist motors, a forearm that rolls, a gripper and a camera eye with a lid |
| `src/lab.rs` | the polished concrete floor (procedural, mipmapped, with a roughness map, reflects the scene), hazard stripes, stencils, the rack, the walls and the lights |
| `src/story.rs` | the performance: keys with character easing (anticipation, overshoot), idle motion, the timelines of the letters, the rigid tumble of the dropped letter, the sounds and the console lines |
| `src/bin/soundtrack.rs` | the score (music box, marimba, pizzicato, glockenspiel, drums, pads, crickets, tape stop) and every effect, timed from the story. The servo sound follows the speed of each joint |

```sh
cargo run --release -p arm-example --bin arm-soundtrack             # writes dynamic_media/*.wav
cargo run --release -p arm-example --bin arm -- render              # 3D and Skia on the GPU -> out.mp4
cargo run --release -p arm-example --bin arm -- --cpu render        # everything on the CPU
cargo run --release -p arm-example --bin arm -- --readback render   # the 3D read back to the CPU, to compare
cargo test --release -p arm-example                                 # kinematics, the letter lands in reach, GPU frames match the CPU, the shared texture matches its readback
```

The 3D renders on the GPU with the `gpu` feature of `fframes-3d` (`Renderer::gpu()`). It uses
the same Vulkan device as Skia (the `wgpu` feature of `fframes_skia_renderer`). The 3D of each
frame is a GPU texture. Skia draws the console overlay on it and converts it to YUV for the
encoder. Nothing is read back between these steps.

The polished concrete reflects the scene (`Scene::reflector`). The reflection renders mirrored
at half resolution, and the roughness map of the floor blurs it. Specular antialiasing stops
the sparkle on the chrome.

The full film renders in about 42 seconds on an Intel UHD 770 iGPU: about 19 ms for each 1080p
frame with depth of field, reflections and 4x MSAA. On average the 3D is within 0.4/255 of the
CPU rasterizer. `--readback` reads the 3D back from the GPU and uploads it to Skia again, for
comparison. With `--cpu` the film takes about six minutes on a 28 thread machine.
