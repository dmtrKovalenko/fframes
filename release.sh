set -euo pipefail
VERSION="$1"
echo "Releasing version $VERSION"

if ! git diff --quiet; then
  echo "Error: There are unstaged changes in the repository."
  exit 1
fi

cargo install cargo-edit
cargo set-version "$VERSION"

# Crates first, in dependency order, so a failing npm publish can not hold back the Rust
# release. `cargo publish` waits until each crate is in the index before the next one.
for crate in \
  webvtt-parser \
  svgr-macro \
  fframes-media \
  media-dir-macro \
  fframes \
  fframes-editor-controller \
  fframes-skia-renderer \
  fframes-native-player \
  cargo-fframes; do
  echo "Publishing $crate"
  (cd "$crate" && cargo publish --allow-dirty --no-verify)
done

cd fframes-editor
pnpm build:prod
pnpm version "$VERSION" --no-git-tag-version
# npm itself: it authenticates with the job's OIDC token (trusted publishing) and attaches provenance
# Every nightly is a prerelease version; npm publishes those only with an explicit dist-tag,
# and `latest` is what `npm install @fframes/editor` has always resolved to
npm publish --access public --tag latest
