import * as React from "react";
import { createRoot } from "react-dom/client";
import { Editor } from "../src/ui/Editor.gen";
import { EditorContext } from "../src/EditorContext.gen";
import type {
  EditorOptions,
  WasmController,
  mediaFolder,
} from "../src/WasmController.gen";
import { processMedia } from "../src/services/mediaLoader.gen";
import "../fonts/fonts.css";
import "../tw.css";

let lastImports: mediaFolder | undefined = undefined;

export function renderEditor(
  wasmController: WasmController,
  partialOptions: Partial<EditorOptions> = {}
) {
  lastImports = partialOptions.dynamicMediaFolder;
  const root = createRoot(document.getElementById("root")!);

  const options: EditorOptions = {
    hideDock: false,
    mediaListLayout: "fromAspectRatio",
    loop: false,
    rewindStepInSeconds: 2,
    dynamicImageSizeLimitBytes: 512 * 1024,
    volumeStepFrom0To100: 20,
    ...partialOptions,
  };

  Promise.all([
    processMedia({
      wasmController,
      options,
      dynamicImports: partialOptions.dynamicMediaFolder,
    }),
    wasmController.prepare(options.lockFps).then(videoMeta => {
      document.title = videoMeta.name.split("::").pop() ?? "fframes";
      root.render(
        <React.StrictMode>
          <EditorContext
            options={options}
            wasmController={wasmController}
            videoMeta={videoMeta}
          >
            <Editor />
          </EditorContext>
        </React.StrictMode>
      );
    }),
  ]);

  document.addEventListener(
    "focus",
    event => {
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

  const importPath = Object.keys(lastImports).find(key =>
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
