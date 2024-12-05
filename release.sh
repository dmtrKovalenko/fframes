set -euo pipefail
VERSION="$1"
echo "Releasing version $VERSION"

# if ! git diff --quiet; then
#   echo "Error: There are unstaged changes in the repository."
#   exit 1
# fi

cargo install cargo-edit
cargo set-version "$VERSION"

# this is needed to force cargo to use specific version from crates during the publish
# after the release all these changes should be reverted
# sed -i 's/path = "[^"]*", //' Cargo.toml

cd fframes-editor
# yarn build:prod
# yarn publish --no-git-tag-version --access public --new-version "$VERSION"

cd ../webvtt-parser && cargo publish --allow-dirty
cd ../svgr-macro && cargo publish --allow-dirty
cd ../fframes-media-loaders && cargo publish --allow-dirty
cd ../media-dir-macro && cargo publish --allow-dirty
cd ../fframes && cargo publish --allow-dirty
cd ../fframes-editor-controller && cargo publish --allow-dirty
cd ../fframes-renderer && cargo publish --allow-dirty
