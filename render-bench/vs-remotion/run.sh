#!/usr/bin/env bash
set -euo pipefail
bench=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$bench/../.." && pwd)
invocation_dir=$PWD
if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  echo "Usage: $0 [--chrome PATH] [--out DIR]"
  echo "Runs the fixed 100,000-element benchmark: 30 frames, 3 warm-up frames, 3 rounds."
  exit 0
fi
cd "$root"
cargo build --release -p vs-remotion-bench
target=$(cargo metadata --no-deps --format-version=1 | node -e \
  'let s=""; process.stdin.on("data", b => s += b); process.stdin.on("end", () => process.stdout.write(JSON.parse(s).target_directory));')
cd "$bench"
npm install --no-audit --no-fund
cd "$invocation_dir"
exec node "$bench/run.mjs" --binary "$target/release/vs-remotion-bench" "$@"
