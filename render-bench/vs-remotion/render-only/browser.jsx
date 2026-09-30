import React, { useEffect, useLayoutEffect, useState } from "react";
import { createRoot } from "react-dom/client";

const root = createRoot(document.getElementById("root"));
let pending;
const color = (id, frame) =>
  `rgb(${(id * 13 + frame * 17) % 256},${(id * 7 + frame * 29) % 256},${(id * 3 + frame * 43) % 256})`;
const ready = frame => {
  if (pending?.frame === frame && --pending.remaining === 0) {
    pending.resolve();
    pending = undefined;
  }
};
function useDerivedFrame(input) {
  const [value, setValue] = useState(-1);
  useEffect(() => setValue(input), [input]);
  return value;
}
// Intentionally pathological derived state: eight dependent effect/commit passes.
// Each stage forwards the frame used for visible color, rather than doing a busy wait.
function EffectCell({ id, slot, frame }) {
  const a = useDerivedFrame(frame);
  const b = useDerivedFrame(a);
  const c = useDerivedFrame(b);
  const d = useDerivedFrame(c);
  const e = useDerivedFrame(d);
  const f = useDerivedFrame(e);
  const g = useDerivedFrame(f);
  const h = useDerivedFrame(g);
  useLayoutEffect(() => {
    if (h === frame) ready(frame);
  }, [h, frame]);
  return (
    <rect
      x={slot % 1000}
      y={Math.floor(slot / 1000)}
      width="1"
      height="1"
      fill={color(id, Math.max(0, h))}
    />
  );
}
function DirectGrid({ nodes, frame, mode }) {
  useLayoutEffect(() => {
    if (mode !== "unkeyed-effects") ready(frame);
  }, [frame, mode]);
  const cells = new Array(nodes);
  for (let slot = 0; slot < nodes; slot++) {
    const id = (slot + frame * 37) % nodes;
    if (mode === "unkeyed-effects") {
      // No keys by design. Production React avoids warning/log overhead.
      cells[slot] = <EffectCell id={id} slot={slot} frame={frame} />;
    } else {
      const props = {
        x: slot % 1000,
        y: Math.floor(slot / 1000),
        width: 1,
        height: 1,
        fill: color(id, frame),
      };
      cells[slot] =
        mode === "keyed-direct" ? (
          <rect key={id} {...props} />
        ) : (
          <rect {...props} />
        );
    }
  }
  return (
    <svg
      width="1000"
      height="1000"
      style={{ display: "block", background: "#000" }}
    >
      {cells}
    </svg>
  );
}
window.renderFrame = (nodes, frame, mode) =>
  new Promise(resolve => {
    if (pending) throw new Error("overlapping frame request");
    pending = {
      frame,
      remaining: mode === "unkeyed-effects" ? nodes : 1,
      resolve,
    };
    root.render(<DirectGrid nodes={nodes} frame={frame} mode={mode} />);
  });
