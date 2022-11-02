clippy:
  cargo clippy -- -D warnings -A clippy::option-map-unit-fn -A clippy::module_inception -A clippy::single-match

clippy-fix:
  cargo clippy --fix -- -D warnings -A clippy::option-map-unit-fn -A clippy::module_inception -A clippy::single-match

build:
  cargo build
  yarn
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

render example:
  cd examples/{{example}} && cargo run --release && just play {{example}}

play example:
  cd examples/{{example}} && ffplay out.mp4

bench example:
  cd examples/{{example}} && time cargo run --release

install-ffmpeg version: 
  git clone https://git.ffmpeg.org/ffmpeg.git ffmpeg
  cd ffmpeg && git fetch --tags
  cd ffmpeg && git checkout n{{version}}

  cd ffmpeg && ./configure --enable-shared --disable-x86asm
  cd ffmpeg && make 
  cd ffmpeg && sudo make install