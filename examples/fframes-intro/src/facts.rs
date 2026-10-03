//! Every measured number that appears on screen, in one place. They come
//! from real runs (see render-bench/vs-remotion and this project's CLI) and
//! are refreshed before the final render.

/// Release shown on the opening and closing screens.
pub const RELEASE_LABEL: &str = concat!("v", env!("CARGO_PKG_VERSION"));

/// Lines of Rust in fframes plus its SVG renderer fork, svgr (`wc -l` over
/// the git-tracked `.rs` files of both repositories).
pub const LINES_OF_RUST: u64 = 89_372;
/// Frames in this video.
pub const FRAMES: u64 = 7_650;
/// Measured wall-clock seconds for this video on Apple M5 Max (Skia on Metal,
/// libx264, with audio).
pub const RENDER_SECONDS: f32 = 79.6;

/// Complete H.264 MP4 export seconds for 300 frames at 3840×2160.
/// M5 Max, one run per pipeline after a three-frame warm-up. Remotion uses `MediaBunny`.
/// Source: render-bench/vs-remotion/measurements/m5-max-4k-circles.json.
pub const BENCH_REMOTION_S: f32 = 121.132_1;
/// Skia on Metal with fast shapes (usvgr 0.46.1): M4 Max, one run of render-bench/vs-remotion
/// on 2026-10-03. The same run measured Remotion + MediaBunny at 131.637 s, so the ratio to the
/// M5 Max Remotion time above is the lower one.
pub const BENCH_FFRAMES_S: f32 = 4.074_223;
pub const BENCH_FFRAMES_CPU_S: f32 = 69.740_39;
pub const BENCH_REMOTION_LABEL: &str = "REMOTION 4.0";
pub const BENCH_FFRAMES_LABEL: &str = "FFRAMES · SKIA ON METAL";
pub const BENCH_NOTE: &str = "M5 MAX · REMOTION 4.0.529 · SERIAL · H.264 MP4 · MEDIANS";
