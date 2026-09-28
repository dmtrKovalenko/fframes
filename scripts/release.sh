#!/bin/bash
# Cuts a release: sets the version in every crate and in the editor package, commits, tags
# and pushes. CI publishes the tag to crates.io and npm and creates the GitHub release.
#
#   ./scripts/release.sh 1.0.0
set -euo pipefail

VERSION="${1:?Usage: ./scripts/release.sh <version> (e.g. 1.0.0)}"

# Strip leading 'v' if provided
VERSION="${VERSION#v}"

TAG="v${VERSION}"

cd "$(dirname "$0")/.."
git pull

# Check for clean working tree
if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "Error: Working tree is not clean. Commit or stash changes first."
  exit 1
fi

# Check if tag already exists
if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "Error: Tag $TAG already exists"
  exit 1
fi

echo "Releasing $VERSION..."

echo "→ Updating Cargo.toml versions to $VERSION"
cargo set-version "$VERSION" 2>/dev/null || {
  echo "cargo-edit not found, installing..."
  cargo install cargo-edit
  cargo set-version "$VERSION"
}

echo "→ Updating @fframes/editor to $VERSION"
(cd fframes-editor && pnpm version "$VERSION" --no-git-tag-version >/dev/null)

git add -u
git commit -m "chore: release $VERSION"

echo "→ Creating tag $TAG"
git tag -a "$TAG" -m "Release $VERSION"

echo "→ Pushing to origin"
git push origin
git push origin "$TAG"

echo ""
echo "Release $VERSION created and pushed."
echo "CI will publish the crates and @fframes/editor: https://github.com/dmtrKovalenko/fframes/actions"
