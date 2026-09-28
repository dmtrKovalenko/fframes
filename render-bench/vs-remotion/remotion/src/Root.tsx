import React from 'react';
import {Composition} from 'remotion';
import {TextGrid} from './TextGrid';
import {TextFx} from './TextFx';

export const RemotionRoot: React.FC = () => (
  <>
    <Composition id="TextGrid" component={TextGrid} durationInFrames={300} fps={30} width={1920} height={1080} />
    {/* 3,334 large text nodes per frame with seeded animated effects (see README, "TextFx") */}
    <Composition id="TextFx" component={TextFx} durationInFrames={300} fps={30} width={1920} height={1080} />
  </>
);
