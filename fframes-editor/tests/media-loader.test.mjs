import assert from "node:assert/strict";
import { test } from "node:test";
import { build } from "esbuild";
import { fileURLToPath } from "node:url";

// Exercise the real generated loader and TS resolvers together, with browser I/O mocked.
const bundle = await build({
  stdin: {
    contents: `
      export { processMedia, MediaLoaderObserver } from "./src/services/mediaLoader.bs.js";
      export { get as mediaAt } from "rescript/lib/es6/belt_MapString.js";
    `,
    resolveDir: fileURLToPath(new URL("../", import.meta.url)),
  },
  bundle: true,
  platform: "node",
  format: "esm",
  write: false,
});

test("failed subtitles settle with asset errors while successful media remains usable", async t => {
  globalThis.AudioContext = class {};
  t.after(() => delete globalThis.AudioContext);
  const { processMedia, MediaLoaderObserver, mediaAt } = await import(
    `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString("base64")}`
  );
  const controller = {
    populate_static_fonts_db_with_static_fonts() {},
    get_static_font_data_by_index() {},
    get_static_audio_data_by_index() {},
    add_subtitles_source: () => 2,
    add_json_subtitles_source() {
      throw new Error("invalid cue timing");
    },
  };
  t.mock.method(globalThis, "fetch", async url => {
    const status =
      url === "/missing" ? 404 : url === "/unavailable" ? 503 : 200;
    return new Response("subtitle data", { status });
  });
  const loading = processMedia(
    {
      "./missing.vtt": "/missing",
      "./captions.json": "/unavailable",
      "./invalid.json": "/invalid",
      "./good.vtt": "/good",
    },
    controller,
    {}
  );
  assert.equal(MediaLoaderObserver.get().allMediaLoaded, false);
  await loading;
  const state = MediaLoaderObserver.get();
  assert.equal(state.allMediaLoaded, true);
  for (const [file, message] of [
    ["missing.vtt", /HTTP 404/],
    ["captions.json", /HTTP 503/],
    ["invalid.json", /invalid cue timing/],
  ]) {
    const asset = mediaAt(state.mediaList, file);
    assert.equal(asset.TAG, 2, `${file} must be Error, not Loading`);
    assert.match(asset._0, message);
  }
  assert.equal(mediaAt(state.mediaList, "good.vtt").TAG, 1);
  const retry = processMedia({ "./missing.vtt": "/good" }, controller, {});
  assert.equal(MediaLoaderObserver.get().allMediaLoaded, false);
  await retry;
  assert.equal(
    mediaAt(MediaLoaderObserver.get().mediaList, "missing.vtt").TAG,
    1
  );
});
