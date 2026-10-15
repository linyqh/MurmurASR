#!/usr/bin/env bash
# Builds the engine (if needed), the .app, and a branded compressed DMG.
# dmgbuild writes Finder metadata without AppleScript, including in CI.
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

mkdir -p "$(dirname "$OUT")"
FINAL_OUT="$OUT"
OUT="$STAGE/$(basename "$FINAL_OUT")"
bash scripts/create-dmg.sh "$APP" "$OUT"
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
hdiutil verify "$OUT"
mv -f "$OUT" "$FINAL_OUT"
OUT="$FINAL_OUT"
(cd "$(dirname "$OUT")" && shasum -a 256 "$(basename "$OUT")" > "$(basename "$OUT").sha256")
echo "Built $OUT ($(du -h "$OUT" | cut -f1))"
