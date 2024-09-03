import * as videoWasmBinding from "./editor-bridge/pkg";
import { renderEditor } from "fframes-editor";
import "fframes-editor/dist/fframes-editor.css";

const dynamicMediaFolder = import.meta.glob("../dynamic_media/*", {
  as: "url",
  eager: true,
});

renderEditor(videoWasmBinding, {
  dynamicMediaFolder,
});
