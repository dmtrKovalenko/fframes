// 3,334 absolutely positioned text nodes per frame. Every value depends on the frame
// index, so nothing can be reused between frames. The math is a line-by-line copy of
// ../fframes/src/lib.rs.
import React from 'react';
import {AbsoluteFill, staticFile, useCurrentFrame} from 'remotion';
import {loadFont} from '@remotion/fonts';

loadFont({family: 'DM Sans', url: staticFile('DMSans-Regular.ttf'), weight: '400'});

export const NODES = 3334;
const COLS = 47;
const ROWS = 71;
const CELL_W = 1920 / COLS;
const CELL_H = 1080 / ROWS;
const FONT_SIZE = 10;
// DM Sans: ascent 0.992em, descent 0.31em. With line-height = 1em the baseline sits
// 0.841em below the top of the box; SVG <text y> is the baseline.
const BASELINE = 0.841 * FONT_SIZE;

const hslToRgb = (h: number, s: number, l: number): [number, number, number] => {
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const hp = h / 60;
  const x = c * (1 - Math.abs((hp % 2) - 1));
  let r = 0, g = 0, b = 0;
  if (hp < 1) [r, g, b] = [c, x, 0];
  else if (hp < 2) [r, g, b] = [x, c, 0];
  else if (hp < 3) [r, g, b] = [0, c, x];
  else if (hp < 4) [r, g, b] = [0, x, c];
  else if (hp < 5) [r, g, b] = [x, 0, c];
  else [r, g, b] = [c, 0, x];
  const m = l - c / 2;
  return [Math.round((r + m) * 255), Math.round((g + m) * 255), Math.round((b + m) * 255)];
};

export const TextGrid: React.FC = () => {
  const f = useCurrentFrame();
  const nodes: React.ReactNode[] = new Array(NODES);
  for (let i = 0; i < NODES; i++) {
    const col = i % COLS;
    const row = Math.floor(i / COLS);
    const x = col * CELL_W + 2 + 4 * Math.sin(f * 0.12 + i * 0.37);
    const y = row * CELL_H + 11 + 3 * Math.cos(f * 0.09 + i * 0.23);
    const alpha = 0.3 + 0.7 * (0.5 + 0.5 * Math.sin(f * 0.2 + i * 0.05));
    const [r, g, b] = hslToRgb((i * 0.9 + f * 4) % 360, 0.75, 0.62);
    const label = (i + f) % 9 === 0 ? 'fframes' : String((f * 37 + i * 101) % 10000);
    nodes[i] = (
      <span
        key={i}
        style={{
          position: 'absolute',
          left: 0,
          top: 0,
          transform: `translate(${x}px, ${y - BASELINE}px)`,
          color: `rgba(${r},${g},${b},${alpha.toFixed(3)})`,
        }}
      >
        {label}
      </span>
    );
  }
  return (
    <AbsoluteFill
      style={{
        backgroundColor: '#0b1020',
        fontFamily: 'DM Sans',
        fontSize: FONT_SIZE,
        lineHeight: 1,
        whiteSpace: 'nowrap',
      }}
    >
      {nodes}
    </AbsoluteFill>
  );
};
