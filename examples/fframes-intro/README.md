# fframes intro

The scale and benchmark scenes show **100,000 rectangles per frame** and the measured
**21.48×** render-only ratio for React's unkeyed, eight-effect stress case versus
fframes + Skia CPU. Timings are medians of three rounds of three frames, after one
warm-up frame: 6.496458 seconds / 0.302423 seconds. PNG capture/compression is included;
video encoding is excluded. The direct React controls measure 3.04× keyed and 2.37×
unkeyed. This is a synthetic stress case, not a general React or Remotion claim.

See the [benchmark methodology and raw evidence](../../render-bench/vs-remotion/README.md).
`src/facts.rs` holds the timing constants. The scale wall contains 100,000 rectangles
in a stylized layout; the measured benchmark canvas is 1000×1000. The separate
“Original cut” scene retains the historical Metal render timing for the earlier video.

Render and refresh the two checked-in landing videos:

```sh
cargo run --release -p fframes-intro -- inspect Scale..Benchmark@end --every 0.25s
cargo run --release -p fframes-intro -- frame 44s,49s,54.5s,58s
cargo run --release -p fframes-intro -- render -o out-intro.mp4
scripts/landing-video.sh out-intro.mp4
```

The intro uses Metal on macOS or Vulkan elsewhere. Its renderer is independent of the
**Skia CPU** backend used for the published benchmark. Keep both AV1 and H.264 landing
assets at 1280×720, 60 fps, 127.5 seconds, and below the deployment's 25 MiB limit.
