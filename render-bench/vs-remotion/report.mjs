function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
}
export function summarize(records, plan) {
  const native = records.filter(r => r.engine === "fframes");
  const browser = records.filter(r => r.engine === "remotion");
  const gpu = records.filter(r => r.engine === "fframes-gpu");
  const valid = runs =>
    runs.length === plan.rounds &&
    new Set(runs.map(r => r.round)).size === plan.rounds &&
    runs.every(
      r =>
        r.status === "ok" &&
        r.samples?.length === plan.frames &&
        r.samples.every(
          (s, i) =>
            s.frame === plan.warmup + i &&
            Number.isFinite(s.total_ms) &&
            s.total_ms > 0
        )
    );
  const complete = valid(native) && valid(browser);
  const gpuComplete = valid(gpu);
  const gpuSkipped =
    gpu.length === plan.rounds && gpu.every(r => r.status === "skipped");
  const total = r => r.samples.reduce((n, s) => n + s.total_ms, 0);
  const fframes = complete ? median(native.map(total)) : null;
  const remotion = complete ? median(browser.map(total)) : null;
  const gpuMs = gpuComplete ? median(gpu.map(total)) : null;
  return {
    status: complete && (gpuComplete || gpuSkipped) ? "complete" : "incomplete",
    fframes_median_ms: fframes,
    remotion_median_ms: remotion,
    speedup: complete ? remotion / fframes : null,
    gpu_median_ms: gpuMs,
    gpu_speedup: complete && gpuComplete ? remotion / gpuMs : null,
    gpu_status: gpuSkipped
      ? "skipped"
      : gpuComplete
        ? "complete"
        : "incomplete",
    gpu_skip_reason: gpuSkipped ? gpu[0].reason : null,
  };
}
export function markdown(report) {
  const result = report.summary;
  return [
    "# fframes + Skia vs Remotion",
    "",
    "100,000 elements: 99,000 rectangles and 1,000 changing text digits. Remotion uses an unkeyed list with 12 dependent effect/state updates per element. All render the same 1000×1000 scene serially, without a video encoder.",
    "",
    "Median of 3 rounds, each with 3 warm-up and 30 measured frames. PNG compression is included; startup, warm-up and disk writes are excluded.",
    "",
    "| Renderer | Median for 30 frames | Speedup vs Remotion |",
    "|---|---:|---:|",
    `| fframes + Skia CPU | ${result.fframes_median_ms?.toFixed(2) ?? "incomplete"} ms | ${result.speedup?.toFixed(2) ?? "n/a"}× |`,
    `| fframes + Skia GPU | ${result.gpu_status === "skipped" ? `skipped: ${result.gpu_skip_reason}` : `${result.gpu_median_ms?.toFixed(2) ?? "incomplete"} ms`} | ${result.gpu_speedup?.toFixed(2) ?? "n/a"}× |`,
    `| Remotion | ${result.remotion_median_ms?.toFixed(2) ?? "incomplete"} ms | — |`,
    "",
  ].join("\n");
}
