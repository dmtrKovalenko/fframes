/* TypeScript file generated from EditorContext.res by genType. */
/* eslint-disable import/first */

// @ts-ignore: Implicit any on import
import * as Curry__Es6Import from "rescript/lib/es6/curry.js";
const Curry: any = Curry__Es6Import;

// @ts-ignore: Implicit any on import
import * as EditorContextBS__Es6Import from "./EditorContext.bs";
const EditorContextBS: any = EditorContextBS__Es6Import;

import type { action as Player_action } from "./Player.gen";

import type { options as WasmController_options } from "./WasmController.gen";

import type { state as Player_state } from "./Player.gen";

import type { t as WasmController_t } from "./WasmController.gen";

import type { videoMeta as WasmController_videoMeta } from "./WasmController.gen";

// tslint:disable-next-line:interface-over-type-literal
export type editorContext = {
  readonly wasmController: WasmController_t;
  readonly videoMeta: WasmController_videoMeta;
  readonly options: WasmController_options;
  readonly usePlayer: () => [Player_state, (_1: Player_action) => void];
};

export const EditorContext: (_1: {
  readonly wasmController: WasmController_t;
  readonly videoMeta: WasmController_videoMeta;
  readonly options: WasmController_options;
  readonly children: JSX.Element;
}) => JSX.Element = function (Arg1: any) {
  const result = Curry._4(
    EditorContextBS.makeEditorContextComponent,
    Arg1.wasmController,
    Arg1.videoMeta,
    Arg1.options,
    Arg1.children
  );
  return result;
};
