import reactRefresh from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  assetsInclude: ["./media/*", "fframes-editor/*.wasm"],
  server: {
    fs: {
      strict: false,
    },
  },
  optimizeDeps: {
    entries: ["./main.tsx"],
  },
  plugins: [
    reactRefresh(),
  ],
});
