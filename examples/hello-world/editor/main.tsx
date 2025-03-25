import initWasm, {
  create_wasm_bridge,
} from "./editor-bridge/pkg/hello_word_bridge_example";
import { renderEditor } from "@fframes/editor";
import "@fframes/editor/dist/fframes-editor.css";

await initWasm();
const bridge = create_wasm_bridge();

renderEditor(
  bridge,
  import.meta.glob("../media/*", {
    query: "url",
    import: "default",
    eager: true,
  })
);
