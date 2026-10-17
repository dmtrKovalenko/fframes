# FFRAMES, ASSEMBLED

A 37 second short film about a lab robot with a camera eye. The robot wakes up in the dark
lab, and its eye light searches the floor. The lab lights flicker on. The robot sees the
camera, then spells `fframes` with letter pieces from a rack. It gets faster and too
confident. In a spin it throws the "a" across the floor, and sparks fly. The music stops like
a tape, and crickets fill the silence. The robot looks at the letter, then into the camera. It
sighs, moves over carefully and picks up the letter from the floor. It puts the letter in its
place, pats it, completes the word and bows.

| file | what |
| --- | --- |
| `src/arm.rs` | the six axis arm and its analytic inverse kinematics for any tool pose. The arm has finned motors, bolt circles, a gas spring, a corrugated hose, a drag chain, three wrist motors, a forearm that rolls, a gripper and a camera eye with a lid, a spot light and a beam |
| `src/lab.rs` | the polished concrete floor (procedural color, normal and occlusion-roughness maps, reflects the scene), hazard stripes, stencils, a printed grid, the rack, shelves of bins, monitors, the walls and the lights |
| `src/effects.rs` | what the film builds itself from the building blocks: camera shake, a light beam in hazy air, sparks with streaks, a printed grid |
| `src/story.rs` | the performance: keys with character easing (anticipation, overshoot), idle motion, the timelines of the letters, the rigid tumble of the dropped letter, the sounds and the console lines |
| `src/bin/soundtrack.rs` | the score (music box, marimba, pizzicato, glockenspiel, drums, pads, crickets, tape stop) and every effect, timed from the story. The servo sound follows the speed of each joint |

```sh
cargo run --release -p arm-example --bin arm-soundtrack             # writes dynamic_media/*.wav
cargo run --release -p arm-example --bin arm -- render              # 3D and Skia on the GPU -> out.mp4
cargo run --release -p arm-example --bin arm -- --cpu render        # everything on the CPU
cargo run --release -p arm-example --bin arm -- --readback render   # the 3D read back to the CPU, to compare
```

## What it uses

| `fframes::three_d` | where |
| --- | --- |
| scene graph, groups, `Scene::local_to_world` | the joints of the arm, the gas spring between two joints |
| `rounded_box`, `cylinder`, `torus`, `sphere_partial`, `lathe`, `extrude`, `tube`, `ring`, `circle`, `text`, `merge` | the arm, the letters, the stencils |
| `Material::standard`, `metal`, `clearcoat`, `emissive`, `glow` | white paint, chrome, lacquered letters, status lights, the beam |
| `map`, `normal_map`, `ao_map`, `metal_rough_map`, `emissive_map`, `Texture::from_fn` | the concrete, the hazard stripes, the grid, the monitors |
| `Light::sun` with `Shadow`, `directional`, `hemisphere`, `spot`, `point` | the ceiling, the fill, the eye light, the glow of the base ring |
| `Environment::Studio`, `Fog::Exp2`, `Background::Color` | the lab at each light level |
| `Reflector::floor` | the polished concrete |
| `Node::instanced` with a color per copy | the bins on the shelves |
| `Node::particles` | the dust in the beam, the sparks |
| `Camera::focus_on`, `Renderer::render_motion_blur` | depth of field in every shot, motion blur in the spin and the throw |
| `Bloom`, `ToneMapping::Neutral`, `exposure` | the glow of the lights and the sparks |
| `Camera::project`, `Camera::pixel_size` | the SVG marker on the thrown letter |

The 3D comes from `fframes = { features = ["3d", "wgpu"] }` and renders on the GPU
(`Renderer::gpu()`). On Linux and Windows it uses the same Vulkan device as Skia (the `wgpu`
feature of `fframes_skia_renderer`): the 3D of each frame is a GPU texture, Skia draws the
console overlay on it and converts it to YUV for the encoder, and nothing is read back between
these steps. On macOS Skia renders with Metal, and the 3D comes back as pixels.

The polished concrete reflects the scene (`Scene::reflector`). The reflection renders mirrored
at half resolution, and the roughness map of the floor blurs it. Specular antialiasing stops
the sparkle on the chrome.

The full film renders in about 42 seconds on an Intel UHD 770 iGPU: about 19 ms for each 1080p
frame with depth of field, reflections and 4x MSAA. On average the 3D is within 0.4/255 of the
CPU rasterizer. `--readback` reads the 3D back from the GPU and uploads it to Skia again, for
comparison. With `--cpu` the film takes about six minutes on a 28 thread machine.
