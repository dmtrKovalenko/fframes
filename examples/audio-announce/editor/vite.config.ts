import reactRefresh from "@vitejs/plugin-react-refresh";
import { defineConfig } from "vite";
import ViteRsw from "vite-plugin-rsw";

export default defineConfig({
  assetsInclude: ["./media/*", "fframes-editor/*.wasm"],
  server: {
    fs: {
      strict: false,
    },
  },
  assetsInlineLimit: 0,
  optimizeDeps: {
    entries: [".editor-bridge/main.tsx"],
  },
  plugins: [
    reactRefresh(),
    ViteRsw({
      profile: "dev",
      crates: ["editor-bridge"],
    }),
  ],
});
