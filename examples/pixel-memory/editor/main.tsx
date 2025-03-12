import * as videoWasmBinding from "./editor-bridge/pkg/pixel_bridge_example";
import { renderEditor } from "@fframes/editor";
import "@fframes/editor/dist/fframes-editor.css";

renderEditor(
  videoWasmBinding, {
  staticMediaFolder: import.meta.glob("../media/*", {
    as: "url",
    eager: true,
  }),
  dynamicMediaFolder: import.meta.glob("../photos/*", {
    as: "url",
    eager: true,
  }),
});
