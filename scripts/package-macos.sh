#!/usr/bin/env bash
# Builds the engine (if needed), the .app, and a plain compressed DMG.
# Tauri's styled DMG step drives Finder via AppleScript, which fails in headless/CI
# sessions; hdiutil alone produces a working installer image.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ -f .env.macos.local ]]; then
    set -a
    # Local signing identity and optional notarytool keychain profile.
    # shellcheck disable=SC1091
    source .env.macos.local
    set +a
fi

if [[ -n "${MURMUR_NOTARY_PROFILE:-}" && -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    echo "Notarization requires APPLE_SIGNING_IDENTITY." >&2
    exit 1
fi

[[ -f src-tauri/resources/engine/lib/libaudiocpp.dylib ]] || bash engine/scripts/build-engine.sh
if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    # Resource libraries are not automatically signed by Tauri's app bundler.
    while IFS= read -r -d '' library; do
        codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$library"
        codesign --verify --strict --verbose=2 "$library"
    done < <(find src-tauri/resources/engine/lib -type f -name '*.dylib' -print0)
fi
npx tauri build --bundles app

VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
ARCH="$(uname -m)"
APP="src-tauri/target/release/bundle/macos/Murmur.app"
OUT="src-tauri/target/release/bundle/dmg/Murmur_${VERSION}_${ARCH}.dmg"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    codesign --verify --deep --strict --verbose=2 "$APP"
fi

if [[ -n "${MURMUR_NOTARY_PROFILE:-}" ]]; then
    ditto -c -k --keepParent "$APP" "$STAGE/Murmur.zip"
    xcrun notarytool submit "$STAGE/Murmur.zip" --keychain-profile "$MURMUR_NOTARY_PROFILE" --wait
    xcrun stapler staple "$APP"
    xcrun stapler validate "$APP"
    rm "$STAGE/Murmur.zip"
fi

cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
mkdir -p "$(dirname "$OUT")"
rm -f "$OUT"
hdiutil create -volname "Murmur" -srcfolder "$STAGE" -ov -format UDZO "$OUT" >/dev/null
if [[ -n "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$OUT"
    codesign --verify --strict --verbose=2 "$OUT"
fi
if [[ -n "${MURMUR_NOTARY_PROFILE:-}" ]]; then
    xcrun notarytool submit "$OUT" --keychain-profile "$MURMUR_NOTARY_PROFILE" --wait
    xcrun stapler staple "$OUT"
    xcrun stapler validate "$OUT"
else
    echo "No MURMUR_NOTARY_PROFILE configured; DMG has not been notarized by this script."
fi
echo "Built $OUT ($(du -h "$OUT" | cut -f1))"
