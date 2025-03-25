import initWasm, {
  create_wasm_bridge,
} from "./editor-bridge/pkg/low_poly_art_bridge";
import { renderEditor } from "@fframes/editor";
import "@fframes/editor/dist/fframes-editor.css";

await initWasm();

renderEditor(create_wasm_bridge(), {
  dynamicMediaFolder: import.meta.glob("../media/*", {
    query: "url",
    import: "default",
    eager: true,
  }),
});
