# Remotion vs fframes + Skia

Both render the same 1000×1000 frames: 99,000 rectangles and 1,000 changing text
digits, using the same bundled font. Every output pixel is checked against the
scene definition. Defaults: 100k elements, 30 measured frames, 3 warm-up frames,
3 rounds, one frame at a time.

Remotion 4.0.529 uses `renderFrames()` with PNG buffers. fframes uses `Previewer`
and Skia. Timings include frame updates, rasterization and PNG compression.
They exclude bundling, startup, warm-up, disk writes and verification. Neither
side runs a video encoder. Results use median round totals.

| Remotion configuration | Updates |
|---|---|
| `keyed-direct` | Stable keys, frame-derived properties |
| `unkeyed-direct` | Same properties, omitted keys |
| `unkeyed-effects` | Eight dependent state/effect passes per element |

The effects case intentionally uses inefficient derived state. All three controls
render identical content. Skia CPU is the default; GPU runs require `metal` or
`vulkan` and a matching Cargo feature.

```sh
cargo build --release -p render-only-bench
cd render-bench/vs-remotion/render-only
npm ci
npm test
node run.mjs --chrome /usr/bin/chromium
```

Options: `--nodes`, `--frames`, `--warmup`, `--rounds`, `--modes`, `--backend`,
`--device`, `--binary`, `--chrome`, `--timeout-ms`, `--out`. `CHROME_PATH` also works.
The root `./run.sh` forwards these options. Output includes JSON samples, PNGs,
source/binary/font hashes and a Markdown table. Failed runs have no speedup.

## Results

Linux ARM64 Docker, Skia CPU, Chromium 154; three rounds. All 360 measured PNGs
match exactly across both engines.

| Configuration | Median time, 30 frames | Speedup |
|---|---:|---:|
| fframes + Skia CPU | 3.681 s | baseline |
| Remotion `keyed-direct` | 12.070 s | 3.28× |
| Remotion `unkeyed-direct` | 8.802 s | 2.39× |
| Remotion `unkeyed-effects` | 67.756 s | 18.41× |

[Raw samples](render-only/results/2026-09-30-remotion-linux-arm64.json) and
[report](render-only/results/2026-09-30-remotion-linux-arm64.md).
Earlier [React DOM results](render-only/results/2026-09-30-linux-arm64.md) used
rectangles only and did not run Remotion. Their 21.48× does not apply here.

The old H.264 experiment remains in [README-encoded.md](README-encoded.md) and
`run_encoded.sh`. Its timings include video encoding.
