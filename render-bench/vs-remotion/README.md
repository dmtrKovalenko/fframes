# fframes + Skia vs Remotion

```sh
./render-bench/vs-remotion/run.sh
```

Requires Rust, Node.js and Chromium (`CHROME_PATH` or `--chrome PATH`).
Results and frame PNGs go to `out/`; use `--out DIR` to choose a directory.

One fixed scene: 99,000 rectangles and 1,000 changing text digits at 1000×1000.
Remotion renders an unkeyed list with 12 dependent effect/state updates per element;
fframes computes the same final content directly with Skia CPU. This measures an
effect-heavy React workload, not typical Remotion performance.

Both render serially without a video encoder. The reported time is the median of
three 30-frame runs after three warm-up frames. It includes PNG compression and
excludes startup, warm-up and disk writes. Every measured frame must match the
expected pixels exactly before a speedup is reported.

The runner writes timings, versions and pixel hashes to `results.json`, with a
summary in `results.md`. Generated results and dependency lockfiles are ignored.
