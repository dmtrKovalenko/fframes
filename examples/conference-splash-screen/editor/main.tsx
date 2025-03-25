import initWasm, {
  create_wasm_bridge,
} from "./editor-bridge/pkg/conference_splash_screen_bridge_example";
import { renderEditor } from "@fframes/editor";
import "@fframes/editor/dist/fframes-editor.css";

await initWasm();
renderEditor(create_wasm_bridge(), {
  dynamicMediaFolder: import.meta.glob("../dynamic_media/*", {
    query: "url",
    import: "default",
    eager: true,
  }),
});
