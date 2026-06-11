#!/usr/bin/env bash
# Sets the package version in Cargo.toml (and syncs Cargo.lock).
# Used by the release workflow: once for the release version (e.g. 0.2.0)
# and once for the follow-up dev version (e.g. 0.2.1-dev).
set -euo pipefail
cd "$(dirname "$0")/.."

version=${1:?usage: set-version.sh <version>}
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-dev)?$ ]] \
    || { echo "error: '$version' is not X.Y.Z or X.Y.Z-dev" >&2; exit 1; }

# perl instead of sed: "replace first match only" works the same on
# Linux and macOS (GNU sed's 0,/re/ address is not portable).
perl -pi -e "\$done ||= s/^version = \".*\"/version = \"$version\"/" Cargo.toml
grep -q "^version = \"$version\"" Cargo.toml \
    || { echo "error: failed to set version in Cargo.toml" >&2; exit 1; }
cargo update --workspace --quiet

echo "Set version $version"
