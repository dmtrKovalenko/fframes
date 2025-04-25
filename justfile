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
  just watch-example-build {{example}} &
  P1=$!
  just start-example {{example}} &
  P2=$!
  wait $P1 $P2

render example *ARGS:
  cd examples/{{example}} && cargo run --release {{ARGS}} && just play {{example}}

play example:
  cd examples/{{example}} && ffplay out.mp4

bench example *ARGS:
  cd examples/{{example}} && cargo build --release {{ARGS}} && time cargo run --release {{ARGS}}

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
  just check-wasm hello-world && just check-wasm podcast && just check-wasm tiktok && just check-wasm beta && just check-wasm low-poly-art && just check-wasm teej-podcast

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
