#!/usr/bin/env bash
# Publishes one version of every crate and of @fframes/editor. CI runs it with the version
# from scripts/release-version.sh; the versions in the tree are set unless they already match
# (a release commit made by scripts/release.sh already carries them).
#
#   scripts/publish.sh 1.0.0
set -euo pipefail
VERSION="$1"
echo "Publishing version $VERSION"

if ! git diff --quiet; then
  echo "Error: There are unstaged changes in the repository."
  exit 1
fi

if [ "$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[]|select(.name=="fframes")|.version')" != "$VERSION" ]; then
  cargo install cargo-edit
  cargo set-version "$VERSION"
fi

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
[ "$(node -p 'require("./package.json").version')" = "$VERSION" ] || pnpm version "$VERSION" --no-git-tag-version
# Nightlies must not become what `npm install @fframes/editor` resolves to once a stable
# version exists; npm insists on an explicit dist-tag for prereleases anyway.
case "$VERSION" in
  *rc*) DIST_TAG=nightly ;;
  *-*) DIST_TAG=next ;;
  *) DIST_TAG=latest ;;
esac
# npm itself: it authenticates with the job's OIDC token (trusted publishing) and attaches provenance
npm publish --access public --tag "$DIST_TAG"
