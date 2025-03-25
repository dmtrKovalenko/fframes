import initWasm, {
  create_wasm_bridge,
} from "./editor-bridge/pkg/beta_editor_bridge";
import { renderEditor } from "@fframes/editor";
import "@fframes/editor/dist/fframes-editor.css";

await initWasm();
const bridge = create_wasm_bridge();

renderEditor(bridge, {
  dynamicImageSizeLimitBytes: 10 * 1024 * 1024, // 10MB
  dynamicMediaFolder: import.meta.glob("../media/*", {
    query: "url",
    import: "default",
    eager: true,
  }),
});
