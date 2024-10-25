import * as videoWasmBinding from "./editor-bridge/pkg/low_poly_art_bridge";
import { renderEditor } from "@fframes/editor";
import "fframes-editor/dist/fframes-editor.css";

renderEditor(videoWasmBinding, {
  dynamicMediaFolder: import.meta.glob("../media/*", {
    as: "url",
    eager: true,
  }),
});
