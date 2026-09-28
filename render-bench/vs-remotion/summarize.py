#!/usr/bin/env python3
"""Turns results.tsv (from run.sh) into results.json with medians, fps and speedups."""
import json, statistics, subprocess, sys
from collections import defaultdict
from pathlib import Path

here = Path(__file__).parent
out = Path(sys.argv[1]) if len(sys.argv) > 1 else here / "results.json"
runs, loads = defaultdict(list), []
# results_ultrafast_sweep.tsv: extra Remotion concurrency sweep at x264 preset ultrafast (see README).
lines = (here / "results.tsv").read_text().splitlines()
if (here / "results_ultrafast_sweep.tsv").exists():
    lines += (here / "results_ultrafast_sweep.tsv").read_text().splitlines()
for line in lines:
    name, _round, secs, load = line.split("\t")
    runs[name].append(float(secs))
    loads.append(float(load))

FRAMES, NODES = 300, 3334
med = {k: round(statistics.median(v), 3) for k, v in runs.items()}
fps = {k: round(FRAMES / v, 1) for k, v in med.items()}
remotion_keys = [k for k in med if k.startswith("remotion_prebundled_c") and "ultrafast" not in k]
best_r = min(remotion_keys, key=med.get)
gpu_keys = [k for k in med if k.startswith(("fframes_metal", "fframes_vulkan")) and "ultrafast" not in k]
best_g = min(gpu_keys, key=med.get)

sh = lambda c: subprocess.run(c, shell=True, capture_output=True, text=True).stdout.strip()
result = {
    "machine": f"{sh('sysctl -n machdep.cpu.brand_string')}, {sh('sysctl -n hw.ncpu')} cores, "
               f"{int(sh('sysctl -n hw.memsize')) // 2**30} GB, macOS {sh('sw_vers -productVersion')}",
    "load_avg": {"min": min(loads), "max": max(loads), "median": statistics.median(loads),
                 "note": "1-minute load average sampled right before each run"},
    "remotion_version": json.loads((here / "remotion/node_modules/remotion/package.json").read_text())["version"],
    "fframes_backend": f"{best_g} (Skia GPU via fframes_skia_renderer); fframes git {sh('git -C ' + str(here) + ' rev-parse --short HEAD')}",
    "encoder": "libx264, crf 18, preset medium, yuv420p on both sides (Remotion defaults for --codec=h264)",
    "frames": FRAMES,
    "text_nodes_per_frame": NODES,
    "text_nodes_total": FRAMES * NODES,
    "runs": {
        "remotion_default": runs["remotion_default"],
        "remotion_best": {"concurrency": int(best_r.split("_c")[-1]), "prebundled": True, "times": runs[best_r]},
        "fframes_gpu": runs[best_g],
        "fframes_cpu": runs["fframes_cpu"],
        "all": runs,
    },
    "median_seconds": med,
    "fps": fps,
    "speedup": {
        "remotion_best_over_fframes_gpu": round(med[best_r] / med[best_g], 2),
        "remotion_default_over_fframes_gpu": round(med["remotion_default"] / med[best_g], 2),
        "remotion_best_over_fframes_cpu": round(med[best_r] / med["fframes_cpu"], 2),
    },
}
uf = [k for k in med if k.startswith("remotion") and "ultrafast" in k]
if uf:
    best_uf = min(uf, key=med.get)
    result["ultrafast_secondary"] = {
        "note": "x264 preset ultrafast on both sides (crf 18): encoder no longer the bottleneck",
        "remotion_best": {"config": best_uf, "median": med[best_uf]},
        "fframes_gpu": {"config": "fframes_metal_ctx2_ultrafast", "median": med["fframes_metal_ctx2_ultrafast"]},
        "fframes_cpu": {"config": "fframes_cpu_ultrafast", "median": med["fframes_cpu_ultrafast"]},
    }
    result["speedup"]["ultrafast_remotion_best_over_fframes_gpu"] = round(
        med[best_uf] / med["fframes_metal_ctx2_ultrafast"], 2)
    result["speedup"]["ultrafast_remotion_best_over_fframes_cpu"] = round(
        med[best_uf] / med["fframes_cpu_ultrafast"], 2)
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(json.dumps(result, indent=2))
print(json.dumps({k: result[k] for k in ("median_seconds", "fps", "speedup")}, indent=2))
