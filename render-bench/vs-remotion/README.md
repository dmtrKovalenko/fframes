# fframes vs Remotion

```sh
./render-bench/vs-remotion/run.sh
```

One scene, four pipelines: fframes CPU (tiny-skia), fframes + Skia GPU,
Remotion + FFmpeg (`renderMedia`), and Remotion + MediaBunny (`renderMediaOnWeb`).

The scene has 99,000 overlapping circles and 1,000 changing DM Sans digits in
20 panels. Every circle moves and changes color; 10,000 radii animate between
16 and 28 SVG units. The 1000×1000 viewBox stretches to 3840×2160. No filters.
Both implementations use the same animation math. Remotion has one keyed
component with `useCurrentFrame()` per drawable and a memoized parent tree.
This is an SVG overdraw stress test, not a claim about all Remotion videos.

Each pipeline exports one complete **300-frame, 10-second H.264 MP4**, without
audio, after a three-frame warm-up. Timing includes rendering, conversion,
encoder setup, encoding, draining, muxing and writes. Build, media preparation,
browser launch, bundling and warm-up are excluded. MediaBunny writes to OPFS;
copying its completed MP4 to the host is excluded. Output checks run after timing.

Settings: 30 fps, 8 Mbps target, GOP 30, 4:2:0. CPU uses all logical cores and
x264 medium. Skia uses `MaxPerformance`. Remotion + FFmpeg uses 18 tabs, capped
by available logical cores, and its default browser graphics settings. Skia
and Remotion + FFmpeg require hardware VideoToolbox on macOS; elsewhere they
use x264. Native codecs use one thread per segment. MediaBunny uses WebCodecs
`prefer-hardware`, which does not guarantee hardware encoding. Equal bitrate
targets do not guarantee equal quality.

Both fframes backends use 100,000-entry text caches. Skia also uses 100,000
geometry entries with a 51.2 MB budget per generation. GPU runs are skipped
when hardware is unavailable.

Measured on an M5 Max, one run each, except fframes + Skia GPU, which is one run on an
M4 Max with fast shapes (usvgr 0.46.1):

| Pipeline                   | Complete MP4 export | fframes GPU speedup |
| -------------------------- | ------------------: | ------------------: |
| fframes + Skia GPU         |             4.074 s |                   — |
| fframes CPU (tiny-skia)    |            69.740 s |              17.12× |
| Remotion + FFmpeg, 18 tabs |           109.227 s |              26.81× |
| Remotion + MediaBunny      |           121.132 s |              29.73× |

CPU uses software x264 encoding. The FFmpeg browser used default software
rasterization with hardware encoding; MediaBunny's Chrome reported Metal.
An eight-tab FFmpeg check was faster at 98.759 s (24.24× versus fframes GPU).
The fixed comparison above uses the requested 18 tabs.

The M4 Max run measured every pipeline on the slower chip: fframes CPU 81.141 s,
Remotion + FFmpeg 118.680 s and Remotion + MediaBunny 131.637 s, so the same-machine
speedups are higher than the ones above ([raw results](measurements/m4-max-skia-fast-shapes.json)).

[Raw measurements and source hashes](measurements/m5-max-4k-circles.json)
include the separate `usvgr` optimization used by the host build. To reproduce
that dependency setup, check out the recorded `usvgr` commit beside this repo
and put this in an untracked `.cargo/config.toml` before running:

```toml
[patch.crates-io]
usvgr = { path = "../svgr/crates/usvgr" }
```

Requires Rust, Node.js, ffprobe and Chrome with H.264 WebCodecs support.
Use `--chrome PATH` for MediaBunny, `--ffmpeg-chrome PATH` for FFmpeg, and
`--out DIR` for outputs. Each pipeline has a ten-minute limit. `results.json`
records settings, versions and output metadata; `results.md` shows all four
pipelines. Generated exports are ignored. On macOS the runner prevents idle sleep.
