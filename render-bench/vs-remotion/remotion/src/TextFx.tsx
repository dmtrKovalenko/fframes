// Workload "TextFx": 3,334 text nodes per frame (300 frames, 1,000,200 in total), 18 to
// 96 px, each with seeded "random" animated effects: rotation / scale / skew, opacity, HSL
// colour cycling, gradient fills (background-clip: text), outlined text
// (-webkit-text-stroke), drop shadows (text-shadow), glow on every 10th node (text-shadow),
// letter-spacing and per-letter wave words. The per-node math is a line-by-line twin of
// ../fframes/src/bin/textfx.rs.
import React from 'react';
import {AbsoluteFill, staticFile, useCurrentFrame} from 'remotion';
import {loadFont} from '@remotion/fonts';

// HSL (degrees, 0..1, 0..1) to #rrggbb, the same math as hsl_to_hex in textfx.rs.
const hslToHex = (h: number, s: number, l: number): string => {
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
  const q = (v: number) => Math.round((v + m) * 255).toString(16).padStart(2, '0');
  return `#${q(r)}${q(g)}${q(b)}`;
};

loadFont({family: 'DM Sans', url: staticFile('DMSans-Regular.ttf'), weight: '400'});

const NODES = 3334;
const WORDS = [
  'fframes', 'Rust', 'Skia', 'GPU', 'render', 'SVG', 'frame', 'video', 'motion', 'text',
  '1,000,000', '60 fps', 'pixels', 'shader', 'glyph', 'kerning', 'encode', 'H.264', 'effects', 'wave',
];
const WAVE_WORDS = ['fframes', 'WAVE', 'rendering', 'motion'];
const GRADIENTS = 6;
const SHADOW_COLORS = ['#ff2d95', '#00e5ff', '#ffd400', '#7c4dff'];
// DM Sans: ascent 0.992em, descent 0.31em. With line-height 1 the baseline sits 0.841em
// below the top of the span; the SVG side puts the baseline at the node origin.
const BASELINE = 0.841;

// lowbias32 integer hash; `hash32` in textfx.rs is the same function.
const hash32 = (x: number): number => {
  x ^= x >>> 16;
  x = Math.imul(x, 0x7feb352d);
  x ^= x >>> 15;
  x = Math.imul(x, 0x846ca68b);
  x ^= x >>> 16;
  return x >>> 0;
};
const rnd = (i: number, k: number) => hash32(i * 16 + k + 12345) / 4294967296;

type Kind = 'solid' | 'gradient' | 'stroke' | 'shadow' | 'spacing' | 'glow' | 'wave';
const KINDS: Kind[] = ['solid', 'gradient', 'stroke', 'shadow', 'spacing'];

