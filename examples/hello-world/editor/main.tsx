import * as videoWasmBinding from "./editor-bridge/pkg/hello_word_bridge_example";
import { renderEditor } from "@fframes/editor";
import "@fframes/editor/dist/fframes-editor.css";

renderEditor(
  videoWasmBinding,
  import.meta.glob("../media/*", {
    as: "url",
    eager: true,
  })
);
