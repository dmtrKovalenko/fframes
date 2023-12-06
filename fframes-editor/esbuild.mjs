import esbuild from "esbuild";

esbuild
  .build({
    entryPoints: ["./src/fframes-editor.tsx"],
    outdir: "dist",
    bundle: true,
    splitting: true,
    treeShaking: true,
    pure: true,
    format: "esm",
    target: ["es2020"],
    external: ["*?url"],
    watch: process.argv.some((arg) => arg.includes("-w")),
    loader: {
      ".ttf": "file",
      ".woff2": "file",
      ".svg": "base64",
    },
  })
  .catch(() => process.exit(1));
