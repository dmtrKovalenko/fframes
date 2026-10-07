# Made of motion

A 489-frame, 1440×1080 fframes promo with pixel objects, a continuous perspective
carousel, thermal performances, paper texture, and per-exposure vector ink. Metal
on macOS; Vulkan elsewhere. This GPU example has no web-editor bridge.

The film adapts the [reference film](https://x.com/jordanarchivess/status/2107536262894915762)
to fframes: “how do you turn a few lines of code into a feeling?”, three cards
(**code. / motion. / feeling.**), and seven flying letters assembling into the
fframes wordmark. Original cut boundaries and audio are retained.

## Final assets

This example contains the final data used by the approved film. No Python,
reconstruction scripts, analysis inputs, or original reference download are needed
to build, preview, or export it. `assets.json` records the source provenance,
tracing settings, and SHA-256 hashes of the final assets.

| Location | Contents |
| --- | --- |
| `src/vector_ink/*.paths.gz` | Five compressed archives of text SVG paths, covering all measured ink exposures |
| `src/*_capture.rs` | Final source-frame measurements for ink, opening effects, hand placement, and camera poses |
| `src/opening-type.path` | Authored title geometry snapped to a pixel grid for the opening's one-frame type pulse |
| `media/` | Three embedded fonts, the original pixel-object atlas, and the hand silhouette/heat-field atlas |
| `dynamic_media/portrait-clean.mp4` | Final cleaned portrait footage |
| `dynamic_media/soundtrack.wav` | Stereo float-PCM soundtrack for native playback |
| `dynamic_media/soundtrack.m4a` | Original AAC packets for final export |

Keep all three runtime media files next to the example when moving the project.
They are included in the source asset set; missing media is an error, not a request
to regenerate it. The final export belongs in `output/made-of-motion.mp4`;
`output/` is excluded from source control.

## Native preview

Run these commands from the repository root:

```sh
cargo run --release -p made-of-motion -- preview 0s
cargo run --release -p made-of-motion -- preview 0.7s  # handwritten pen and star
cargo run --release -p made-of-motion -- preview 3.5s  # orange ball and portrait
cargo run --release -p made-of-motion -- preview 6.5s  # ring and three handoffs
cargo run --release -p made-of-motion -- preview 13.6s # hand and white strokes
cargo run --release -p made-of-motion -- preview 19s  # red ink and wordmark
```

Space pauses; h/l seek; j/k step one frame. Use the native preview for revisions
before requesting a full export.

`--ink-only` displays the vector exposures at their original source positions on
neutral paper, without objects, footage, grain or branding adaptations. It also
works with frame, strip, onion, SVG and inspect commands.

```sh
cargo run --release -p made-of-motion -- --ink-only preview 0s
cargo run --release -p made-of-motion -- inspect --all-frames
cargo run --release -p made-of-motion -- --ink-only frame 21,51,90,170,334,416
cargo run --release -p made-of-motion -- onion 158..199 -n 10 -o output/onion-orbit.png
cargo run --release -p made-of-motion -- --ink-only onion 414..430 -n 12 -o output/onion-ink.png
```

## What is vector, shader, or footage?

| Layer | Implementation |
| --- | --- |
| Handwritten opening, brown/red/white pen strokes, black dots and whips, broad gray trails | SVG paths traced independently for each measured exposure |
| Portrait marks, hand marks, red feature-card marks and inter-card speed lines | Per-exposure SVG contours; source caption/icon regions excluded or reconstructed |
| Red marks around the ending letters | Four measured SVG nib tracks translated onto seven brand letters |
| Portrait, 3.88–6.38s | Cleaned source footage with SVG ink added back and shader caption cleanup |
| Hand, 13.83–15.92s | Source-derived silhouette and smooth heat field; shader recreates palette, background and light |
| Electric star and contracting orange ball | SkSL geometry with measured centre/radius/rotation/cusp parameters |
| Paper, grain, four-frame thermal cut and collision light | Procedural shaders |
| Icons, ring, hero motion, typography | Original pixel atlas and Rust animation; selected motion measurements inform poses |

No reference ink pixels are sampled at runtime. `VectorInk` decompresses the text
SVG archives and builds the scene trees once, then samples by global frame index.
Contours retain pressure and soft trails with 64 opacity levels, even-odd fills,
0.4px simplification tolerance, and 0.1px coordinate precision.

This is vector rotoscoping, not recovery of the creator's original splines or a
fluid simulation. Source compression, pigment/opacity estimation and reconstructed
occlusions prevent a claim of pixel-identical recovery. The brand ending preserves
the larger final red gestures through frame 488, following assembly and the fast
camera pullback. Only the independent black gestures fade. Ink-only mode keeps
the four original tracks at their source positions.

The featured icons occupy permanent slots in one continuously rotating wheel.
Flights cover frames 190–205, 227–242 and 261–270; returns cover 214–225, 249–259
and 280–294. During feature cards the other icons are hidden while their tracks
continue rotating. Returning icons dock in their moving vacant slots.

`objects.rs` prepares the trajectories with cached `timeline!` animations;
`hero.rs` uses those same poses. The motion diamond eases between measured camera
poses. `flight.rs` retains the seven letter flights, three-frame assembly and
four-frame camera pullback.

`opening.rs` retains the oversized type through frame 13, with full-width dashed
x-height/baseline rulers, lateral camera moves and a cursor wipe. Two one-frame
color flashes precede it. A pixel pulse and four broken-line exposures collapse
into a measured selection around the adapted copy; that selection blinks on
frames 18–20 and 22. The large type intentionally crosses the canvas edge during
the camera moves, so inspection reports expected clipping warnings there.

## Explicit final export

The source has 489 frames at 24000/1001 fps. Native preview uses 24 fps, about 20ms
shorter over the whole film, with decoded stereo audio and no gain changes or
limiting. For delivery, the following FFmpeg step restores the rational cadence
and copies the original AAC packets unchanged. It replaces the final output file.

Run only when a full export is requested:

```sh
cargo run --release -p made-of-motion -- render \
  -o examples/made-of-motion/output/made-of-motion.24fps.mp4

ffmpeg -hide_banner -loglevel error -y \
  -i examples/made-of-motion/output/made-of-motion.24fps.mp4 \
  -i examples/made-of-motion/dynamic_media/soundtrack.m4a \
  -map 0:v:0 -map 1:a:0 \
  -vf 'settb=1/24000,setpts=N*1001' -r 24000/1001 -fps_mode cfr \
  -c:v libx264 -crf 16 -preset slow -pix_fmt yuv420p \
  -c:a copy -video_track_timescale 24000 -movflags +faststart \
  examples/made-of-motion/output/made-of-motion.mp4
```

Verify the delivered video:

```sh
ffprobe -v error -select_streams v:0 \
  -show_entries stream=width,height,r_frame_rate,nb_frames \
  examples/made-of-motion/output/made-of-motion.mp4

ffmpeg -v error -i examples/made-of-motion/output/made-of-motion.mp4 \
  -map 0:a:0 -c:a copy -f adts - | shasum -a 256
```

Expect 1440×1080, 489 frames, `24000/1001` fps, and the `audio_adts_sha256` value
from `assets.json`. No tests are added; use inspection, frames, onions, formatting
and the repository's Clippy rules when changing the example.
