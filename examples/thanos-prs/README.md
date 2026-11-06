# The Thanos Button

A 13.2-second fframes video. [Dax’s lava to-do list was the visual reference.](https://x.com/thdxr/status/2118601135917269119)

The pull request index uses the November 6, 2026 GitHub snapshot in `pull-requests.json`. It contains 57 fframes PRs and 346 fff PRs across all states. It excludes `dkovalenko-oai`, `dmtrKovalenko`, and the supplied spelling `dmtrKovbalenko`. The snap uses a seeded sample of 201 records. All GitHub access was read-only.

## Preview and render

From the repository root, on macOS with Metal or Linux/Windows with Vulkan:

```sh
cargo run --release -p thanos-prs -- preview 0s --paused
mkdir -p examples/thanos-prs/output
cargo run --release -p thanos-prs -- render -o examples/thanos-prs/output/thanos-prs.mp4
```

Use `--renderer skia-cpu` when a GPU is unavailable for export. SkSL drives the overview tiles, the shockwave refraction, the lava reaction, and the ash that carries away the affected rows. The cursor and UI use SVG and the sound cues are placed on the fframes timeline. `sound.py` synthesizes the original WAV assets.

## Gauntlet artwork

The built-in image generator created the source artwork for `media/gauntlet.png`. It was cropped and resized with its transparency preserved.

### Image prompt

> Use case: stylized-concept. A single Infinity Gauntlet from Marvel, isolated as a transparent PNG hero prop for a pull-request app animation. One gold armored right hand and short ornate segmented brass cuff, wrist at bottom and hand pointed up in the iconic ready-to-snap pose, thumb pressed against raised middle fingertip, index finger slightly lifted, ring finger and little finger curled, back of hand visible at a cinematic three-quarter angle. Distinct purple, red, blue, orange and green Infinity Stones with a larger glowing yellow gemstone on the back. Crisp high-end game collectible render, polished beveled antique gold, restrained warm rim light, intricate physical material, very clear silhouette with all fingers fully inside frame. Centered square composition, glove fills frame, cuff ends cleanly. Genuine transparent background with no backdrop, no ground, no detached sparks, no text, no watermark.
