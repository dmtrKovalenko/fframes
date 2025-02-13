set -euo pipefail
VERSION="$1"
echo "Releasing version $VERSION"

if ! git diff --quiet; then
  echo "Error: There are unstaged changes in the repository."
  exit 1
fi

cargo install cargo-edit
cargo set-version "$VERSION"

cd fframes-editor
yarn build:prod
yarn publish --no-git-tag-version --access public --new-version "$VERSION"

cd ../webvtt-parser && cargo publish --allow-dirty --no-verify
cd ../svgr-macro && cargo publish --allow-dirty --no-verify
cd ../fframes-media-loaders && cargo publish --allow-dirty --no-verify
cd ../media-dir-macro && cargo publish --allow-dirty --no-verify
cd ../fframes && cargo publish --allow-dirty --no-verify
cd ../fframes-editor-controller && cargo publish --allow-dirty --no-verify
cd ../fframes-renderer && cargo publish --allow-dirty --no-verify
cd ../fframes-skia-renderer && cargo publish --allow-dirty --no-verify
