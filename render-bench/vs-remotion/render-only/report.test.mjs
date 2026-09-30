import test from "node:test";
import assert from "node:assert/strict";
import { median, summarize } from "./report.mjs";
const plan = {
  nodes: [1000000],
  modes: ["unkeyed-effects"],
  rounds: 3,
  frames: 1,
  warmup: 1,
};
const run = (engine, round, ms) => ({
  nodes: 1000000,
  engine,
  round,
  status: "ok",
  verified: true,
  samples: [{ frame: 1, total_ms: ms }],
});
const records = [
  ...Array.from({ length: 3 }, (_, i) => run("fframes", i, 10)),
  ...Array.from({ length: 3 }, (_, i) => run("unkeyed-effects", i, 210)),
];
test("reports median measured ratio, not the best frame", () => {
  assert.equal(median([1, 3, 2, 100]), 2.5);
  assert.equal(summarize(records, plan)[0].speedup, 21);
});
test("does not convert failure, missing run or invalid pixels into speedup", () => {
  for (const bad of [
    records.slice(1),
    records.map((r, i) => (i ? r : { ...r, status: "timeout" })),
    records.map((r, i) => (i ? r : { ...r, verified: false })),
    records.map((r, i) => (i ? r : { ...r, samples: [{ total_ms: 0 }] })),
  ]) {
    assert.equal(summarize(bad, plan)[0].speedup, null);
    assert.equal(summarize(bad, plan)[0].reaches_20x, false);
  }
});
test("does not round a sub-20 result up to the target", () => {
  const data = records.map(r =>
    r.engine === "fframes"
      ? r
      : { ...r, samples: [{ frame: 1, total_ms: 199.99 }] }
  );
  assert.equal(summarize(data, plan)[0].reaches_20x, false);
});
