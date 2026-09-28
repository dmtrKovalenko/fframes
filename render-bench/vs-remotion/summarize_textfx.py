#!/usr/bin/env python3
"""Turns results_textfx.tsv (from run_textfx.sh) into results_textfx.json: runs, medians, fps,
best configurations and speedups per x264 preset.
   python3 summarize_textfx.py [out.json ...]"""
import json, statistics, subprocess, sys
from collections import defaultdict
from pathlib import Path

here = Path(__file__).parent
outs = [Path(p) for p in sys.argv[1:]] or [here / "results_textfx.json"]
FRAMES, NODES = 300, 3334

runs = defaultdict(lambda: defaultdict(list))
frames = defaultdict(lambda: defaultdict(set))
loads = []
for line in (here / "results_textfx.tsv").read_text().splitlines():
    preset, name, _round, secs, l1, l2, packets, decoded = line.split("\t")
    runs[preset][name].append(float(secs))
    frames[preset][name].add((int(packets), int(decoded)))
    loads += [float(l1), float(l2)]

sh = lambda c: subprocess.run(c, shell=True, capture_output=True, text=True).stdout.strip()
result = {
    "workload": "TextFx: 1920x1080, 30 fps, 300 frames, 3,334 text nodes per frame (18-96 px) with seeded "
                "animated effects (rotate/scale/skew, opacity, HSL cycling, gradient fill, stroke, drop shadow, "
                "glow on every 10th node, letter-spacing, per-letter wave words)",
    "text_nodes_total": FRAMES * NODES,
    "machine": f"{sh('sysctl -n machdep.cpu.brand_string')}, {sh('sysctl -n hw.ncpu')} cores, "
               f"{int(sh('sysctl -n hw.memsize')) // 2**30} GB, macOS {sh('sw_vers -productVersion')}",
    "remotion_version": json.loads((here / "remotion/node_modules/remotion/package.json").read_text())["version"],
    "fframes_git": sh(f"git -C {here} rev-parse --short HEAD"),
    "encoder": "libx264, crf 18, same preset on both sides; timings are wall time of the whole command",
    "load_avg_1min": {"min": min(loads), "max": max(loads), "median": statistics.median(loads),
                      "note": "sampled right before and right after each run"},
    "presets": {},
}
for preset, by_cfg in runs.items():
    med = {k: round(statistics.median(v), 3) for k, v in by_cfg.items()}
    best_r = min((k for k in med if k.startswith("remotion")), key=med.get)
    best_g = min((k for k in med if k.startswith(("fframes_metal", "fframes_vulkan"))), key=med.get)
    result["presets"][preset] = {
        "runs": {k: v for k, v in by_cfg.items()},
        "median_seconds": med,
        "fps": {k: round(FRAMES / v, 2) for k, v in med.items()},
        "frames_packets_decoded": {k: sorted(v) for k, v in frames[preset].items()},
        "best": {"remotion": best_r, "fframes_gpu": best_g},
        "speedup": {
            "remotion_best_over_fframes_gpu_best": round(med[best_r] / med[best_g], 2),
            "remotion_best_over_fframes_cpu": round(med[best_r] / med["fframes_cpu"], 2)
            if "fframes_cpu" in med else None,
            "note": "> 1 means fframes is faster",
        },
    }
for out in outs:
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2))
for preset, p in result["presets"].items():
    print(preset, json.dumps({"median": p["median_seconds"], "best": p["best"], "speedup": p["speedup"]}, indent=1))
