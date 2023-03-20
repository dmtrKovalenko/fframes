import reactRefresh from "@vitejs/plugin-react-refresh";
import { defineConfig } from "vite";
import ViteRsw from "vite-plugin-rsw";

function svgLoader(options = {}) {
  const { svgoConfig, svgo, defaultImport } = options;

  const svgRegex = /\.mp3$/;

  return {
    name: "svg-loader",
    enforce: "pre",

    async load(id) {
      if (!id.match(svgRegex)) {
        return;
      }

      const [path, query] = id.split("?", 2);

      const importType = query || defaultImport;

      if (importType === "url") {
        return; // Use default svg loader
      }

      let svg;

      try {
        svg = await fs.readFile(path, "utf-8");
      } catch (ex) {
        console.warn(
          "\n",
          `${id} couldn't be loaded by vite-svg-loader, fallback to default loader`
        );
        return;
      }

      if (importType === "raw") {
        return `export default ${JSON.stringify(svg)}`;
      }

      if (svgo !== false && query !== "skipsvgo") {
        svg = optimizeSvg(svg, {
          ...svgoConfig,
          path,
        }).data;
      }

      const { code } = compileTemplate({
        id: JSON.stringify(id),
        source: svg,
        filename: path,
        transformAssetUrls: false,
      });

      return `${code}\nexport default { render: render }`;
    },
  };
}

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
