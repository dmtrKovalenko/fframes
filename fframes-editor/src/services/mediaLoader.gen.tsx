/* TypeScript file generated from mediaLoader.res by genType. */
/* eslint-disable import/first */


// @ts-ignore: Implicit any on import
import * as Curry__Es6Import from 'rescript/lib/es6/curry.js';
const Curry: any = Curry__Es6Import;

// @ts-ignore: Implicit any on import
import * as mediaLoaderBS__Es6Import from './mediaLoader.bs';
const mediaLoaderBS: any = mediaLoaderBS__Es6Import;

import type {AudioBuffer_t as WebAudio_AudioBuffer_t} from '../../src/bindings/WebAudio.gen';

import type {Js_ArrayBuffer_t as ReScriptJs_Js_ArrayBuffer_t} from './shims/Js.shim';

import type {Js_Dict_t as ReScriptJs_Js_Dict_t} from './shims/Js.shim';

import type {Js_Int16Array_t as ReScriptJs_Js_Int16Array_t} from './shims/Js.shim';

import type {Js_Promise_t as ReScriptJs_Js_Promise_t} from './shims/Js.shim';

import type {options as WasmController_options} from '../../src/WasmController.gen';

import type {t as WasmController_t} from '../../src/WasmController.gen';

// tslint:disable-next-line:interface-over-type-literal
export type audioInfo = {
  readonly duration: number; 
  readonly sampleRate: number; 
  readonly arrayBuffer: ReScriptJs_Js_ArrayBuffer_t; 
  readonly audioData: WebAudio_AudioBuffer_t; 
  readonly monoPcmData: ReScriptJs_Js_Int16Array_t
};

// tslint:disable-next-line:interface-over-type-literal
export type imageInfo = {
  readonly src: string; 
  readonly width: number; 
  readonly height: number
};

// tslint:disable-next-line:interface-over-type-literal
export type fontInfo = {
  readonly name: string; 
  readonly style: string; 
  readonly weight: number; 
  readonly unicodeRange: string
};

// tslint:disable-next-line:interface-over-type-literal
export type processedMedia = 
    { tag: "Font"; value: fontInfo }
  | { tag: "Subtitles"; value: number }
  | { tag: "Image"; value: imageInfo }
  | { tag: "Audio"; value: audioInfo };

// tslint:disable-next-line:interface-over-type-literal
export type mediaImport = string;

// tslint:disable-next-line:interface-over-type-literal
export type forceTsReturnResolveMedia = "MediaResolved";

// tslint:disable-next-line:interface-over-type-literal
export type mediaResolveFn = (_1:string, _2:string, _3:WasmController_t) => ReScriptJs_Js_Promise_t<forceTsReturnResolveMedia>;
export type MediaResolver = mediaResolveFn;

// tslint:disable-next-line:interface-over-type-literal
export type mediaResolveFnWithOptions = (_1:WasmController_options, _2:string, _3:string, _4:WasmController_t) => ReScriptJs_Js_Promise_t<forceTsReturnResolveMedia>;
export type MediaResolverWithOptions = mediaResolveFnWithOptions;

export const resolveMedia: (name:string, media:processedMedia) => forceTsReturnResolveMedia = function (Arg1: any, Arg2: any) {
  const result = Curry._2(mediaLoaderBS.resolveMedia, Arg1, Arg2.tag==="Font"
    ? {TAG: 0, _0:Arg2.value} as any
    : Arg2.tag==="Subtitles"
    ? {TAG: 1, _0:Arg2.value} as any
    : Arg2.tag==="Image"
    ? {TAG: 2, _0:Arg2.value} as any
    : {TAG: 3, _0:Arg2.value} as any);
  return "MediaResolved"
};

export const processImports: (_1:{
  readonly imports: ReScriptJs_Js_Dict_t<mediaImport>; 
  readonly wasmController: WasmController_t; 
  readonly options: WasmController_options
}) => ReScriptJs_Js_Promise_t<void> = function (Arg1: any) {
  const result = Curry._3(mediaLoaderBS.processImports, Arg1.imports, Arg1.wasmController, Arg1.options);
  return result
};
