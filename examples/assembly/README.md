# UNIT 01

An 11 second, 60 fps minimal film. In a white void, a robot assembles itself from 13 parts.
Each part glides in from outside the frame, hovers for a moment and snaps into place on the
beat (100 BPM). The camera does one slow orbit. The materials are matte ceramic, satin graphite
and chrome. The only color is an orange line across the visor, which lights up at the end.

The robot is a `fframes-3d` scene with a transparent background and a floor that catches the
shadows. The white backdrop and the type are SVG. `Video::audio` places the sounds at the snap
times.

```sh
cargo run --release -p assembly-example --bin assembly-soundtrack   # writes dynamic_media/*.wav
cargo run --release -p assembly-example --bin assembly -- render     # Skia on the GPU -> out.mp4
cargo run --release -p assembly-example --bin assembly -- --cpu render
```

The film renders in about 20 seconds on a 28 thread machine.
