import initWasm, { create_wasm_bridge } from "./editor-bridge/pkg";
import { renderEditor } from "@fframes/editor";
import "@fframes/editor/dist/fframes-editor.css";

await initWasm();
const wasmBridge = create_wasm_bridge();

renderEditor(wasmBridge, {
  dynamicMediaFolder: import.meta.glob("../media/*", {
    query: "url",
    import: "default",
    eager: true,
  }),
});
