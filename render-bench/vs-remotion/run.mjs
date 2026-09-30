import fs from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import { isolated } from "./process.mjs";
import { createHash } from "node:crypto";
import { bundle } from "@remotion/bundler";
import {
  openBrowser,
  selectComposition,
  renderFrames,
} from "@remotion/renderer";
import { summarize, markdown } from "./report.mjs";
const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");
const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i < 0 ? fallback : args[i + 1];
};
const git = (...a) =>
  execFileSync("git", ["-C", root, ...a], { encoding: "utf8" }).trim();

async function remotionWorker(config) {
  const chromiumOptions = { disableWebSecurity: false };
  const browser = await openBrowser("chrome", {
    browserExecutable: config.chrome,
    chromiumOptions,
    logLevel: "error",
  });
  try {
    const composition = await selectComposition({
      serveUrl: config.bundle,
      id: "MixedGrid",
      puppeteerInstance: browser,
      timeoutInMilliseconds: config.timeout,
      logLevel: "error",
    });
    const samples = [];
    const images = new Map();
    let dom;
    await renderFrames({
      composition,
      serveUrl: config.bundle,
      puppeteerInstance: browser,
      concurrency: 1,
      frameRange: [0, config.warmup + config.frames - 1],
      imageFormat: "png",
      outputDir: null,
      timeoutInMilliseconds: config.timeout,
      logLevel: "error",
      onStart: ({ parallelEncoding, resolvedConcurrency }) => {
        if (parallelEncoding || resolvedConcurrency !== 1)
          throw new Error("unexpected Remotion rendering pipeline");
      },
      onFrameBuffer: async (buffer, frame) => {
        if (frame >= config.warmup) images.set(frame, buffer);
        // Inspect the real Remotion page during warm-up, outside measured frames.
        if (frame === config.warmup - 1) {
          for (const page of await browser.pages()) {
            const counts = await page.evaluate(() => ({
              rectangles: document.querySelectorAll("svg rect").length,
              texts: document.querySelectorAll("svg text").length,
            }));
            if (counts.rectangles + counts.texts === config.nodes) dom = counts;
          }
          if (!dom || dom.texts !== Math.ceil(config.nodes / 100))
            throw new Error("incorrect Remotion DOM element count");
        }
      },
      onFrameUpdate: (_count, frame, total_ms) => {
        if (frame >= config.warmup) samples.push({ frame, total_ms });
      },
    });
    samples.sort((a, b) => a.frame - b.frame);
    // Remotion's per-frame timing covers seek, effects, raster and PNG capture.
    // Save captured buffers only after rendering has finished.
    for (const [frame, buffer] of images)
      await fs.writeFile(path.join(config.directory, `${frame}.png`), buffer);
    return {
      samples,
      renderer: "@remotion/renderer renderFrames",
      concurrency: 1,
      browser: execFileSync(config.chrome, ["--version"], {
        encoding: "utf8",
      }).trim(),
      chromium_options: chromiumOptions,
      dom,
    };
  } finally {
    await browser.close({ silent: true });
  }
}
async function directoryHash(directory) {
  const hash = createHash("sha256");
  for (const name of (
    await fs.readdir(directory, { recursive: true })
  ).sort()) {
    const file = path.join(directory, name);
    if ((await fs.stat(file)).isFile())
      hash.update(name).update(await fs.readFile(file));
  }
  return hash.digest("hex");
}
if (args[0] === "--worker") {
  const config = JSON.parse(await fs.readFile(args[1], "utf8"));
  console.log(JSON.stringify(await remotionWorker(config)));
} else {
  for (let i = 0; i < args.length; i += 2) {
    if (!["--chrome", "--out", "--binary"].includes(args[i]) || !args[i + 1])
      throw new Error(`unknown or incomplete option: ${args[i]}`);
  }
  const plan = {
    nodes: 100000,
    effect_passes: 12,
    rounds: 3,
    frames: 30,
    warmup: 3,
    timeout: 300000,
    backends: ["skia-cpu", "skia-gpu-if-available"],
    chrome: option("chrome", process.env.CHROME_PATH ?? "/usr/bin/chromium"),
  };
  const binary = path.resolve(
    option("binary", path.join(root, "target/release/vs-remotion-bench"))
  );
  const out = path.resolve(
    option(
      "out",
      path.join(here, "out", new Date().toISOString().replaceAll(":", "-"))
    )
  );
  await fs.mkdir(out, { recursive: true });
  try {
    await fs.access(path.join(out, "results.json"));
    throw new Error(
      "output already contains results; choose a new --out directory"
    );
  } catch (err) {
    if (err.code !== "ENOENT") throw err;
  }
  const report = {
    schema_version: 5,
    comparison: "Remotion renderFrames vs fframes Previewer + Skia",
    workload:
      "99% rectangles, 1% changing DM Sans text digits; text painted last",
    plan,
    environment: {
      platform: os.platform(),
      arch: os.arch(),
      cpus: os.cpus()[0]?.model,
      logical_cpus: os.availableParallelism(),
      memory_bytes: os.totalmem(),
      node: process.version,
      remotion: JSON.parse(
        await fs.readFile(path.join(here, "node_modules/remotion/package.json"))
      ).version,
      react: JSON.parse(
        await fs.readFile(path.join(here, "node_modules/react/package.json"))
      ).version,
      git: git("rev-parse", "HEAD"),
      dirty: !!git("status", "--porcelain"),
      command: process.argv,
      font_sha256: createHash("sha256")
        .update(await fs.readFile(path.join(here, "media/DMSans-Regular.ttf")))
        .digest("hex"),
      binary_sha256: createHash("sha256")
        .update(await fs.readFile(binary))
        .digest("hex"),
    },
    records: [],
  };
  const serveUrl = await bundle({
    entryPoint: path.join(here, "browser.jsx"),
    outDir: path.join(out, "remotion-bundle"),
    publicDir: path.join(here, "media"),
    enableCaching: false,
  });
  report.environment.browser_bundle_sha256 = await directoryHash(serveUrl);
  const persist = async () => {
    report.summary = summarize(report.records, plan);
    await fs.writeFile(
      path.join(out, "results.json"),
      JSON.stringify(report, null, 2)
    );
    await fs.writeFile(path.join(out, "results.md"), markdown(report));
  };
  for (let round = 0; round < plan.rounds; round++) {
    const nodes = plan.nodes;
    const engines = ["fframes", "fframes-gpu", "remotion"];
    const order = engines.map((_, i) => engines[(i + round) % engines.length]);
    for (const engine of order) {
      const directory = path.join(out, `${nodes}-${engine}-${round}`);
      await fs.mkdir(directory, { recursive: true });
      const config = {
        ...plan,
        nodes,
        engine,
        round,
        directory,
        bundle: serveUrl,
      };
      const configPath = path.join(directory, "config.json");
      await fs.writeFile(configPath, JSON.stringify(config));
      console.error(
        `round ${round + 1}/${plan.rounds}: ${nodes} nodes, ${engine}`
      );
      const result =
        engine !== "remotion"
          ? await isolated(
              binary,
              [engine === "fframes" ? "cpu" : "gpu", directory],
              plan.timeout,
              path.join(directory, "process.log")
            )
          : await isolated(
              process.execPath,
              [fileURLToPath(import.meta.url), "--worker", configPath],
              plan.timeout,
              path.join(directory, "process.log")
            );
      const record = {
        nodes,
        engine,
        round,
        status: result.status,
        load_after: os.loadavg(),
        code: result.code,
        signal: result.signal,
      };
      try {
        if (result.status !== "ok")
          throw new Error(result.error || result.status);
        Object.assign(record, JSON.parse(result.stdout));
      } catch (err) {
        record.status =
          result.status === "ok" ? "invalid-output" : result.status;
        record.error = String(err);
      }
      report.records.push(record);
      await persist();
      console.error(`${engine}: ${record.status}`);
    }
  }
  console.log(markdown(report));
  if (report.summary.status !== "complete") process.exitCode = 2;
}
