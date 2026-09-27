clippy *ARGS:
  cargo clippy {{ARGS}} -- -D warnings -A clippy::option-map-unit-fn -A clippy::module_inception -A clippy::single-match -A clippy::single-range-in-vec-init

clippy-fix *ARGS:
  @just clippy --fix {{ARGS}}

syncpack:
  yarn syncpack lint

build:
  cargo build
  cd fframes-editor && yarn build 

init-repo:
  ffmpeg -version
  rustc --version
  yarn --version

  cargo build
  yarn install
  cd fframes-editor && yarn rescript:build && yarn bundle:dev

watch-editor:
  cd fframes-editor && yarn dev

watch-example-build example:
  cd examples/{{example}}/editor/editor-bridge && \
  cargo watch -i ../../../../.gitignore -s "wasm-pack build  --target web --dev"

start-example example:
  cd examples/{{example}}/editor && yarn dev

run example:
  #!/bin/bash
  set -eou pipefail
  trap 'echo "Killing background jobs..."; kill $(jobs -p) 2>/dev/null; exit 1' EXIT INT TERM
  
  just watch-example-build {{example}} &
  just start-example {{example}} &
  just watch-editor

render example *ARGS:
  cd examples/{{example}} && cargo run --release {{ARGS}} && just play {{example}}

play example:
  #!/bin/bash
  cd examples/{{example}}
  if command -v vlc >/dev/null 2>&1; then
    vlc out.mp4
  elif command -v iina >/dev/null 2>&1; then
    iina out.mp4
  elif command -v mpv >/dev/null 2>&1; then
    mpv out.mp4
  elif command -v ffplay >/dev/null 2>&1; then
    ffplay out.mp4
  else
    echo "No supported media player found. Please install iina, mpv, or vlc."
    exit 1
  fi

bench example *ARGS:
  cd examples/{{example}} && cargo build --release {{ARGS}} && time cargo run --release {{ARGS}}

# Per-stage and end-to-end CPU/GPU rendering benchmark over every example,
# e.g. `just render-bench --json before.json` then `just render-bench --compare before.json`
render-bench *ARGS:
  DYLD_LIBRARY_PATH=/opt/homebrew/lib:${DYLD_LIBRARY_PATH:-} cargo run --release -p render-bench -- {{ARGS}}

hyperfine example *ARGS:
  #!/bin/bash
  set -eou pipefail

  cd examples/{{example}}
  EXECUTABLE=$(
    cargo build --release {{ARGS}} --message-format=json |
    jq -r 'select(.executable != null) | .executable'
  )
  hyperfine $EXECUTABLE --warmup 2

check-wasm example:
  cd examples/{{example}}/editor/editor-bridge && cargo check --lib --target wasm32-unknown-unknown

check-examples:
  just check-wasm hello-world && just check-wasm podcast && just check-wasm tiktok && just check-wasm beta && just check-wasm low-poly-art && just check-wasm teej-podcast && just check-wasm motion-graphics

install-ffmpeg version: 
  git clone https://git.ffmpeg.org/ffmpeg.git ffmpeg
  cd ffmpeg && git fetch --tags
  cd ffmpeg && git checkout n{{version}}

  cd ffmpeg && ./configure --enable-shared --disable-x86asm
  cd ffmpeg && make 
  cd ffmpeg && sudo make install
test *ARGS: 
  cargo test {{ARGS}}
  cargo test -p fframes_test_utils --no-default-features {{ARGS}}

# Render a markdown file in the terminal (defaults to the agent guidelines)
md file="AGENTS.md":
  bat {{file}}
