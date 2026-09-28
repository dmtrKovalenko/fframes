// Workload "podcast": two real video feeds with rounded corners and drop shadows, an
// animated blurred gradient background, word-by-word captions, a progress bar and a
// spring-animated title card. Written the way the Remotion docs suggest: <OffthreadVideo>,
// CSS filter: blur(), box-shadow, gradients, flexbox for the caption layout, spring().
// The fframes twin is ../fframes/src/bin/podcast.rs.
import React from 'react';
import {AbsoluteFill, OffthreadVideo, spring, staticFile, useCurrentFrame, useVideoConfig} from 'remotion';
import {Video as MediaVideo} from '@remotion/media';
import {
  CAPTION_START, CAPTION_WORDS, WORDS_PER_LINE, WORD_SECONDS, clamp01, easeOutCubic,
} from './shared';

const BLOBS = [
  {color: '#7c3aed', r: 360, x: -520, y: -220},
  {color: '#db2777', r: 300, x: 480, y: 200},
  {color: '#0ea5e9', r: 340, x: -200, y: 260},
  {color: '#f59e0b', r: 260, x: 380, y: -260},
];

const CARDS = [
  {src: 'left30.mp4', x: 80, label: 'Host'},
  {src: 'right30.mp4', x: 980, label: 'Guest'},
];
const CARD_Y = 200, CARD_W = 860, CARD_H = 484;

const fmt = (s: number) => `${String(Math.floor(s / 60)).padStart(2, '0')}:${String(Math.floor(s % 60)).padStart(2, '0')}`;

export type PodcastProps = {media?: 'offthread' | 'mediabunny'};

export const Podcast: React.FC<PodcastProps> = ({media = 'offthread'}) => {
  const frame = useCurrentFrame();
  const {fps, durationInFrames} = useVideoConfig();
  const t = frame / fps;

  const title = spring({frame, fps, config: {mass: 1, stiffness: 120, damping: 14}});

  const kCur = Math.min(CAPTION_WORDS.length - 1, Math.max(0, Math.floor((t - CAPTION_START) / WORD_SECONDS)));
  const line = Math.floor(kCur / WORDS_PER_LINE);
  const lineWords = CAPTION_WORDS.slice(line * WORDS_PER_LINE, line * WORDS_PER_LINE + WORDS_PER_LINE);

  return (
    <AbsoluteFill style={{backgroundColor: '#0b0a16', fontFamily: 'DM Sans', fontWeight: 500, color: 'white'}}>
      {/* animated blurred gradient background */}
      <AbsoluteFill style={{filter: 'blur(90px)'}}>
        {BLOBS.map((b, i) => {
          const cx = 960 + b.x + 220 * Math.sin(t * 0.35 + i * 1.7);
          const cy = 540 + b.y + 160 * Math.cos(t * 0.28 + i * 2.3);
          return (
            <div key={i} style={{
              position: 'absolute', left: cx - b.r, top: cy - b.r, width: 2 * b.r, height: 2 * b.r,
              borderRadius: '50%', backgroundColor: b.color, opacity: 0.75,
            }} />
          );
        })}
      </AbsoluteFill>

      {/* title card */}
      <div style={{
        position: 'absolute', left: 80, top: 64, width: 620, height: 84, borderRadius: 42,
        background: 'linear-gradient(90deg, #7c3aed, #db2777)', boxShadow: '0 20px 40px rgba(0,0,0,0.35)',
        fontSize: 36, lineHeight: '84px', paddingLeft: 36, boxSizing: 'border-box', whiteSpace: 'nowrap',
        opacity: clamp01(title), transform: `translateY(${(1 - title) * -60}px) scale(${0.8 + 0.2 * title})`,
      }}>
        The Render Loop · Episode 42
      </div>

      {/* video feeds */}
      {CARDS.map((c, i) => {
        const p = spring({frame: frame - 8 - i * 6, fps, config: {mass: 1, stiffness: 100, damping: 15}});
        const style: React.CSSProperties = {width: '100%', height: '100%', objectFit: 'cover'};
        return (
          <div key={c.src} style={{
            position: 'absolute', left: c.x, top: CARD_Y, width: CARD_W, height: CARD_H, borderRadius: 28,
            overflow: 'hidden', boxShadow: '0 30px 60px rgba(0,0,0,0.5)',
            opacity: clamp01(p), transform: `translateY(${(1 - p) * 80}px)`,
          }}>
            {media === 'mediabunny'
              ? <MediaVideo src={staticFile(c.src)} muted objectFit="cover" style={{width: '100%', height: '100%'}} />
              : <OffthreadVideo src={staticFile(c.src)} muted style={style} />}
            <div style={{
              position: 'absolute', left: 20, top: CARD_H - 64, width: 150, height: 44, borderRadius: 22,
              backgroundColor: 'rgba(0,0,0,0.55)', fontSize: 22, lineHeight: '44px', paddingLeft: 22,
              boxSizing: 'border-box',
            }}>
              {c.label}
            </div>
          </div>
        );
      })}

      {/* word-by-word captions */}
      <div style={{position: 'absolute', left: 0, right: 0, top: 760, display: 'flex', justifyContent: 'center'}}>
        <div style={{
          display: 'flex', gap: 16, padding: '0 30px', height: 90, borderRadius: 20,
          backgroundColor: 'rgba(0,0,0,0.45)', fontSize: 54, lineHeight: '90px', whiteSpace: 'nowrap',
        }}>
          {lineWords.map((w, j) => {
            const k = line * WORDS_PER_LINE + j;
            const start = CAPTION_START + k * WORD_SECONDS;
            const a = t < start ? 0 : easeOutCubic((t - start) / 0.15);
            const active = k === kCur && t >= start;
            return (
              <span key={k} style={{
                display: 'inline-block', opacity: a, color: active ? '#facc15' : 'white',
                transform: `translateY(${(1 - a) * 16}px) scale(${active ? 1.1 : 1})`,
              }}>
                {w}
              </span>
            );
          })}
        </div>
      </div>

      {/* progress bar and time */}
      <div style={{
        position: 'absolute', right: 80, top: 950, fontSize: 26, lineHeight: '40px', color: 'rgba(255,255,255,0.8)',
      }}>
        {fmt(t)} / {fmt(durationInFrames / fps)}
      </div>
      <div style={{position: 'absolute', left: 80, top: 1000, width: 1760, height: 10, borderRadius: 5, backgroundColor: 'rgba(255,255,255,0.15)'}} />
      <div style={{
        position: 'absolute', left: 80, top: 1000, height: 10, borderRadius: 5,
        width: Math.max(10, (1760 * frame) / (durationInFrames - 1)),
        background: 'linear-gradient(90deg, #7c3aed, #db2777)',
      }} />
    </AbsoluteFill>
  );
};
