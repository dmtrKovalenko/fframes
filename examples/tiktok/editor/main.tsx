import initWasm, { create_wasm_bridge } from "./editor-bridge/pkg";
import { renderEditor } from "@fframes/editor";
import "@fframes/editor/dist/fframes-editor.css";

await initWasm();
const bridge = create_wasm_bridge();

renderEditor(bridge, {
  staticMediaFolder: import.meta.glob("../media/*", {
    query: "url",
    import: "default",
    eager: true,
  }),
});
