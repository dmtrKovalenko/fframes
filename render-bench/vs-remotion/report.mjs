function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
}
export function summarize(records, plan) {
  const native = records.filter(r => r.engine === "fframes");
  const browser = records.filter(r => r.engine === "remotion");
  const valid = runs =>
    runs.length === plan.rounds &&
    new Set(runs.map(r => r.round)).size === plan.rounds &&
    runs.every(
      r =>
        r.status === "ok" &&
        r.verified &&
        r.samples?.length === plan.frames &&
        r.samples.every(
          (s, i) =>
            s.frame === plan.warmup + i &&
            Number.isFinite(s.total_ms) &&
            s.total_ms > 0
        )
    );
  const complete = valid(native) && valid(browser);
  const total = r => r.samples.reduce((n, s) => n + s.total_ms, 0);
  const fframes = complete ? median(native.map(total)) : null;
  const remotion = complete ? median(browser.map(total)) : null;
  return {
    status: complete ? "complete" : "incomplete",
    fframes_median_ms: fframes,
    remotion_median_ms: remotion,
    speedup: complete ? remotion / fframes : null,
  };
}
export function markdown(report) {
  const result = report.summary;
  return [
    "# fframes + Skia vs Remotion",
    "",
    "100,000 elements: 99,000 rectangles and 1,000 changing text digits. Remotion uses an unkeyed list with 12 dependent effect/state updates per element. Both render identical 1000×1000 frames serially, without a video encoder.",
    "",
    "Median of 3 rounds, each with 3 warm-up and 30 measured frames. PNG compression is included; startup, warm-up, disk writes and pixel verification are excluded.",
    "",
    "| Renderer | Median for 30 frames |",
    "|---|---:|",
    `| fframes + Skia CPU | ${result.fframes_median_ms?.toFixed(2) ?? "incomplete"} ms |`,
    `| Remotion | ${result.remotion_median_ms?.toFixed(2) ?? "incomplete"} ms |`,
    "",
    `Speedup in this effect-heavy workload: ${result.speedup?.toFixed(2) ?? "unavailable"}×.`,
    "",
  ].join("\n");
}
