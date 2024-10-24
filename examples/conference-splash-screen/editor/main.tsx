import * as videoWasmBinding from "./editor-bridge/pkg/conference_splash_screen_bridge_example";
import { renderEditor } from "@fframes/editor";
import "@fframes/editor/dist/fframes-editor.css";

renderEditor(
  videoWasmBinding,
  {
    dynamicMediaFolder: import.meta.glob("../dynamic_media/*", {
      as: "url",
      eager: true,
    })
  }
);
