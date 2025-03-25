import React from "react";
import initWasm, {
  create_wasm_bridge,
} from "./editor-bridge/pkg/pixel_bridge_example";
import { renderEditor } from "@fframes/editor";
import "@fframes/editor/dist/fframes-editor.css";

await initWasm();

renderEditor(create_wasm_bridge(), {
  dynamicMediaFolder: import.meta.glob("../photos/*", {
    query: "url",
    import: "default",
    eager: true,
  }),
});
