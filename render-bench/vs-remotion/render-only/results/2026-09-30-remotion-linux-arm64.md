# Remotion / fframes + Skia render-only benchmark

Same 1000×1000 SVG frames: 99% rectangles, 1% text digits. Remotion renderFrames() and fframes + Skia capture PNGs without a video encoder. The effects case uses eight dependent state/effect passes per element.

Backend: skia-cpu; device: CPU; no physical GPU; 3 rounds, 30 measured frames and 3 warm-up frames per run.

| Nodes per frame | Remotion configuration | Remotion median ms | fframes median ms | Ratio | >=20x |
|---:|---|---:|---:|---:|---|
| 100000 | keyed-direct | 12069.56 | 3681.01 | 3.28 | no |
| 100000 | unkeyed-direct | 8801.52 | 3681.01 | 2.39 | no |
| 100000 | unkeyed-effects | 67756.29 | 3681.01 | 18.41 | no |

Times are medians of each round's measured-frame total. Startup, warm-up, disk writes and pixel checks are excluded. PNG compression is included. Failures have no speedup; raw attempts are retained in the accompanying JSON.
