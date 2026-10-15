#!/usr/bin/env bash
# Builds the engine (if needed), the .app, and a plain compressed DMG.
# Tauri's styled DMG step drives Finder via AppleScript, which fails in headless/CI
# sessions; hdiutil alone produces a working installer image.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

[[ -f src-tauri/resources/engine/lib/libaudiocpp.dylib ]] || bash engine/scripts/build-engine.sh
npx tauri build --bundles app

VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
ARCH="$(uname -m)"
APP="src-tauri/target/release/bundle/macos/Murmur.app"
OUT="src-tauri/target/release/bundle/dmg/Murmur_${VERSION}_${ARCH}.dmg"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
mkdir -p "$(dirname "$OUT")"
rm -f "$OUT"
hdiutil create -volname "Murmur" -srcfolder "$STAGE" -ov -format UDZO "$OUT" >/dev/null
echo "Built $OUT ($(du -h "$OUT" | cut -f1))"
