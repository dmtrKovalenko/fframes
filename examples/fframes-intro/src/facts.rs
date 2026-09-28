//! Every measured number that appears on screen, in one place. They come
//! from real runs (see render-bench/vs-remotion and this project's CLI) and
//! are refreshed before the final render.

/// Lines of Rust in fframes plus its SVG renderer fork, svgr (`wc -l` over
/// the git-tracked `.rs` files of both repositories).
pub const LINES_OF_RUST: u64 = 89_372;
/// Frames in this video.
pub const FRAMES: u64 = 7_650;
/// Wall-clock seconds of the final render of this video (Skia on Metal,
/// libx264, with audio).
pub const RENDER_SECONDS: f32 = 40.5;

/// Benchmark: 1,000,000 text nodes (300 frames × 3,334 nodes, 1920x1080,
/// H.264, libx264 ultrafast crf 18 on both), median wall-clock seconds of
/// the whole command. Remotion: 4.0.529, pre-bundled, concurrency 12 (its
/// best). fframes: Skia on Metal, 2 GPU contexts.
pub const BENCH_REMOTION_S: f32 = if MOCK_BENCH { 50.4 } else { 7.40 };
pub const BENCH_FFRAMES_S: f32 = 2.60;
pub const BENCH_REMOTION_LABEL: &str = "REMOTION 4.0";
pub const BENCH_FFRAMES_LABEL: &str = "FFRAMES · SKIA ON METAL";
/// The same run with x264 preset medium on both sides, where encoding takes
/// a larger share of the time.
pub const BENCH_MEDIUM_REMOTION_S: f32 = 9.41;
pub const BENCH_MEDIUM_FFRAMES_S: f32 = 6.29;
pub const BENCH_NOTE: &str = "M4 MAX · REMOTION 4.0.529, BEST CONCURRENCY, PRE-BUNDLED · X264 ULTRAFAST CRF 18 ON BOTH · MEDIANS";

/// Placeholder benchmark numbers while the realistic workloads are measured.
/// Never publish a render made with this on.
pub const MOCK_BENCH: bool = true;
