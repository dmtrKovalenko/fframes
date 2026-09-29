#!/usr/bin/env bash
# Prints the version CI publishes for the current checkout.
#
#   scripts/release-version.sh
#
# A pushed `v1.2.3` tag is released as `1.2.3`. Every other push to main is a nightly derived
# from the latest tag: after a prerelease tag it continues that prerelease
# (`v1.0.0-beta.7` -> `1.0.0-beta.7.rc-45`), after a stable tag it is a release candidate of
# the next patch version (`v1.0.0` -> `1.0.1-rc.12`), so it sorts after the release and cargo
# and npm keep resolving to the stable one by default. The number is the count of commits
# since the tag.
set -euo pipefail

ref="${GITHUB_REF_NAME:-}"
if [[ "$ref" == v[0-9]* ]] && git rev-parse -q --verify "refs/tags/$ref" >/dev/null; then
  echo "${ref#v}"
  exit 0
fi

tag=$(git describe --tags --abbrev=0)
last=${tag#v}
count=$(git rev-list --count "$tag..HEAD")
if [[ "$last" == *-* ]]; then
  echo "$last.rc-$count"
else
  IFS=. read -r major minor patch <<<"$last"
  echo "$major.$minor.$((patch + 1))-rc.$count"
fi
