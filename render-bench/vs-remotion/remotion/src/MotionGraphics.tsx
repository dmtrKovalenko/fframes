// Workload "motion-graphics": a promo-style scene at 60 fps. Blurred photo backdrop with a
// gradient overlay, 40 animated SVG shapes, three layered cards with box-shadows and
// blurred glows (one holds an <Img>), and kinetic per-letter headline text with a glow.
// The fframes twin is ../fframes/src/bin/motion.rs.
import React from 'react';
import {AbsoluteFill, Img, spring, staticFile, useCurrentFrame, useVideoConfig} from 'remotion';
import {SHAPES, STAR_PATH, TRIANGLE_PATH, clamp01, easeOutCubic, hslToHex} from './shared';

const CARDS = [
  {x: 1060, y: 140, w: 700, h: 420, glow: '#a855f7'},
  {x: 160, y: 590, w: 560, h: 300, glow: '#22d3ee'},
  {x: 820, y: 660, w: 520, h: 240, glow: '#f472b6'},
];
const HEADLINE = [
  {top: 110, words: ['MOTION']},
  {top: 290, words: ['IN', 'CODE']},
];

const CardContent: React.FC<{i: number}> = ({i}) => {
  if (i === 0) {
    return <Img src={staticFile('poster.jpg')} style={{width: '100%', height: '100%', objectFit: 'cover'}} />;
  }
  if (i === 1) {
    return (
      <AbsoluteFill style={{background: 'linear-gradient(to bottom right, #4f46e5, #9333ea)', padding: '0 40px'}}>
        <div style={{fontFamily: 'Bebas Neue', fontWeight: 400, fontSize: 120, lineHeight: '120px', marginTop: 50}}>60 FPS</div>
        <div style={{fontSize: 30, lineHeight: '40px', marginTop: 10}}>rendered on the GPU</div>
      </AbsoluteFill>
    );
  }
  return (
    <AbsoluteFill style={{
      backgroundColor: 'rgba(255,255,255,0.12)', border: '1.5px solid rgba(255,255,255,0.35)',
      borderRadius: 32, padding: '0 40px',
    }}>
      <div style={{fontSize: 40, lineHeight: '50px', marginTop: 70}}>Layered cards</div>
      <div style={{fontSize: 26, lineHeight: '36px', marginTop: 12, color: 'rgba(255,255,255,0.7)'}}>
        shadows, glows, gradients
      </div>
    </AbsoluteFill>
  );
};

export const MotionGraphics: React.FC = () => {
  const frame = useCurrentFrame();
  const {fps} = useVideoConfig();
  const t = frame / fps;
  let letterIndex = 0;
  const sub = easeOutCubic((t - 1.0) / 0.6);

  return (
    <AbsoluteFill style={{backgroundColor: '#0b0a16', fontFamily: 'DM Sans', fontWeight: 500, color: 'white'}}>
      {/* blurred photo backdrop + gradient overlay */}
      <Img src={staticFile('poster.jpg')} style={{
        position: 'absolute', left: 0, top: 0, width: 1920, height: 1080,
        filter: 'blur(30px)', transform: `scale(${1.1 + 0.01 * t})`,
      }} />
      <AbsoluteFill style={{background: 'linear-gradient(to bottom right, rgba(15,10,40,0.85), rgba(60,20,90,0.6))'}} />

      {/* 40 animated shapes */}
      <svg width={1920} height={1080} style={{position: 'absolute', left: 0, top: 0}}>
        {Array.from({length: SHAPES}, (_, i) => {
          const s = 24 + ((i * 37) % 50);
          const x = ((i * 263) % 1920) + 60 * Math.sin(0.8 * t + i);
          const y = ((i * 149) % 1080) + 50 * Math.cos(0.6 * t + 1.3 * i);
          const rot = t * 40 * (i % 2 ? 1 : -1) + i * 20;
          const fill = hslToHex((i * 9 + t * 30) % 360, 0.8, 0.6);
          const transform = `translate(${x} ${y}) rotate(${rot}) scale(${s / 2})`;
          const kind = i % 4;
          return kind === 0 ? <circle key={i} r={1} fill={fill} opacity={0.7} transform={transform} />
            : kind === 1 ? <rect key={i} x={-1} y={-1} width={2} height={2} rx={0.5} fill={fill} opacity={0.7} transform={transform} />
            : <path key={i} d={kind === 2 ? TRIANGLE_PATH : STAR_PATH} fill={fill} opacity={0.7} transform={transform} />;
        })}
      </svg>

      {/* glows and layered cards */}
      {CARDS.map((c, i) => {
        const p = spring({frame: frame - (0.4 + 0.25 * i) * fps, fps, config: {mass: 1, stiffness: 90, damping: 14}});
        const dy = (1 - p) * 120 + 12 * Math.sin(1.3 * t + 2 * i);
        return (
          <React.Fragment key={i}>
            <div style={{
              position: 'absolute', left: c.x - 20, top: c.y - 20, width: c.w + 40, height: c.h + 40, borderRadius: 48,
              backgroundColor: c.glow, filter: 'blur(50px)',
              opacity: (0.55 + 0.25 * Math.sin(2 * t + i)) * clamp01(p), transform: `translateY(${dy}px)`,
            }} />
            <div style={{
              position: 'absolute', left: c.x, top: c.y, width: c.w, height: c.h, borderRadius: 32, overflow: 'hidden',
              boxShadow: '0 40px 80px rgba(0,0,0,0.45)', opacity: clamp01(p), transform: `translateY(${dy}px)`,
            }}>
              <CardContent i={i} />
            </div>
          </React.Fragment>
        );
      })}

      {/* kinetic headline */}
      {HEADLINE.map((line) => (
        <div key={line.top} style={{
          position: 'absolute', left: 160, top: line.top, display: 'flex', gap: 40,
          fontFamily: 'Bebas Neue', fontWeight: 400, fontSize: 180, lineHeight: '180px',
          textShadow: '0 0 30px rgba(168,85,247,0.8)',
        }}>
          {line.words.map((word) => (
            <div key={word} style={{display: 'flex'}}>
              {word.split('').map((ch, ci) => {
                const j = letterIndex++;
                const p = spring({frame: frame - (0.15 + j * 0.05) * fps, fps, config: {mass: 1, stiffness: 140, damping: 12}});
                const wave = 6 * Math.sin(3 * t + 0.5 * j);
                return (
                  <span key={ci} style={{
                    display: 'inline-block', opacity: clamp01(p * 1.5), transformOrigin: 'center bottom',
                    transform: `translateY(${(1 - p) * 100 + wave}px) rotate(${(1 - p) * -25}deg)`,
                  }}>
                    {ch}
                  </span>
                );
              })}
            </div>
          ))}
        </div>
      ))}
      <div style={{
        position: 'absolute', left: 164, top: 480, fontSize: 36, lineHeight: '50px', color: 'rgba(255,255,255,0.85)',
        opacity: sub, transform: `translateX(${(1 - sub) * -40}px)`,
      }}>
        fframes · Rust · Skia GPU
      </div>
    </AbsoluteFill>
  );
};
