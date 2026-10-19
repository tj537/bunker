#!/bin/bash
# Builds dist/Bunker.app – a universal app for Apple Silicon and Intel Macs.
#
#   ./scripts/build-mac.sh            build only
#   ./scripts/build-mac.sh --install  build, replace /Applications/Bunker.app and remove the build copy
#   ./scripts/build-mac.sh --zip      build and pack dist/Bunker-macos-universal.zip for a release
#   NATIVE=1 ./scripts/build-mac.sh   only for this Mac's CPU (faster while developing)
set -euo pipefail
cd "$(dirname "$0")/.."

CARGO="${CARGO:-cargo}"
command -v "$CARGO" >/dev/null || CARGO="$HOME/.cargo/bin/cargo"
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
APP=dist/Bunker.app
INSTALL_DIR=/Applications

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

if [[ "${NATIVE:-}" == "1" ]]; then
    echo "==> Building release binary (this Mac only)"
    "$CARGO" build --release
    cp target/release/bunker "$APP/Contents/MacOS/bunker"
else
    echo "==> Building release binaries (Apple Silicon + Intel)"
    rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null 2>&1 || true
    "$CARGO" build --release --target aarch64-apple-darwin
    "$CARGO" build --release --target x86_64-apple-darwin
    lipo -create -output "$APP/Contents/MacOS/bunker" \
        target/aarch64-apple-darwin/release/bunker \
        target/x86_64-apple-darwin/release/bunker
fi

echo "==> Assembling app bundle"
cp assets/icon.icns "$APP/Contents/Resources/AppIcon.icns"
sed "s/__VERSION__/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"

# Local ad-hoc signature (no Apple account needed) so macOS accepts the bundle as a whole
codesign --force --deep --sign - "$APP"
echo "==> Done: $APP"

if [[ "${1:-}" == "--zip" ]]; then
    # Fixed file name, so the README's "latest download" link always works
    ZIP="dist/Bunker-macos-universal.zip"
    rm -f "$ZIP"
    ditto -c -k --sequesterRsrc --keepParent "$APP" "$ZIP"
    echo "==> Packed: $ZIP"
    shasum -a 256 "$ZIP"
fi

if [[ "${1:-}" == "--install" ]]; then
    echo "==> Installing to $INSTALL_DIR"
    # A running copy would keep the old version open
    osascript -e 'tell application "Bunker" to quit' >/dev/null 2>&1 || true
    sleep 1
    rm -rf "$INSTALL_DIR/Bunker.app"
    cp -R "$APP" "$INSTALL_DIR/"
    # Otherwise Spotlight and Launchpad show a second Bunker
    rm -rf "$APP"
    rmdir dist 2>/dev/null || true
    echo "==> Installed: $INSTALL_DIR/Bunker.app"
fi
