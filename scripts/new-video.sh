#!/usr/bin/env bash
# Creates a new fframes video project.
#
#   curl -fsSL https://raw.githubusercontent.com/dmtrKovalenko/fframes/main/scripts/new-video.sh | bash -s -- my-video
#
# Every argument is passed to `cargo fframes new`, e.g.:
#
#   ... | bash -s -- my-video --format portrait --fps 60 --backend skia-metal --yes
#
# Without --yes it asks for anything not passed when a terminal is attached. It installs
# `cargo-fframes` from crates.io if it is missing (FFRAMES_FROM_GIT=1 installs the `main`
# branch instead) and checks the system dependencies (Rust, ffmpeg, pkg-config) first.
set -euo pipefail

REPO="${FFRAMES_REPO:-https://github.com/dmtrKovalenko/fframes}"
BRANCH="${FFRAMES_BRANCH:-main}"

say() { printf '\033[1m%s\033[0m\n' "$*" >&2; }
fail() { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }

command -v cargo >/dev/null 2>&1 || fail "Rust is not installed. Install it from https://rustup.rs and run this again."

missing=()
command -v pkg-config >/dev/null 2>&1 || missing+=("pkg-config")
command -v ffmpeg >/dev/null 2>&1 || missing+=("ffmpeg (for ffprobe/ffplay; the renderer links its own ffmpeg)")
if ((${#missing[@]})); then
  say "Optional tools not found: ${missing[*]}"
  case "$(uname -s)" in
    Darwin) say "  brew install pkg-config ffmpeg nasm" ;;
    Linux) say "  sudo apt install pkg-config ffmpeg nasm clang libfontconfig1-dev" ;;
  esac
fi

if ! cargo fframes --help >/dev/null 2>&1 || [[ "${FFRAMES_REINSTALL:-}" == "1" ]]; then
  if [[ "${FFRAMES_FROM_GIT:-}" == "1" ]]; then
    say "Installing cargo-fframes from $REPO ($BRANCH)"
    cargo install --locked --git "$REPO" --branch "$BRANCH" cargo-fframes
  else
    say "Installing cargo-fframes from crates.io"
    cargo install --locked cargo-fframes
  fi
fi

# When piped from curl stdin is the script itself, give the prompts the terminal back.
if [[ ! -t 0 ]] && [[ -r /dev/tty ]] && [[ " $* " != *" --yes "* ]] && [[ " $* " != *" -y "* ]]; then
  exec cargo fframes new "$@" </dev/tty
fi
exec cargo fframes new "$@"
