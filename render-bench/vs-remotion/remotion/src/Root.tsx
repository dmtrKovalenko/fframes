import React from 'react';
import {Composition} from 'remotion';
import {TextGrid} from './TextGrid';
import {TextFx} from './TextFx';
import {Podcast, PodcastProps} from './Podcast';
import {MotionGraphics} from './MotionGraphics';

export const RemotionRoot: React.FC = () => (
  <>
    <Composition id="TextGrid" component={TextGrid} durationInFrames={300} fps={30} width={1920} height={1080} />
    {/* 3,334 large text nodes per frame with seeded animated effects (see README, "TextFx") */}
    <Composition id="TextFx" component={TextFx} durationInFrames={300} fps={30} width={1920} height={1080} />
    {/* realistic workloads (see README): 20 s podcast, 10 s motion graphics at 60 fps */}
    <Composition id="Podcast" component={Podcast} durationInFrames={600} fps={30} width={1920} height={1080}
      defaultProps={{media: 'offthread'} as PodcastProps} />
    {/* 4K: rendered with --scale=2 (3840x2160), first 5 s */}
    <Composition id="Podcast4K" component={Podcast} durationInFrames={150} fps={30} width={1920} height={1080}
      defaultProps={{media: 'offthread'} as PodcastProps} />
    <Composition id="MotionGraphics" component={MotionGraphics} durationInFrames={600} fps={60} width={1920} height={1080} />
  </>
);