// `only`: debug aid for the visual check (`--props='{"only":"stroke"}'` draws only the
// stroke nodes); unset in every timed run.
export const TextFx: React.FC<{only?: Kind}> = ({only}) => {
  const f = useCurrentFrame();

  // Per-frame animated "shared" styles (the <defs> of the SVG side).
  const gradients: string[] = [];
  for (let v = 0; v < GRADIENTS; v++) {
    const base = v * 60 + f * 4;
    const c0 = hslToHex(base % 360, 0.9, 0.6);
    const c1 = hslToHex((base + 120) % 360, 0.9, 0.6);
    const c2 = hslToHex((base + 240) % 360, 0.9, 0.6);
    gradients.push(`linear-gradient(90deg, ${c0} 0%, ${c1} 50%, ${c2} 100%)`);
  }
  const shadows = SHADOW_COLORS.map((color, v) => {
    const a = f * 0.08 + v * 1.5708;
    // CSS blur radius = 2 x the Gaussian standard deviation (stdDeviation 3 on the SVG side)
    return `${(5 * Math.cos(a)).toFixed(3)}px ${(5 * Math.sin(a)).toFixed(3)}px 6px ${color}`;
  });
  const glowSigma = 3 + 2 * (0.5 + 0.5 * Math.sin(f * 0.1));

  const nodes: React.ReactNode[] = new Array(NODES);
  for (let i = 0; i < NODES; i++) {
    const r = (k: number) => rnd(i, k);
    const kind: Kind = i % 10 === 0 ? 'glow' : i % 97 === 5 ? 'wave' : KINDS[Math.floor(r(11) * 5)];
    if (only && kind !== only) continue;
    let size = Math.round(18 + 78 * Math.pow(r(0), 1.6));
    if (kind === 'glow' || kind === 'shadow') size = Math.max(size, 36);
    if (kind === 'wave') size = Math.round(56 + 40 * r(0));
    const phase = r(3) * Math.PI * 2;
    const speed = 0.5 + r(4);
    const x = -60 + r(1) * 1900 + 40 * Math.sin(f * 0.03 * speed + phase);
    const y = 40 + r(2) * 1060 + 30 * Math.cos(f * 0.025 * speed + phase * 1.3);
    const rot = r(6) < 0.15
      ? f * 3 * speed + r(5) * 360
      : (r(5) - 0.5) * 60 + 25 * Math.sin(f * 0.04 * speed + phase);
    const scale = 0.8 + 0.35 * Math.sin(f * 0.06 * speed + phase * 0.7);
    const skew = r(7) < 0.4 ? 20 * Math.sin(f * 0.05 + phase) : 0;
    const opacity = 0.35 + 0.65 * (0.5 + 0.5 * Math.sin(f * 0.07 * speed + phase * 2));
    const hue = (r(8) * 360 + f * 3 * speed) % 360;
    const color = hslToHex(hue, 0.85, 0.62);
    const label = kind === 'wave'
      ? WAVE_WORDS[Math.floor(r(10) * WAVE_WORDS.length)]
      : r(9) < 0.2
        ? String((f * 37 + i * 101) % 100000)
        : WORDS[Math.floor(r(10) * WORDS.length)];

    const style: React.CSSProperties = {
      position: 'absolute',
      left: 0,
      top: 0,
      transformOrigin: '0 0',
      transform: `translate(${x.toFixed(3)}px, ${y.toFixed(3)}px) rotate(${rot.toFixed(3)}deg) skewX(${skew.toFixed(3)}deg) scale(${scale.toFixed(4)}) translateY(${-BASELINE * size}px)`,
      opacity: Number(opacity.toFixed(3)),
      fontSize: size,
      color,
    };
    let children: React.ReactNode = label;
    switch (kind) {
      case 'gradient':
        style.backgroundImage = gradients[Math.floor(r(12) * GRADIENTS)];
        style.WebkitBackgroundClip = 'text';
        style.backgroundClip = 'text';
        style.color = 'transparent';
        break;
      case 'stroke':
        style.color = 'transparent';
        style.WebkitTextStroke = `${Math.max(size / 28, 1.5).toFixed(3)}px ${color}`;
        break;
      case 'shadow':
        style.textShadow = shadows[Math.floor(r(13) * 4)];
        break;
      case 'spacing':
        style.letterSpacing = `${(size * 0.25 * (0.5 + 0.5 * Math.sin(f * 0.1 + phase))).toFixed(3)}px`;
        break;
      case 'glow': {
        const g = `0 0 ${(2 * glowSigma).toFixed(3)}px ${color}`;
        style.textShadow = `${g}, ${g}`;
        break;
      }
      case 'wave':
        style.fontKerning = 'none';
        children = label.split('').map((ch, j) => (
          <span
            key={j}
            style={{display: 'inline-block', transform: `translateY(${(0.25 * size * Math.sin(f * 0.2 + j * 0.6)).toFixed(3)}px)`}}
          >
            {ch}
          </span>
        ));
        break;
    }
    nodes[i] = (
      <span key={i} style={style}>
        {children}
      </span>
    );
  }
  return (
    <AbsoluteFill
      style={{backgroundColor: '#0b1020', fontFamily: 'DM Sans', fontWeight: 400, lineHeight: 1, whiteSpace: 'nowrap'}}
    >
      {nodes}
    </AbsoluteFill>
  );
};
