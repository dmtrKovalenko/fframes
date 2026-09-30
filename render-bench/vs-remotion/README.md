# React DOM vs fframes + Skia: render-only stress benchmark

The primary benchmark now measures completed frames, **without a video encoder**.
The default is **1,000,000 elements in every frame**, not one million elements summed
across a video. `./run.sh` writes raw JSON and a Markdown results table.

This is a deliberately pathological React workload. It measures the cost of a huge SVG
DOM and effect-driven derived state against direct SVG scene generation in Rust. It is
not representative of ordinary React applications and is not a measurement of the full
Remotion pipeline. A 20x result, if measured, applies only to the named workload, backend,
machine and timing boundary. The reporter does not force that ratio or turn failures
into speedups.

## Workload and controls

Every frame renders up to a million opaque 1px rectangles on a 1000x1000 black canvas.
All rectangles are visible, identities rotate by 37 positions per frame, and every color
changes with both identity and frame. Both implementations use the same integer equations.
No fonts, external assets, random seeds, sleeps, busy loops, or video codecs are involved.

| Configuration | List identity | State updates |
|---|---|---|
| `keyed-direct` | Stable React keys | Direct frame-derived properties |
| `unkeyed-direct` | Intentionally omitted keys | Direct frame-derived properties |
| `unkeyed-effects` | Intentionally omitted keys | Eight dependent `useEffect`/state passes per element |
| fframes + Skia | Direct SVG scene generation | Frame-derived properties |

The effects case intentionally models a poor React implementation: values that could be
computed during render instead pass through eight state/effect stages. It is **not an
optimized React baseline**. Omitting keys alone does not guarantee slower reconciliation;
that is why both direct controls are included. React is compiled in production mode,
without StrictMode or development warning overhead. No virtualization or hidden elements
are used. Each React frame waits for every element's final effect commit before capture.

## What is timed

- **React:** update request, component construction/reconciliation, all effects and commits,
  browser rasterization, readback and PNG capture in memory. One warmed browser/page.
- **fframes:** `Previewer::render`, including SVG tree generation/conversion, Skia rasterization,
  GPU synchronization/readback when using a GPU, then PNG compression in memory. One warmed
  previewer, surface and renderer.
- **Both:** one frame at a time, same dimensions, same frame indices and warm-up count.
  Neither uses a video encoder, audio, muxing, or asynchronous video-encoding pipeline.
- **Excluded:** compiling/bundling, process/browser/context startup, disk writes and correctness
  checks. PNG compression is included on both sides because Chromium exposes screenshots as
  compressed images; this is **not** a codec-free raw-rasterization microbenchmark. Skia raw
  render time and PNG time are also retained separately. Chromium commit and capture time
  are retained separately; its capture time includes rasterization plus PNG compression.

Each measured PNG is decoded and every RGBA pixel is checked against an independent image
formula. Different images invalidate a run. This prevents timing stale effect state, an
empty scene, a partial DOM, or skipped native rendering.

All configurations run each round, with order rotated between rounds. The table reports
ratios of median per-run total times, never the fastest sample. Missing rounds, crashes,
timeouts, invalid images or nonpositive timings produce `incomplete`, not a numeric ratio.
Every attempt is retained in `results.json`. The `reaches_20x` field uses the unrounded ratio.

## Run

Requires Rust, Node 22+, Chrome/Chromium, and the repository's native build prerequisites.
Use a machine with enough memory: a million React components with eight effects can exceed
several GB. Memory exhaustion is a reported failure, not a performance claim.

```sh
# Repository root; CPU Skia works without a GPU.
cargo build --release -p render-only-bench
cd render-bench/vs-remotion/render-only
npm ci
npm test

# Small correctness/smoke run first.
node run.mjs --nodes 1000 --rounds 1 --frames 2 --chrome /usr/bin/chromium

# Default complete million-element matrix: 3 rounds, 3 measured frames, 1 warm-up.
node run.mjs --chrome /usr/bin/chromium --out out/million

# Predetermined size sweep; retain every size, including failures.
node run.mjs --nodes 1000,10000,100000,1000000 --out out/sweep
```

On macOS pass `--chrome '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'`.
`CHROME_PATH` is also supported. GPU backends are explicit and fail if unavailable:

```sh
cargo build --release -p render-only-bench --features metal
# or --features vulkan on a Vulkan machine
./render-bench/vs-remotion/run.sh --backend metal --device 'Apple M4 Max' \
  --chrome '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
```

Other options: `--binary PATH`, `--frames N`, `--warmup N`, `--rounds N`,
`--timeout-ms N` (per complete attempt, default 300000), `--modes CSV`, `--out DIR`.
Do not describe a CPU or software Vulkan run as a physical-GPU result. Preserve the raw
JSON, command, source revision, binary/bundle hashes and machine details when quoting a ratio.
`--modes` supports focused diagnosis; publish all three controls for comparative claims.

## Results and historical benchmark

Measured runs for this change are recorded under `render-only/results/` once available.
The output report states whether 20x was actually reached for each configuration.

The previous encoded text-grid experiment is preserved in [README-encoded.md](README-encoded.md)
and `run_encoded.sh`. Its historical 1.50x / 2.85x ratios include H.264 encoding and use a
different workload. They cannot be compared directly with the render-only stress ratios.
`run_textfx.sh`, `summarize.py` and `summarize_textfx.py` remain tools for that legacy experiment.
The repository-wide `render-bench` stage profiler is unchanged.
