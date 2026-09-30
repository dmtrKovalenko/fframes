# Remotion vs fframes: 1,000,000 text nodes

One video, built twice: 1920x1080, 30 fps, 10 s (300 frames), H.264 mp4, no audio. Every
frame draws 3,334 text nodes (300 x 3,334 = 1,000,200 in total) in a 47 x 71 grid: short
numbers and the word "fframes", DM Sans Regular 10 px (the same `DMSans-Regular.ttf` on both
sides), with per-node position, opacity and color computed from the frame index, so nothing
can be reused from one frame to the next.

- `remotion/`: Remotion 4.0.529 project. `src/TextGrid.tsx` renders 3,334 absolutely
  positioned `<span>`s per frame (plain React, `useCurrentFrame`, font loaded with
  `@remotion/fonts`).
- `fframes/`: workspace crate `vs-remotion-bench`. `src/main.rs` builds 3,334 `<text>` nodes
  per frame with `svgr!` in a loop and renders with `fframes::render` on the Skia GPU
  backend (Metal or Vulkan/MoltenVK) or the CPU backend.

The per-node math (position, alpha, HSL to RGB, label) is the same code in both files, so both
videos show the same frames. Frame 150 of each is in `logs/remotion_f150.png` and
`logs/fframes_f150.png`. Chrome draws the glyphs slightly heavier; the layout, text and
colors are the same.

## Same settings on both sides

