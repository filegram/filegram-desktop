#!/usr/bin/env bash
# Assembles a universal (arm64 + x86_64) Filegram.app from the release
# binaries, the .icns icon and the Info.plist template, then zips it with
# ditto (plain `zip`/upload-artifact would drop the executable bit).
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
app=target/release/Filegram.app

rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

rm -rf "$app" "$app.zip"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
sed "s/@VERSION@/$version/g" assets/macos/Info.plist > "$app/Contents/Info.plist"
lipo -create \
    target/aarch64-apple-darwin/release/filegram \
    target/x86_64-apple-darwin/release/filegram \
    -output "$app/Contents/MacOS/filegram"
cp assets/icon/filegram.icns "$app/Contents/Resources/filegram.icns"

# Ad-hoc signature: arm64 macOS refuses to launch unsigned bundles.
codesign --force -s - "$app"

ditto -c -k --keepParent "$app" "$app.zip"
echo "Bundled $app.zip (version $version, archs: $(lipo -archs "$app/Contents/MacOS/filegram"))"
