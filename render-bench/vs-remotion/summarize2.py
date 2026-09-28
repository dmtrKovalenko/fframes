#!/usr/bin/env python3
"""results2.tsv (from run2.sh) -> results2.json: per workload and x264 preset, the fastest
Remotion and fframes configuration by median wall time, their runs, fps and the speedup.

usage: python3 summarize2.py [results2.tsv] [results2.json]
"""
import json, statistics, subprocess, sys
from collections import defaultdict
from pathlib import Path

here = Path(__file__).parent
tsv = Path(sys.argv[1]) if len(sys.argv) > 1 else here / "results2.tsv"
out = Path(sys.argv[2]) if len(sys.argv) > 2 else here / "results2.json"

WORKLOADS = {
    "podcast": {"resolution": "1920x1080", "fps": 30, "frames": 600,
                "what": "2 video feeds (left30.mp4/right30.mp4, 1280x720 h264) with rounded corners and drop shadows, "
                        "4 blurred moving gradient blobs (blur 90px), word-by-word captions, progress bar, spring title card"},
    "motion": {"resolution": "1920x1080", "fps": 60, "frames": 600,
               "what": "blurred photo backdrop + gradient overlay, 40 animated SVG shapes, 3 layered cards with "
                       "box-shadows and blurred glows (one with poster.jpg), per-letter kinetic headline with a glow"},
    "podcast4k": {"resolution": "3840x2160", "fps": 30, "frames": 150,
                  "what": "the podcast workload, first 5 s, rendered at 2x scale (Remotion --scale=2, fframes scale_resolution 2.0)"},
}

runs = defaultdict(list)
loads = defaultdict(list)
frames = defaultdict(set)
for line in tsv.read_text().splitlines():
    wl, preset, cfg, _round, secs, l1, l2, packets, decoded = line.split("\t")
    runs[(wl, preset, cfg)].append(float(secs))
    loads[(wl, preset, cfg)].append(float(l1))
    frames[(wl, preset, cfg)].add((int(packets), int(decoded)))

sh = lambda c: subprocess.run(c, shell=True, capture_output=True, text=True).stdout.strip()
result = {
    "machine": f"{sh('sysctl -n machdep.cpu.brand_string')}, {sh('sysctl -n hw.ncpu')} cores, "
               f"{int(sh('sysctl -n hw.memsize')) // 2**30} GB, macOS {sh('sw_vers -productVersion')}",
    "remotion_version": json.loads((here / "remotion/node_modules/remotion/package.json").read_text())["version"],
    "fframes_git": sh(f"git -C {here} rev-parse --short HEAD"),
    "encoder": "libx264 crf 18 on both sides, preset as keyed; wall time of the whole command "
               "(Remotion: pre-bundled, bundling not counted; fframes: compiled release binary)",
    "method": "median of 5 interleaved rounds, 10 s idle between runs, runs discarded and redone if the "
              "machine's other render job started during them",
    "workloads": {},
}
all_loads = [l for v in loads.values() for l in v]
result["load_avg_1min_before_runs"] = {"min": min(all_loads), "median": statistics.median(all_loads), "max": max(all_loads)}

for wl, meta in WORKLOADS.items():
    entry = dict(meta)
    for preset in ("ultrafast", "medium"):
        keys = [k for k in runs if k[0] == wl and k[1] == preset]
        if not keys:
            continue
        med = {k[2]: round(statistics.median(runs[k]), 3) for k in keys}
        best = {}
        for tool in ("remotion", "fframes"):
            cands = [c for c in med if c.startswith(tool)]
            c = min(cands, key=med.get)
            best[tool] = {
                "config": c,
                "runs": runs[(wl, preset, c)],
                "median_s": med[c],
                "fps": round(meta["frames"] / med[c], 1),
                "load_avg_before_runs": loads[(wl, preset, c)],
                "frames_packets_decoded": sorted(frames[(wl, preset, c)]),
            }
        entry[preset] = {
            "remotion_best": best["remotion"],
            "fframes_best": best["fframes"],
            "speedup": round(best["remotion"]["median_s"] / best["fframes"]["median_s"], 2),
            "all_medians_s": dict(sorted(med.items(), key=lambda kv: kv[1])),
            "all_runs_s": {k[2]: runs[k] for k in sorted(keys)},
        }
    result["workloads"][wl] = entry

out.write_text(json.dumps(result, indent=2))
for wl, e in result["workloads"].items():
    for preset in ("ultrafast", "medium"):
        if preset in e:
            p = e[preset]
            print(f"{wl:10} {preset:9} remotion {p['remotion_best']['median_s']:7.2f}s ({p['remotion_best']['config']})  "
                  f"fframes {p['fframes_best']['median_s']:6.2f}s ({p['fframes_best']['config']})  x{p['speedup']}")
