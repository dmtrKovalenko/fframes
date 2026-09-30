# fframes + Skia vs Remotion

```sh
./render-bench/vs-remotion/run.sh
```

Requires Rust, Node.js and Chromium (`CHROME_PATH` or `--chrome PATH`).
Results and frame PNGs go to `out/`; use `--out DIR` to choose a directory.

One fixed scene: 99,000 rectangles and 1,000 changing text digits at 1000×1000.
Remotion renders an unkeyed list with 12 dependent effect/state updates per element;
fframes computes the same final content directly with Skia CPU and, when available,
Skia GPU (Metal on macOS, Vulkan elsewhere). GPU is skipped when no hardware device
is available. This measures an
effect-heavy React workload, not typical Remotion performance.

All render serially without a video encoder. The reported time is the median of
three 30-frame runs after three warm-up frames. It includes PNG compression and
excludes startup, warm-up and disk writes. Every measured frame must match the
expected pixels exactly before a speedup is reported.

The runner writes timings, versions and pixel hashes to `results.json`, with a
summary in `results.md`. Generated results and dependency lockfiles are ignored.

Measured on Linux ARM64 Docker with Remotion 4.0.529 and React 19.2.0,
source `fe45c03`. All 180 measured frames match exactly.

| Renderer           | Median for 30 frames |
| ------------------ | -------------------: |
| fframes + Skia CPU |              4.403 s |
| Remotion           |            116.465 s |

Speedup for this workload: **26.45×**.
