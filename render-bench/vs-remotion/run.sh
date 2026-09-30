#!/usr/bin/env bash
# Primary comparison: frame rendering/capture only, no video encoding.
set -euo pipefail
cd "$(dirname "$0")/render-only"
exec node run.mjs "$@"
