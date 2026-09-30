import fs from "node:fs/promises";
import { spawn, execFileSync } from "node:child_process";

// Chromium creates its own process group. Terminate the full descendant tree,
// including its groups, before the worker dies and reparents those children.
export function terminateTree(pid) {
  if (!pid) return;
  if (process.platform === "win32") {
    try {
      execFileSync("taskkill", ["/pid", String(pid), "/t", "/f"], {
        stdio: "ignore",
      });
    } catch {}
    return;
  }
  const rows = execFileSync("ps", ["-axo", "pid=,ppid=,pgid="], {
    encoding: "utf8",
  })
    .trim()
    .split("\n")
    .map(line => line.trim().split(/\s+/).map(Number));
  const descendants = new Set([pid]);
  let changed = true;
  while (changed) {
    changed = false;
    for (const [child, parent] of rows)
      if (descendants.has(parent) && !descendants.has(child)) {
        descendants.add(child);
        changed = true;
      }
  }
  const ownGroup = rows.find(([id]) => id === process.pid)?.[2];
  const groups = new Set(
    rows.filter(([id]) => descendants.has(id)).map(row => row[2])
  );
  for (const group of groups)
    if (group !== ownGroup) {
      try {
        process.kill(-group, "SIGKILL");
      } catch {}
    }
  for (const id of descendants) {
    try {
      process.kill(id, "SIGKILL");
    } catch {}
  }
}
export async function isolated(command, argv, timeout, logPath) {
  return await new Promise((resolve, reject) => {
    const child = spawn(command, argv, {
      detached: process.platform !== "win32",
      env: { ...process.env, NODE_ENV: "production" },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "",
      stderr = "",
      expired = false;
    const stop = () => terminateTree(child.pid);
    const timer = setTimeout(() => {
      expired = true;
      stop();
    }, timeout);
    child.stdout.on("data", b => {
      stdout += b;
    });
    child.stderr.on("data", b => {
      stderr += b;
    });
    child.on("error", err => {
      stderr += String(err);
    });
    child.on("close", async (code, signal) => {
      clearTimeout(timer);
      try {
        stop();
        await fs.writeFile(logPath, stdout + "\n" + stderr);
        resolve({
          status: expired ? "timeout" : code === 0 ? "ok" : "failed",
          code,
          signal,
          stdout,
          error: stderr.slice(-4000),
        });
      } catch (err) {
        reject(err);
      }
    });
  });
}
