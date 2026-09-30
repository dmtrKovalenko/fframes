import React, { useEffect, useLayoutEffect, useMemo, useState } from "react";
import {
  Composition,
  registerRoot,
  staticFile,
  useCurrentFrame,
  useDelayRender,
} from "remotion";

const color = (id, frame) =>
  `rgb(${(id * 13 + frame * 17) % 256},${(id * 7 + frame * 29) % 256},${(id * 3 + frame * 43) % 256})`;
function useDerivedFrame(input) {
  const [value, setValue] = useState(-1);
  useEffect(() => setValue(input), [input]);
  return value;
}
// Intentionally pathological derived state: eight dependent effect/commit passes.
// Each stage forwards the frame used for visible color, rather than doing a busy wait.
function EffectCell({ id, slot, frame, ready }) {
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
  }, [h, frame, ready]);
  return cell(id, slot, Math.max(0, h));
}
function cell(id, slot, frame, key) {
  if (slot % 100 === 0) {
    const index = slot / 100;
    return (
      <text
        key={key}
        x={(index % 100) * 10 + 1}
        y={Math.floor(index / 100) * 10 + 7}
        fontFamily="Bench Digits"
        fontSize="10"
        fill={color(id, frame)}
      >
        {(id + frame) % 10}
      </text>
    );
  }
  return (
    <rect
      key={key}
      x={slot % 1000}
      y={Math.floor(slot / 1000)}
      width="1"
      height="1"
      fill={color(id, frame)}
    />
  );
}
function GridVideo({ nodes, mode }) {
  const frame = useCurrentFrame();
  const { delayRender, continueRender, cancelRender } = useDelayRender();
  const [fontHandle] = useState(() =>
    delayRender("benchmark font", { retries: 0 })
  );
  useEffect(() => {
    const font = new FontFace(
      "Bench Digits",
      `url(${staticFile("BenchDigits.ttf")})`
    );
    font
      .load()
      .then(loaded => {
        document.fonts.add(loaded);
        continueRender(fontHandle);
      })
      .catch(cancelRender);
  }, [fontHandle, continueRender, cancelRender]);
  const ready = useMemo(() => {
    const handle = delayRender(`frame ${frame} effects`, { retries: 0 });
    let remaining = mode === "unkeyed-effects" ? nodes : 1;
    return doneFrame => {
      if (doneFrame === frame && --remaining === 0) continueRender(handle);
    };
  }, [frame, nodes, mode, delayRender, continueRender]);
  useLayoutEffect(() => {
    if (mode !== "unkeyed-effects") ready(frame);
  }, [frame, mode, ready]);
  const cells = [];
  const append = slot => {
    const id = (slot + frame * 37) % nodes;
    cells.push(
      mode === "unkeyed-effects" ? (
        <EffectCell id={id} slot={slot} frame={frame} ready={ready} />
      ) : (
        cell(id, slot, frame, mode === "keyed-direct" ? id : undefined)
      )
    );
  };
  // Paint text last so every digit remains visible at all supported sizes.
  for (let slot = 0; slot < nodes; slot++) if (slot % 100 !== 0) append(slot);
  for (let slot = 0; slot < nodes; slot += 100) append(slot);
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
const Root = () => (
  <Composition
    id="MixedGrid"
    component={GridVideo}
    width={1000}
    height={1000}
    fps={30}
    durationInFrames={10000}
    defaultProps={{ nodes: 100000, mode: "keyed-direct" }}
  />
);
registerRoot(Root);
