#!/usr/bin/env bash
# Regenerate cargo-sources.json from Cargo.lock.
#
# Flathub builds run with no network access, so every crate must be vendored
# into cargo-sources.json. Run this whenever Cargo.lock changes (a release
# bump, a dependency update) and commit the result alongside the manifest.
#
# Requires Python 3 with aiohttp and tomlkit. The generator is fetched from
# flatpak/flatpak-builder-tools.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
out="$repo_root/packaging/flatpak/cargo-sources.json"
gen="$(mktemp -t flatpak-cargo-generator.XXXXXX.py)"
trap 'rm -f "$gen"' EXIT

curl -fsSL -o "$gen" \
  https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py

python3 "$gen" "$repo_root/Cargo.lock" -o "$out"
echo "wrote $out"