| | Remotion | fframes |
|---|---|---|
| Encoder | libx264 (Remotion's bundled ffmpeg) | libx264 (FFmpeg 9 linked into fframes) |
| Rate control | crf 18 (Remotion's h264 default) | crf 18 |
| x264 preset | none passed, so libx264's default `medium` | `medium`, set explicitly |
| Pixel format | yuvj420p (Remotion's default JPEG frames) | yuv420p |
| Output | 300 frames, 1920x1080, 109.5 MB | 300 frames, 1920x1080, 104.4 MB |

Both timings include encoding. Encoding runs alongside rendering on both sides, and at
preset `medium` it is expensive for this content, because thousands of small moving glyphs
compress badly. To see the renderers with less encoding in the way, both sides were also
run at x264 preset `ultrafast` (still crf 18).

What each time covers: wall time of the whole command, measured from outside with a
high-resolution timer (`run_encoded.sh`).
- `remotion_default`: `npx remotion render src/index.ts ...`, which bundles (about 1.2 s),
  starts Chrome, renders and encodes.
- `remotion_prebundled_cN`: `remotion render build ...` on a bundle made once with
  `remotion bundle`. Bundling is not counted (the fframes compile is not counted either);
  Chrome start, rendering and encoding are.
- `fframes_*`: the release binary, already compiled: process start, font loading,
  GPU context setup, rendering, encoding, muxing.

Remotion was given its best settings: concurrency 8 (its default on 16 cores), 12 and 16,
with the fastest median reported. The Chrome headless shell download and one warm-up render
came first and are not counted. fframes was tried on Metal and Vulkan with 1 or 2 GPU
contexts (`GPU_CONTEXTS`), and its fastest median is reported the same way.

## Results

Machine: Apple M4 Max (16 cores: 12 performance, 4 efficiency), 64 GB, macOS 26.5.1. Node
v26.8.2, Chrome Headless Shell 149.0.7790.0, rustc 1.92.0, fframes at git 1070159. Runs
were interleaved (round 1 of every configuration, then round 2, and so on) with 10 s idle
between runs, 5 rounds, median reported. The 1-minute load average before each run was 10.5
to 30 (median 19.6). About half of that comes from the benchmark itself: Chrome and x264
threads count toward the load average, and the 1-minute average lags. The rest was macOS
background daemons (spotlightknowledged, mediaanalysisd).

Main comparison (x264 medium, crf 18, all timings include encoding):

| configuration | median (s) | fps | runs (s) |
|---|---:|---:|---|
| Remotion, `npx remotion render` default (bundling included) | 10.56 | 28.4 | 10.41 10.68 10.56 10.59 10.18 |
| Remotion, pre-bundled, concurrency 8 (best) | **9.41** | 31.9 | 9.00 9.87 9.62 9.16 9.41 |
| Remotion, pre-bundled, concurrency 12 | 9.68 | 31.0 | 9.68 9.48 9.86 9.46 9.74 |
| Remotion, pre-bundled, concurrency 16 | 10.05 | 29.9 | 9.98 10.06 10.05 9.67 12.11 |
| fframes Skia Metal, 1 GPU context (default config) | 7.50 | 40.0 | 7.80 7.50 7.50 7.32 7.34 |
| fframes Skia Metal, 2 GPU contexts | 6.47 | 46.4 | 6.56 6.47 6.47 6.31 6.46 |
| fframes Skia Vulkan (MoltenVK), 2 GPU contexts (best) | **6.29** | 47.7 | 6.52 6.36 6.25 6.28 6.29 |
| fframes CPU backend (tiny-skia) | 6.80 | 44.1 | 7.08 6.86 6.80 6.68 6.77 |

**Speedup: fframes GPU is 1.50x faster than Remotion's best (9.41 s / 6.29 s)** and 1.68x
faster than a default `npx remotion render` (10.56 s / 6.29 s). A large share of both
times is the same libx264 encode at the same settings, so the two renderers differ by more
than 1.5x (see the secondary table).

Secondary (x264 ultrafast, crf 18, encoding still included):

| configuration | median (s) | fps | runs (s) |
|---|---:|---:|---|
| Remotion, pre-bundled, concurrency 8 | 8.75 | 34.3 | 7.32 7.41 8.75 8.85 9.15 |
| Remotion, pre-bundled, concurrency 12 (best, separate 3-run sweep) | **7.40** | 40.5 | 7.40 6.62 7.60 |
| Remotion, pre-bundled, concurrency 16 (separate 3-run sweep) | 7.59 | 39.5 | 7.86 7.59 7.09 |
| fframes Skia Metal, 2 GPU contexts | **2.60** | 115.4 | 2.70 2.61 2.50 2.57 2.60 |
| fframes CPU backend | 3.15 | 95.3 | 3.21 3.10 3.14 3.16 3.15 |

At preset ultrafast, where encoding costs less, fframes GPU is **2.85x** faster (7.40 s / 2.60 s).
Remotion gains only about 2 s from the faster preset, because Chrome's per-frame screenshots
now take most of its time.

All numbers are in `results.tsv` and `results_ultrafast_sweep.tsv` and are summarized in
`results.json` (`python3 summarize.py`).

### Caveats

- The headline 1.5x is small because x264 at preset `medium` costs a lot of CPU on both
  sides. On its own, encoding these 300 frames with libx264 medium, crf 18, from raw YUV
  takes 3.8 s with all 16 cores (78 fps). fframes renders on the same cores, so its total
  (6.29 s) comes out close to rendering (2.60 s at ultrafast) plus that encode. Remotion's
  total rises by only 2.0 s (7.40 s to 9.41 s), probably because its render stage leaves
  cores idle that the encoder can use. That is why the gap shrinks from 2.85x to 1.5x.
- On this M4 Max, fframes' CPU backend (16 cores) is nearly as fast as the GPU backend
  (6.80 s vs 6.29 s, or 3.15 s vs 2.60 s at ultrafast). This scene is 3,334 small glyph
  runs per frame. Building the tree and laying out the text happen on the CPU for both
  backends, so the GPU helps less here than it does for scenes that are heavy to rasterize.
- fframes' default GPU config (1 context) is 7.50 s. The 6.29 s result uses 2 GPU contexts
  (`SkiaPipelineConcurrencyPolicy::Concurrency(2)`). Remotion was tuned the same way
  (concurrency sweep plus pre-bundling), so both sides are shown at their best settings.
  Default against default is 10.56 s vs 7.50 s = 1.41x.
- The fframes CPU backend wrote **299 frames instead of 300** in every run (the GPU backends
  and Remotion wrote 300). This looks like a bug in the CPU render path and should be
  checked before quoting CPU numbers next to GPU numbers. It changes the CPU time by
  about 0.3%.
- Earlier passes with no idle time between runs, or with other renders on the machine, were
  discarded. They showed the same ordering with more noise.
- Single machine, Apple Silicon only. Linux or x86 results can differ, especially for
  Chrome and MoltenVK compared with native Vulkan.

## Reproduce

```sh
# Remotion (once)
cd render-bench/vs-remotion/remotion
npm install
npx remotion browser ensure                 # Chrome headless shell download, not timed
npx remotion bundle src/index.ts --out-dir build
npx remotion render src/index.ts TextGrid out/warmup.mp4 --codec=h264   # warm-up

# fframes (once; the Skia build needs LIBCLANG_PATH/CLANG_PATH, see the repo notes)
cd ../../..                                 # repo root
cargo build --release -p vs-remotion-bench

# Benchmark: 5 interleaved rounds, 10 s idle between runs
cd render-bench/vs-remotion
ROUNDS=5 COOLDOWN=10 ./run_encoded.sh               # writes results.tsv, videos and logs in out/
python3 summarize.py                        # writes results.json
```

Single runs:

```sh
# Remotion
cd render-bench/vs-remotion/remotion
npx remotion render src/index.ts TextGrid out/remotion.mp4 --codec=h264
npx remotion render build TextGrid out/remotion.mp4 --codec=h264 --concurrency=8

# fframes (from the repo root); backend: metal | vulkan | cpu, third arg: x264 preset
GPU_CONTEXTS=2 DYLD_LIBRARY_PATH=/opt/homebrew/lib \
  target/release/vs-remotion-bench vulkan out/fframes.mp4 medium
```

Checking the outputs:

```sh
ffprobe -v error -select_streams v -count_frames \
  -show_entries stream=codec_name,width,height,pix_fmt,nb_read_frames -of csv=p=0 out/fframes_vulkan_ctx2.mp4
ffmpeg -i out/fframes_vulkan_ctx2.mp4 -vf "select=eq(n\,150)" -frames:v 1 fframes_f150.png
```

`logs/remotion_log.txt` and `logs/fframes_log.txt` hold the terminal output of one run of
each.
