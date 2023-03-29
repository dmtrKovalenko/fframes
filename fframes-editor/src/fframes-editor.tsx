import * as React from "react";
import { createRoot } from "react-dom/client";
import { Editor } from "./ui/Editor.gen";
import { EditorContext } from "./EditorContext.gen";
import type { WasmController } from "./WasmController.gen";
import { processImports } from "./services/mediaLoader.gen";
import "../fonts/fonts.css";
import "../tw.css";

type Imports = Parameters<typeof processImports>[0]["imports"];
let lastImports: Imports | null = null;

export function renderEditor(imports: Imports, wasmController: WasmController) {
  lastImports = imports;
  const root = createRoot(document.getElementById("root")!);

  wasmController.default().then(() => {
    Promise.all([
      processImports({ imports, wasmController }),
      wasmController.prepare().then((videoMeta) => {
        root.render(
          <EditorContext wasmController={wasmController} videoMeta={videoMeta}>
            <Editor />
          </EditorContext>
        );
      }),
    ]);
  });

  document.addEventListener(
    "focus",
    (event) => {
      if (event.target instanceof HTMLElement) {
        const target = event.target;
        // if in 2 seconds focus still on the target element blur it to prevent stealing keystrokes from
        // the editor
        setTimeout(() => {
          if (document.activeElement === target) {
            target.blur();
          }
        }, 2000);
      }
    },
    true
  );
}

export async function load_audio_wasm_callback(name: string) {
  if (!lastImports) {
    throw new Error(
      `Can not process audio duration callback for ${name} imports glob not provided.`
    );
  }

  const importPath = Object.keys(lastImports).find((key) =>
    lastImports?.[key].endsWith(name)
  );
  if (!importPath) {
    throw new Error(
      `Can not process file ${name}. Did you forget to include it in media folder?`
    );
  }

  const response = await fetch(lastImports[importPath]);

  if (!response.ok) {
    throw new Error(
      `Can not load ${name}. Error: ${response.status}(${response.statusText})`
    );
  }

  const context = new AudioContext();

  try {
    const audioBuffer = await context.decodeAudioData(
      await response.arrayBuffer()
    );

    return audioBuffer.duration;
  } catch (e) {
    throw new Error(
      `Can process audio file "${name}". Maybe it's not an audio? ${e}`
    );
  }
}
