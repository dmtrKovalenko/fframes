/* TypeScript file generated from Player.res by genType. */
/* eslint-disable import/first */


// tslint:disable-next-line:interface-over-type-literal
export type playState = 
    "Playing"
  | "Paused"
  | "WaitingForAction"
  | "CantPlay";

// tslint:disable-next-line:interface-over-type-literal
export type state = {
  readonly frame: number; 
  readonly startPlayingFrame: number; 
  readonly playState: playState; 
  readonly fpsLimit?: number; 
  readonly svg?: string; 
  readonly volume?: number; 
  readonly sceneIndex?: number
};

// tslint:disable-next-line:interface-over-type-literal
export type action = 
    "AllowPlay"
  | "Play"
  | "Pause"
  | { tag: "Seek"; value: number }
  | { tag: "NewFrame"; value: number }
  | { tag: "SetVolume"; value: number };
