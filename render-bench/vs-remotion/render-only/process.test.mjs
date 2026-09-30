import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { isolated } from "./process.mjs";
test(
  "timeout terminates a detached browser-like descendant",
  { skip: process.platform === "win32" },
  async () => {
    const dir = await fs.mkdtemp(
      path.join(os.tmpdir(), "render-bench-cleanup-")
    );
    try {
      const script = `const {spawn}=require('node:child_process'); const fs=require('node:fs'); const p=spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{detached:true,stdio:'ignore'});fs.writeFileSync(${JSON.stringify(path.join(dir, "pid"))},String(p.pid));setInterval(()=>{},1000);`;
      const result = await isolated(
        process.execPath,
        ["-e", script],
        1000,
        path.join(dir, "log")
      );
      assert.equal(result.status, "timeout");
      const pid = Number(await fs.readFile(path.join(dir, "pid"), "utf8"));
      // An orphan zombie awaits PID1 reaping but consumes no CPU or memory.
      let state = "";
      try {
        state = execFileSync("ps", ["-o", "stat=", "-p", String(pid)], {
          encoding: "utf8",
        }).trim();
      } catch {}
      assert.ok(
        !state || state.startsWith("Z"),
        `descendant still running: ${state}`
      );
    } finally {
      await fs.rm(dir, { recursive: true, force: true });
    }
  }
);
