// @ts-check
/// <reference types="node" />
import esbuild from "esbuild";
const release = process.argv.some(arg => arg.includes("-r"));

let ctx = await esbuild.context({
  entryPoints: ["./src/fframes-editor.tsx"],
  outdir: "dist",
  bundle: true,
  splitting: true,
  treeShaking: true,
  format: "esm",
  target: ["es2020"],
  external: ["*?url"],
  minify: release,
  define: {
    "process.env.NODE_ENV": JSON.stringify(
      release ? "production" : "development"
    ),
  },
  loader: {
    ".ttf": "file",
    ".woff2": "file",
    ".svg": "base64",
  },
});

if (process.argv.includes("--watch")) {
  await ctx.watch();
} else {
  await ctx.rebuild();
  await ctx.dispose();
}
