#!/usr/bin/env bash
# Stamps the build version into Cargo.toml: keeps major.minor from the
# checked-in version and sets patch to the commit count, so every CI build
# gets a monotonically increasing version (e.g. 0.1.247). Everything that
# reads the version (cargo-deb, cargo-generate-rpm, bundle-macos.sh,
# CARGO_PKG_VERSION) picks it up from Cargo.toml automatically.
#
# Requires the full git history (checkout with fetch-depth: 0).
set -euo pipefail
cd "$(dirname "$0")/.."

base=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2 | cut -d. -f1,2)
[[ -n "$base" ]] || { echo "error: could not parse version from Cargo.toml" >&2; exit 1; }
count=$(git rev-list --count HEAD)
version="$base.$count"

# perl instead of sed: "replace first match only" works the same on
# Linux and macOS runners (GNU sed's 0,/re/ address is not portable).
perl -pi -e "\$stamped ||= s/^version = \".*\"/version = \"$version\"/" Cargo.toml
grep -q "^version = \"$version\"" Cargo.toml \
    || { echo "error: failed to stamp version into Cargo.toml" >&2; exit 1; }
# Keep Cargo.lock in sync so --locked builds don't fail on the stamped version.
cargo update --workspace --offline 2>/dev/null || cargo update --workspace

echo "Stamped version $version"
if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    echo "version=$version" >> "$GITHUB_OUTPUT"
fi
