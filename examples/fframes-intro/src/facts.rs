//! Every measured number that appears on screen, in one place. They come
//! from recorded runs; each measurement below describes its workload and
//! provenance. The original cut's render time is retained as historical data.

/// Release shown on the opening and closing screens.
pub const RELEASE_LABEL: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// Lines of Rust in fframes plus its SVG renderer fork, svgr (`wc -l` over
/// the git-tracked `.rs` files of both repositories).
pub const LINES_OF_RUST: u64 = 89_372;
/// Frames in this video.
pub const FRAMES: u64 = 7_650;
/// Historical wall-clock seconds for the original intro cut (Skia on Metal,
/// libx264, with audio), before the 100k benchmark revision.
pub const RENDER_SECONDS: f32 = 40.5;

/// Render-only stress benchmark: 100,000 rectangles per frame, 1000x1000.
/// Median totals for three measured frames after one warm-up, across three rounds.
/// React 19.2 production, unkeyed list with eight dependent effect/state passes;
/// fframes + Skia CPU. Linux ARM64 Docker. PNG capture/compression included,
/// video encoding excluded. Source and all controls:
/// render-bench/vs-remotion/render-only/results/2026-09-30-linux-arm64.json
pub const BENCH_NODES: usize = 100_000;
pub const BENCH_FRAMES: usize = 3;
pub const BENCH_REACT_S: f32 = 6.496_458;
pub const BENCH_FFRAMES_S: f32 = 0.302_422_9;
pub const BENCH_REACT_LABEL: &str = "REACT · EFFECTS STRESS";
pub const BENCH_FFRAMES_LABEL: &str = "FFRAMES · SKIA CPU";
pub const BENCH_NOTE: &str =
    "LINUX ARM64 · 3 ROUNDS · MEDIAN TOTALS · PNG CAPTURE INCLUDED · NO VIDEO ENCODER";
