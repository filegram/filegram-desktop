#!/usr/bin/env bash
# Assembles a universal (arm64 + x86_64) Filegram.app from the release
# binaries, the .icns icon and the Info.plist template, then packs it into
# a compressed DMG with an /Applications symlink for drag-and-drop install.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
app=target/release/Filegram.app
dmg=target/release/Filegram.dmg

rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

rm -rf "$app" "$dmg"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
sed "s/@VERSION@/$version/g" assets/macos/Info.plist > "$app/Contents/Info.plist"
lipo -create \
    target/aarch64-apple-darwin/release/filegram \
    target/x86_64-apple-darwin/release/filegram \
    -output "$app/Contents/MacOS/filegram"
cp assets/icon/filegram.icns "$app/Contents/Resources/filegram.icns"

# Ad-hoc signature: arm64 macOS refuses to launch unsigned bundles.
codesign --force -s - "$app"

staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT
cp -R "$app" "$staging/"
ln -s /Applications "$staging/Applications"
hdiutil create -volname Filegram -srcfolder "$staging" -format UDZO -ov "$dmg"

echo "Bundled $dmg (version $version, archs: $(lipo -archs "$app/Contents/MacOS/filegram"))"
