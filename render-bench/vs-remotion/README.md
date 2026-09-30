# fframes + Skia vs Remotion

```sh
./render-bench/vs-remotion/run.sh
```

Requires Rust, Node.js and Chromium (`CHROME_PATH` or `--chrome PATH`).
Results and frame PNGs go to `out/`; use `--out DIR` to choose a directory.

One fixed scene: 99,000 rectangles and 1,000 changing text digits at 1000×1000.
Both use DM Sans Regular from `examples/beta/media/DMSans-Regular.ttf`.
Remotion uses an unkeyed list with 12 dependent effect/state updates per element.
fframes computes the same content directly with Skia CPU and, when available,
Skia GPU (Metal on macOS, Vulkan elsewhere). GPU is skipped without a hardware device.
Results apply to this React workload with many effects.

All render serially without a video encoder. Times are medians of three 30-frame
runs after three warm-up frames. PNG compression is included; startup, warm-up
and disk writes are excluded.

The runner writes timings and versions to `results.json`, with a summary in
`results.md`. Generated results and dependency lockfiles are ignored.

Measured on Linux ARM64 Docker at `8af4abe`, with Remotion 4.0.529:

| Renderer           |     Median for 30 frames |
| ------------------ | -----------------------: |
| fframes + Skia CPU |                  3.877 s |
| fframes + Skia GPU | Skipped: no hardware GPU |
| Remotion           |                120.801 s |

fframes + Skia CPU was **31.16×** faster for this workload.
