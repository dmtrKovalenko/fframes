/* TypeScript file generated from WasmController.res by genType. */
/* eslint-disable import/first */


import type {Js_BigInt_t as ReScriptJs_Js_BigInt_t} from './shims/Js.shim';

import type {Js_Int16Array_t as ReScriptJs_Js_Int16Array_t} from './shims/Js.shim';

import type {Js_Nullable_t as ReScriptJs_Js_Nullable_t} from './shims/Js.shim';

import type {Js_Promise_t as ReScriptJs_Js_Promise_t} from './shims/Js.shim';

import type {Js_RegExp_t as ReScriptJs_Js_RegExp_t} from './shims/Js.shim';

import type {Js_Uint8Array_t as ReScriptJs_Js_Uint8Array_t} from './shims/Js.shim';

import type {Js_Undefined_t as ReScriptJs_Js_Undefined_t} from './shims/Js.shim';

// tslint:disable-next-line:interface-over-type-literal
export type namedRange = {
  readonly name: string; 
  readonly start: number; 
  readonly end: number
};

// tslint:disable-next-line:interface-over-type-literal
export type options = {
  readonly ignoreMediaRegex?: ReScriptJs_Js_RegExp_t; 
  readonly hideDock: boolean; 
  readonly loop: boolean; 
  readonly lockFps?: number; 
  readonly mediaListLayout: 
    "list"
  | "fromAspectRatio"
  | "grid"; 
  readonly rewindStepInSeconds: number; 
  readonly imageLengthLimit: number; 
  readonly volumeStepFrom0To100: number
};
export type EditorOptions = options;

// tslint:disable-next-line:interface-over-type-literal
export type videoMeta = {
  readonly name: string; 
  readonly width: number; 
  readonly height: number; 
  readonly fps: number; 
  readonly durationInFrames: number; 
  readonly hasAudio: boolean; 
  readonly originalFps?: number; 
  readonly audioMap: ReScriptJs_Js_Nullable_t<namedRange[]>; 
  readonly scenesTimeline: ReScriptJs_Js_Nullable_t<namedRange[]>
};
export type VideoMeta = videoMeta;

// tslint:disable-next-line:interface-over-type-literal
export type fontInfo = {
  readonly name: ReScriptJs_Js_Nullable_t<ReScriptJs_Js_Uint8Array_t>; 
  readonly weight: number; 
  readonly style: string
};

// tslint:disable-next-line:interface-over-type-literal
export type initOut = { readonly prepare: (_1:number, _2:number) => number };

// tslint:disable-next-line:interface-over-type-literal
export type t = {
  readonly add_audio_source: (_1:string, _2:ReScriptJs_Js_Int16Array_t) => void; 
  readonly add_image_source: (_1:string, _2:string, _3:ReScriptJs_Js_Undefined_t<string>) => void; 
  readonly add_subtitles_source: (_1:string, _2:string) => number; 
  readonly default: () => ReScriptJs_Js_Promise_t<initOut>; 
  readonly prepare: (_1:ReScriptJs_Js_Undefined_t<number>) => ReScriptJs_Js_Promise_t<videoMeta>; 
  readonly render_frame: (_1:ReScriptJs_Js_BigInt_t) => string; 
  readonly render_preview_frame: (_1:ReScriptJs_Js_BigInt_t) => string; 
  readonly ingest_font: (_1:ReScriptJs_Js_Uint8Array_t) => fontInfo
};
export type WasmController = t;
