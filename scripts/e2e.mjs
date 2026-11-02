// Renders key frames of every example through its command line with one renderer and compares
// them with odiff to e2e/snapshots, which every renderer has to match. Frames and diffs of the
// run go to target/e2e/<renderer>.
//   node scripts/e2e.mjs cpu|skia-cpu|gpu [--update] [--example name]
import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  rmSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { compare } from "odiff-bin";

const [renderer, ...flags] = process.argv.slice(2);
let update = false;
let selectedExample;
const usage = () => {
  console.error(
    "usage: node scripts/e2e.mjs cpu|skia-cpu|gpu [--update] [--example name]"
  );
  process.exit(1);
};
if (!["cpu", "skia-cpu", "gpu"].includes(renderer)) usage();
for (let i = 0; i < flags.length; i++) {
  if (flags[i] === "--update") update = true;
  else if (flags[i] === "--example" && flags[i + 1] && !selectedExample)
    selectedExample = flags[++i];
  else usage();
}

// Renderers resample images and round colours differently: odiff's colour threshold 0.15
// absorbs that, and up to 0.5% of the pixels may still differ.
const THRESHOLD = 0.15;
const MAX_DIFF_PERCENT = 0.5;

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const target = process.env.CARGO_TARGET_DIR ?? join(root, "target");
const exe = process.platform === "win32" ? ".exe" : "";
const out = join(target, "e2e", renderer);

// `skia`: shaders, which the CPU renderer (tiny-skia) does not run. shader-mode is not here: its
// media is kept outside Git.
// prettier-ignore
const allExamples = [
  { dir: "anamorphic-video", pkg: "anamorphic-video", bin: "anamorphic-video", frames: "50%" },
  { dir: "audio-announce", pkg: "audio-announce-example", bin: "audio_announce_example_bin", frames: "5s,30s" },
  { dir: "beta", pkg: "beta-example", bin: "beta_example_bin", frames: "CodeDemoScene@50%,IphoneScene@50%,BetaExamples@50%" },
  // Preset layouts, typed highlights, wrapping and simultaneous cues.
  { dir: "captions-showcase", pkg: "captions-showcase-example", bin: "captions-showcase-example", frames: "1.4s,2.3s,4.2s,6.3s,13.3s,19s", skia: true, args: ["--shader", "--captions-only"] },
  { dir: "captions", pkg: "captions-example", bin: "captions-example", frames: "1.5s,5.2s,7s" },
  { dir: "conference-splash-screen", pkg: "conference-splash-screen-example", bin: "conference_splash_screen_bin", frames: "SponsorScene@50%,SpeakerScene@50%" },
  { dir: "fframes-intro", pkg: "fframes-intro", bin: "fframes-intro", frames: "CodeScene@50%,ShaderScene@2.6s,BenchmarkScene@12.4s,OutroScene@7s", skia: true },
  { dir: "hello-world", pkg: "hello-world-example", bin: "hello_world_example_bin", frames: "5s,20s" },
  { dir: "low-poly-art", pkg: "low-poly-art-example", bin: "low_poly_art_example_bin", frames: "Owl@3s,Owl@8s" },
  // tiger.json at four points of its loop: crouching, the take-off, in the air, landing
  { dir: "lottie", pkg: "lottie", bin: "lottie", frames: "Code@2s,Code@3.5s,Code@3.7s,Code@3.9s" },
  { dir: "made-of-motion", pkg: "made-of-motion", bin: "made-of-motion", frames: "Portrait@50%,Orbit@50%,Fframes@50%", skia: true },
  { dir: "marketing", pkg: "marketing-example", bin: "marketing-example", frames: "3s,10s,18s" },
  { dir: "neon-triangle", pkg: "neon-triangle-example", bin: "neon_triangle_example_bin", frames: "2s,4s", skia: true },
  { dir: "pixel-memory", pkg: "pixel-example", bin: "pixel_memory_example_bin", frames: "StartScene@1.2s,ParallaxGridPhotos[0]@7s,SinglePhotoFloat[0]@50%,SpiralHeapGallery@50%", args: ["--song", "revenge.mp3", "--seed", "48"] },
  { dir: "podcast", pkg: "podcast-example", bin: "podcast_example_bin", frames: "10s,40s" },
  { dir: "shaders", pkg: "shaders-example", bin: "shaders_example_bin", frames: "2s,6s", skia: true },
  { dir: "signal-lab", pkg: "signal-lab", bin: "signal-lab", frames: "ProductScene@3s,DataScene@3s,SystemScene@3s" },
  { dir: "teej-podcast", pkg: "teej_podcast_example", bin: "teej_podcast_example_bin", frames: "10s,2:00" },
  { dir: "tiktok", pkg: "tiktok-example", bin: "tiktok_example_bin", frames: "10s,30s" },
];

const examples = allExamples.filter(
  ({ dir }) => !selectedExample || dir === selectedExample
);
if (!examples.length) {
  console.error(`unknown example: ${selectedExample}`);
  process.exit(1);
}
if (selectedExample && renderer === "cpu" && examples[0].skia) {
  console.error(`${selectedExample} requires skia-cpu or gpu`);
  process.exit(1);
}

const run = (command, args, cwd) =>
  spawnSync(command, args, { cwd, stdio: ["ignore", "ignore", "inherit"] })
    .status === 0;

// One build: each example on its own would compile FFmpeg for its own codecs.
const packages = examples.flatMap(({ pkg }) => ["-p", pkg]);
const build = ["build", "--release", "--bins", ...packages];
if (spawnSync("cargo", build, { cwd: root, stdio: "inherit" }).status !== 0)
  process.exit(1);

rmSync(selectedExample ? join(out, selectedExample) : out, {
  recursive: true,
  force: true,
});
const failed = [];
const created = [];
for (const { dir, bin, frames, skia, args = [] } of examples) {
  if (skia && renderer === "cpu") continue;
  const frameDir = join(out, dir);
  const argv = [...args, "--renderer", renderer, "--scale", "0.5"];
  argv.push("frame", frames, "-o", frameDir);
  const binary = join(target, "release", bin + exe);
  if (!run(binary, argv, join(root, "examples", dir))) {
    failed.push(`${dir}: ${frames}`);
    continue;
  }

  for (const file of readdirSync(frameDir)) {
    const name = `${dir}/${file}`;
    const actual = join(frameDir, file);
    const snapshot = join(root, "e2e", "snapshots", dir, file);
    if (!existsSync(snapshot)) {
      mkdirSync(dirname(snapshot), { recursive: true });
      copyFileSync(actual, snapshot);
      created.push(name);
      continue;
    }

    const diff = actual.replace(/\.png$/, ".diff.png");
    const result = await compare(snapshot, actual, diff, {
      antialiasing: true,
      threshold: THRESHOLD,
    });
    const percent = result.match ? 0 : (result.diffPercentage ?? 100);
    console.log(`${name}: ${percent.toFixed(3)}% differ`);
    if (percent <= MAX_DIFF_PERCENT) continue;
    if (update) copyFileSync(actual, snapshot);
    else failed.push(`${name}: see ${diff}`);
  }
}

if (created.length) {
  console.error(
    `new snapshots, look at them and commit them:\n${created.join("\n")}`
  );
  // CI compares nothing against a snapshot it just created
  if (process.env.CI) process.exit(1);
}
if (failed.length) {
  console.error(`frames differ (${renderer}):\n${failed.join("\n")}`);
  process.exit(1);
}
