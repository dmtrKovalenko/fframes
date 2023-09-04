import * as videoWasmBinding from "./editor-bridge/pkg/editor-bridge";
import { renderEditor } from "fframes-editor";
import "fframes-editor/dist/fframes-editor.css";

renderEditor(videoWasmBinding, {
  staticMediaFolder: import.meta.glob("../media/*", {
    as: "url",
    eager: true,
  }),
});
