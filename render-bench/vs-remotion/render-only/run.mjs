import fs from "node:fs/promises";
import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";
import { isolated } from "./process.mjs";
import { performance } from "node:perf_hooks";
import { createHash } from "node:crypto";
import { build } from "esbuild";
import puppeteer from "puppeteer-core";
import { PNG } from "pngjs";
import { summarize, markdown } from "./report.mjs";
const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../..");
const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i < 0 ? fallback : args[i + 1];
};
const integer = (value, name, max = 1000000) => {
  const n = Number(value);
  if (!Number.isInteger(n) || n < 1 || n > max)
    throw new Error(`invalid ${name}`);
  return n;
};
const git = (...a) =>
  execFileSync("git", ["-C", root, ...a], { encoding: "utf8" }).trim();

async function verify(directory, frames, nodes) {
  const hashes = [];
  for (const frame of frames) {
    const png = PNG.sync.read(
      await fs.readFile(path.join(directory, `${frame}.png`))
    );
    if (png.width !== 1000 || png.height !== 1000)
      throw new Error("incorrect image dimensions");
    for (let slot = 0; slot < 1000000; slot++) {
      const id = (slot + frame * 37) % nodes;
      const rgb =
        slot < nodes
          ? [
              (id * 13 + frame * 17) % 256,
              (id * 7 + frame * 29) % 256,
              (id * 3 + frame * 43) % 256,
            ]
          : [0, 0, 0];
      for (let c = 0; c < 4; c++)
        if (png.data[slot * 4 + c] !== (c === 3 ? 255 : rgb[c]))
          throw new Error(
            `pixel mismatch: frame ${frame}, pixel ${slot}, channel ${c}`
          );
    }
    hashes.push(createHash("sha256").update(png.data).digest("hex"));
  }
  return hashes;
}
async function browserWorker(config) {
  const browser = await puppeteer.launch({
    executablePath: config.chrome,
    headless: true,
    protocolTimeout: config.timeout,
    args: [
      "--no-sandbox",
      "--disable-dev-shm-usage",
      "--force-color-profile=srgb",
      "--js-flags=--max-old-space-size=4096",
    ],
  });
  try {
    const page = await browser.newPage();
    let rejectCrash;
    const crashed = new Promise((_, reject) => {
      rejectCrash = reject;
    });
    crashed.catch(() => {});
    page.on("error", rejectCrash);
    await page.setViewport({ width: 1000, height: 1000, deviceScaleFactor: 1 });
    await page.setContent(
      '<!doctype html><html><head><style>html,body{margin:0;background:#000}</style></head><body><div id="root"></div></body></html>'
    );
    await page.addScriptTag({ path: config.bundle });
    const samples = [];
    for (let frame = 0; frame < config.frames + config.warmup; frame++) {
      const start = performance.now();
      await page.evaluate(
        (n, f, m) => window.renderFrame(n, f, m),
        config.nodes,
        frame,
        config.engine
      );
      const commit_ms = performance.now() - start;
      // Screenshot forces rasterization/readback and returns a PNG in memory.
      const bytes = await Promise.race([
        page.screenshot({
          type: "png",
          optimizeForSpeed: true,
        }),
        crashed,
      ]);
      const total_ms = performance.now() - start;
      if (frame >= config.warmup) {
        samples.push({
          frame,
          commit_ms,
          capture_png_ms: total_ms - commit_ms,
          total_ms,
        });
        await fs.writeFile(path.join(config.directory, `${frame}.png`), bytes);
      }
    }
    const dom_rectangles = await page.evaluate(
      () => document.querySelectorAll("rect").length
    );
    if (dom_rectangles !== config.nodes)
      throw new Error("incorrect DOM element count");
    const session = await browser.target().createCDPSession();
    const system = await session.send("SystemInfo.getInfo");
    return {
      samples,
      browser: await browser.version(),
      browser_args: browser.process().spawnargs,
      device_scale_factor: 1,
      dom_rectangles,
      browser_gpu: system.gpu,
    };
  } finally {
    await browser.close();
  }
}
if (args[0] === "--worker") {
  const config = JSON.parse(await fs.readFile(args[1], "utf8"));
  console.log(JSON.stringify(await browserWorker(config)));
} else {
  const plan = {
    nodes: option("nodes", "1000000")
      .split(",")
      .map(n => integer(n, "nodes")),
    modes: option("modes", "keyed-direct,unkeyed-direct,unkeyed-effects").split(
      ","
    ),
    rounds: integer(option("rounds", "3"), "rounds", 100),
    frames: integer(option("frames", "3"), "frames", 1000),
    warmup: integer(option("warmup", "1"), "warmup", 100),
    timeout: integer(option("timeout-ms", "300000"), "timeout", 3600000),
    backend: option("backend", "skia-cpu"),
    device: option("device", "CPU; no physical GPU"),
    chrome: option("chrome", process.env.CHROME_PATH ?? "/usr/bin/chromium"),
  };
  if (
    plan.modes.some(
      m => !["keyed-direct", "unkeyed-direct", "unkeyed-effects"].includes(m)
    )
  )
    throw new Error("invalid mode");
  const binary = path.resolve(
    option("binary", path.join(root, "target/release/render-only-bench"))
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
    schema_version: 1,
    plan,
    environment: {
      platform: os.platform(),
      arch: os.arch(),
      cpus: os.cpus()[0]?.model,
      logical_cpus: os.availableParallelism(),
      memory_bytes: os.totalmem(),
      node: process.version,
      react: JSON.parse(
        await fs.readFile(path.join(here, "node_modules/react/package.json"))
      ).version,
      git: git("rev-parse", "HEAD"),
      dirty: !!git("status", "--porcelain"),
      command: process.argv,
      binary_sha256: createHash("sha256")
        .update(await fs.readFile(binary))
        .digest("hex"),
    },
    records: [],
  };
  const bundle = path.join(out, "browser.js");
  await build({
    entryPoints: [path.join(here, "browser.jsx")],
    bundle: true,
    minify: true,
    outfile: bundle,
    define: { "process.env.NODE_ENV": '"production"' },
  });
  report.environment.browser_bundle_sha256 = createHash("sha256")
    .update(await fs.readFile(bundle))
    .digest("hex");
  const persist = async () => {
    report.summary = summarize(report.records, plan);
    await fs.writeFile(
      path.join(out, "results.json"),
      JSON.stringify(report, null, 2)
    );
    await fs.writeFile(path.join(out, "results.md"), markdown(report));
  };
  for (let round = 0; round < plan.rounds; round++)
    for (const nodes of plan.nodes) {
      const engines = ["fframes", ...plan.modes];
      // Rotate the complete, predetermined matrix rather than selecting the best run.
      const order = engines.map(
        (_, i) => engines[(i + round) % engines.length]
      );
      for (const engine of order) {
        const directory = path.join(out, `${nodes}-${engine}-${round}`);
        await fs.mkdir(directory, { recursive: true });
        const config = { ...plan, nodes, engine, round, directory, bundle };
        const configPath = path.join(directory, "config.json");
        await fs.writeFile(configPath, JSON.stringify(config));
        console.error(
          `round ${round + 1}/${plan.rounds}: ${nodes} nodes, ${engine}`
        );
        const result =
          engine === "fframes"
            ? await isolated(
                binary,
                [
                  plan.backend,
                  String(nodes),
                  String(plan.frames),
                  String(plan.warmup),
                  directory,
                ],
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
          verified: false,
          load_after: os.loadavg(),
          code: result.code,
          signal: result.signal,
        };
        try {
          if (result.status !== "ok")
            throw new Error(result.error || result.status);
          Object.assign(record, JSON.parse(result.stdout));
          record.pixel_sha256 = await verify(
            directory,
            record.samples.map(s => s.frame),
            nodes
          );
          record.verified = true;
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
  if (report.summary.some(r => r.status !== "complete")) process.exitCode = 2;
}
