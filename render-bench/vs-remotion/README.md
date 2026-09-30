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

The Remotion run is being measured. Earlier [React DOM results](render-only/results/2026-09-30-linux-arm64.md)
used rectangles only and did not run Remotion; their 21.48× result does not apply
to this workload. The intro's requested 21.8× copy is not a measured result of this run.

The old H.264 experiment remains in [README-encoded.md](README-encoded.md) and
`run_encoded.sh`. Its timings include video encoding.
