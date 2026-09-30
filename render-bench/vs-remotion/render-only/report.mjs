export function median(values) {
  if (!values.length || values.some(x => !Number.isFinite(x) || x <= 0))
    throw new Error("invalid timing samples");
  const sorted = [...values].sort((a, b) => a - b);
  const m = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[m] : (sorted[m - 1] + sorted[m]) / 2;
}
export function summarize(records, plan) {
  const rows = [];
  for (const nodes of plan.nodes) {
    const native = records.filter(
      r => r.nodes === nodes && r.engine === "fframes"
    );
    const total = r => r.samples.reduce((n, s) => n + s.total_ms, 0);
    for (const mode of plan.modes) {
      const browser = records.filter(
        r => r.nodes === nodes && r.engine === mode
      );
      const valid = runs =>
        runs.length === plan.rounds &&
        new Set(runs.map(r => r.round)).size === plan.rounds &&
        runs.every(
          r =>
            r.status === "ok" &&
            r.verified &&
            Array.isArray(r.samples) &&
            r.samples.length === plan.frames &&
            r.samples.every(
              (s, i) =>
                s.frame === plan.warmup + i &&
                Number.isFinite(s.total_ms) &&
                s.total_ms > 0
            )
        );
      const complete = valid(native) && valid(browser);
      const rustMs = complete ? median(native.map(total)) : null;
      const reactMs = complete ? median(browser.map(total)) : null;
      rows.push({
        nodes,
        mode,
        status: complete ? "complete" : "incomplete",
        fframes_median_ms: rustMs,
        react_median_ms: reactMs,
        speedup: complete ? reactMs / rustMs : null,
        reaches_20x: complete ? reactMs / rustMs >= 20 : false,
      });
    }
  }
  return rows;
}
export function markdown(report) {
  const lines = [
    "# React DOM / fframes + Skia render-only stress benchmark",
    "",
    "Synthetic workload; ratios apply only to the named configuration. Production React, the listed number of rectangles in every frame, eight dependent effect passes in unkeyed-effects. No video encoder. Both sides produce PNGs in memory; PNG compression is included, disk writes and verification are excluded.",
    "",
    `Backend: ${report.plan.backend}; device: ${report.plan.device}; ${report.plan.rounds} rounds, ${report.plan.frames} measured frames and ${report.plan.warmup} warm-up frames per run.`,
    "",
    "| Nodes per frame | React configuration | React median ms | fframes median ms | Ratio | >=20x |",
    "|---:|---|---:|---:|---:|---|",
  ];
  for (const r of report.summary)
    lines.push(
      `| ${r.nodes} | ${r.mode} | ${r.react_median_ms?.toFixed(2) ?? "incomplete"} | ${r.fframes_median_ms?.toFixed(2) ?? "incomplete"} | ${r.speedup?.toFixed(2) ?? "n/a"} | ${r.reaches_20x ? "yes" : "no"} |`
    );
  lines.push(
    "",
    "All attempts, failures, per-frame samples, environment and source revision are retained in results.json. A crash or timeout is not a numeric speedup. The keyed and unkeyed direct controls are reported alongside the intentionally inefficient effects case. This is a React DOM/Chromium microbenchmark, not a measurement of the full Remotion pipeline.",
    ""
  );
  return lines.join("\n");
}
