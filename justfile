clippy *ARGS:
  cargo clippy {{ARGS}} -- -D warnings -A clippy::option-map-unit-fn -A clippy::module_inception -A clippy::single-match

clippy-fix:
  cargo clippy --fix -- -D warnings -A clippy::option-map-unit-fn -A clippy::module_inception -A clippy::single-match

build:
  cargo build
  yarn install ---frozen-lockfile
  cd fframes-editor && yarn rescript:build

init-repo:
  ffmpeg -version
  rustc --version
  yarn --version

  cargo build
  yarn install
  cd fframes-editor && yarn rescript:build && yarn bundle:dev

watch-editor:
  cd fframes-editor && yarn dev

run example:
  cd examples/{{example}}/editor && yarn dev

render example *ARGS:
  cd examples/{{example}} && cargo run --release {{ARGS}} && just play {{example}}

play example:
  cd examples/{{example}} && ffplay out.mp4

bench example:
  cd examples/{{example}} && cargo build --release && time cargo run --release

check-wasm example:
  cd examples/{{example}} && cargo check --lib --target wasm32-unknown-unknown

check-examples:
  just check-wasm hellow-world & just check-wasm podcast & just check-wasm tiktok & just check-wasm beta & just check-wasm low-poly-art

install-ffmpeg version: 
  git clone https://git.ffmpeg.org/ffmpeg.git ffmpeg
  cd ffmpeg && git fetch --tags
  cd ffmpeg && git checkout n{{version}}

  cd ffmpeg && ./configure --enable-shared --disable-x86asm
  cd ffmpeg && make 
  cd ffmpeg && sudo make install

test-release *ARGS: 
  cargo test --release
  cargo test -p fframes_test_utils --no-default-features
