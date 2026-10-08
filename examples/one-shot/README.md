# One shot

A 27-second, 1440×1080 promo at 24 fps in the visual language of
[made-of-motion](../made-of-motion): warm paper, oversized type with rulers, thermal
footage on black, and a nervous pen that underlines and circles words. Nothing is
reused from that film: the footage, ink, objects, music and edit are new.

> how do you make AI produce stunning results in one shot?
> you can't. · humans spot repeating patterns. · we call it slop.
> AI is a tool. · you are the author. · be yourself. create. · create with fframes

| scene | time | what happens |
| --- | --- | --- |
| Question | 0.00–5.63 s | a brush "?", flash frames, giant type panning through dashed rulers, a selection box on "AI", the question typed with random pen marks; "stunning" is circled and filled with a molten SkSL shader |
| Gun | 5.63–8.13 s | an orange toy pistol (footage as a flat heat field) fires on the soundtrack's drop: recoil echoes, muzzle flash shader, sparks, a casing, pen smoke; the bullet burns "you can't." into the dark |
| Patterns | 8.13–11.13 s | a girl (footage as a thermal field) dashes across, the line appears in her wake; her echoes snap into six identical copies |
| Slop | 11.13–14.13 s | a porridge pot stirred at 52 px with pixel flies, drips and steam; "slop." knocked out of ink blots with turbulent edges and drips |
| Tool | 14.13–16.13 s | a vortex of pen strokes around "AI is a tool." that collapses into the next figure |
| Author | 16.13–19.13 s | a man in profile rises into frame out of a cold blur, warms to a yellow heat field and reaches his open hand toward "you are the author."; pen sparks fly off his fingertips |
| Create | 19.13–27.00 s | nineteen real tools orbit "create.", break into 404 copies that fly into the fframes wordmark and resolve into the logo |

Cuts sit on the 120 BPM soundtrack; every sound effect is timed from the same
frame numbers as the picture (`SFX` in `src/lib.rs`).

## Run

The soundtrack and effects are downloaded, not committed:

```sh
examples/one-shot/tools/fetch.sh
cargo run --release -p one-shot -- preview
cargo run --release -p one-shot -- render            # examples/one-shot/output/one-shot.mp4
cargo run --release -p one-shot -- strip all -n 36    # contact sheet
```

## How the footage was made

`media/` holds derivatives only, produced by `tools/footage.sh`:

1. Stock clips and photos are downloaded from Pexels.
2. `tools/vision.swift` uses Apple Vision: person segmentation for the man reaching out,
   per-person instance masks (tracked by centroid) for the running girl, foreground
   instance masks for the pistol and for lifting each tool out of its photo.
3. `tools/prep.py` turns masks into heat fields (blurred cores, warmer skin, a colour
   key and hole filling for the pistol's camo, skin hot and hair cool for the profile)
   and packs three frames per RGB atlas cell. `src/shaders/heat.sksl`
   maps them through thermal, yellow and flat-orange palettes with a glow. The pot is
   quantised to a shared 14-colour palette at 52 px and drawn by `pixel.sksl`. The
   wordmark from `landing/brand` is rasterised to sample the mosaic targets
   (`src/logo_points.rs`).

The pen (`src/ink.rs`) is procedural: Catmull-Rom centrelines turned into ribbons
with a pressure curve, drawn on along their length and re-jittered every two frames.

## Sources

Footage and photos, [Pexels licence](https://www.pexels.com/license/):

- toy pistol, [5257461](https://www.pexels.com/video/person-holding-a-toy-gun-5257461/)
- girls playing tag, [5273821](https://www.pexels.com/video/two-girls-playing-tag-inside-the-playground-5273821/)
- man reaching out, [10204155](https://www.pexels.com/video/side-view-of-a-man-moving-his-arm-10204155/)
- porridge pot, [4909363](https://www.pexels.com/video/a-person-is-stirring-a-pot-of-oatmeal-4909363/)
- tools: photos 1117543, 1203819, 12997264, 13044706, 1772123, 187334, 210927,
  30452350, 3394650, 4523060, 4678185, 6461504, 7138915, 9227661

Audio, [Mixkit free licence](https://mixkit.co/license/): "State of Mind" (music
#429, cut from 65.89 s so its drop lands on the shot) and sound effects 2369, 174,
166, 2364, 2384, 2536, 3194, 2350, 1668, 1662, 2655, 1491, 1236, 2358, 263, 536, 2361,
1493, 2589, 2359, 772.

`inspect` reports text cut by the canvas edge in the opening and during the camera
push into "stunning"; both are intended.
