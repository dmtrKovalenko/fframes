// Helpers shared by the realistic workloads. Every function here has a line-by-line
// twin in ../fframes/src/shared.rs so both tools draw the same frames.
import {loadFont} from '@remotion/fonts';
import {staticFile} from 'remotion';

loadFont({family: 'DM Sans', url: staticFile('DMSans-Medium.ttf'), weight: '500'});
loadFont({family: 'Bebas Neue', url: staticFile('BebasNeue-Regular.ttf'), weight: '400'});

export const clamp01 = (v: number) => Math.min(1, Math.max(0, v));
export const easeOutCubic = (v: number) => 1 - Math.pow(1 - clamp01(v), 3);

export const hslToHex = (h: number, s: number, l: number): string => {
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

// Captions: word k starts at CAPTION_START + k * WORD_SECONDS, WORDS_PER_LINE words a line.
export const CAPTION_WORDS = (
  'so the thing nobody tells you about rendering video in code is that the browser ' +
  'was never built for it every frame goes through layout paint and a screenshot ' +
  'and then it has to be encoded which is fine for a demo but once you have real ' +
  'footage blurred backgrounds and captions on screen the costs add up fast and you ' +
  'start waiting on renders instead of shipping videos'
).split(' ');
export const WORD_SECONDS = 0.3;
export const CAPTION_START = 0.4;
export const WORDS_PER_LINE = 6;

// Motion graphics: 40 shapes.
export const SHAPES = 40;
// 5-point star, outer radius 1, inner radius 0.45, centered on 0,0.
export const STAR_PATH = (() => {
  const pts: string[] = [];
  for (let k = 0; k < 10; k++) {
    const r = k % 2 === 0 ? 1 : 0.45;
    const a = -Math.PI / 2 + (k * Math.PI) / 5;
    pts.push(`${(r * Math.cos(a)).toFixed(4)} ${(r * Math.sin(a)).toFixed(4)}`);
  }
  return `M ${pts.join(' L ')} Z`;
})();
export const TRIANGLE_PATH = 'M 0 -1 L 0.866 0.5 L -0.866 0.5 Z';
