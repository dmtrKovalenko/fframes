#!/usr/bin/env node
// Publishes skills/ on fframes.studio as an agent-skills well-known index, so
// `npx skills add https://fframes.studio` downloads four markdown files instead of
// cloning the whole repository with its example media. Runs as the wrangler build
// step (see wrangler.jsonc); the output directory is generated and gitignored.
import { cpSync, mkdirSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const skillsDir = join(root, "skills");
const outDir = join(root, "landing", ".well-known", "agent-skills");

function listFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    return entry.isDirectory() ? listFiles(path) : [path];
  });
}

// Reads `description` from the SKILL.md front matter: either an inline value or a
// folded (`>`, `>-`) / literal (`|`, `|-`) block scalar.
function readDescription(skillMd) {
  const frontMatter = skillMd.match(/^---\n([\s\S]*?)\n---/)?.[1];
  if (!frontMatter) throw new Error("SKILL.md has no front matter");

  const lines = frontMatter.split("\n");
  const start = lines.findIndex((line) => line.startsWith("description:"));
  if (start === -1) throw new Error("SKILL.md front matter has no description");

  const value = lines[start].slice("description:".length).trim();
  const block = value.match(/^([>|])[+-]?$/);
  if (!block) return value.replace(/^(["'])(.*)\1$/, "$2");

  const body = [];
  for (const line of lines.slice(start + 1)) {
    if (line.trim() !== "" && !/^\s/.test(line)) break;
    body.push(line.trim());
  }
  return (block[1] === ">" ? body.join(" ") : body.join("\n")).trim();
}

rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });

const skills = readdirSync(skillsDir, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map(({ name }) => {
    const dir = join(skillsDir, name);
    const files = listFiles(dir)
      .map((path) => relative(dir, path).split(sep).join("/"))
      .filter((path) => !path.split("/").at(-1).startsWith("."))
      .sort();
    const description = readDescription(readFileSync(join(dir, "SKILL.md"), "utf8"));

    for (const file of files) {
      mkdirSync(dirname(join(outDir, name, file)), { recursive: true });
      cpSync(join(dir, file), join(outDir, name, file));
    }
    return { name, description, files };
  });

writeFileSync(join(outDir, "index.json"), `${JSON.stringify({ skills }, null, 2)}\n`);
console.log(`Published ${skills.map((skill) => skill.name).join(", ")} to ${relative(root, outDir)}`);
