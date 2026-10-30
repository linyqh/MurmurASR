#!/usr/bin/env bash
# Builds the signed, notarized DMG locally and uploads it to the GitHub
# Release draft for the current app version (creating the draft if needed).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ -f .env.macos.local ]]; then
    set -a
    # shellcheck disable=SC1091
    source .env.macos.local
    set +a
fi
if [[ -z "${APPLE_SIGNING_IDENTITY:-}" || -z "${MURMUR_NOTARY_PROFILE:-}" ]]; then
    echo "Release builds must be signed and notarized: set APPLE_SIGNING_IDENTITY and MURMUR_NOTARY_PROFILE in .env.macos.local." >&2
    exit 1
fi

if [[ -n "$(git status --porcelain)" ]]; then
    echo "Working tree has uncommitted changes; commit or stash them first." >&2
    exit 1
fi

VERSION="$(node -p "require('./src-tauri/tauri.conf.json').version")"
TAG="v$VERSION"
HEAD="$(git rev-parse HEAD)"

draft="$(gh release view "$TAG" --json isDraft -q .isDraft 2>/dev/null || true)"
if [[ "$draft" == false ]]; then
    echo "$TAG is already published; refusing to overwrite its assets." >&2
    exit 1
fi
if [[ "$draft" == true ]]; then
    remote="$(git ls-remote origin "refs/tags/$TAG^{}" | cut -f1)"
    [[ -n "$remote" ]] || remote="$(git ls-remote origin "refs/tags/$TAG" | cut -f1)"
    [[ -n "$remote" ]] || remote="$(git rev-parse "$(gh release view "$TAG" --json targetCommitish -q .targetCommitish)^{commit}")"
    if [[ "$remote" != "$HEAD" ]]; then
        echo "Draft $TAG targets $remote but HEAD is $HEAD; check out that commit first." >&2
        exit 1
    fi
else
    git fetch --quiet origin
    if [[ -z "$(git branch -r --contains HEAD)" ]]; then
        echo "HEAD is not on origin yet; push it first." >&2
        exit 1
    fi
    gh release create "$TAG" --draft --target "$HEAD" --title "MurmurASR $TAG" --generate-notes
fi
bash scripts/package-macos.sh

DMG="src-tauri/target/release/bundle/dmg/Murmur_${VERSION}_$(uname -m).dmg"
gh release upload "$TAG" --clobber "$DMG" "$DMG.sha256"
echo "Uploaded $(basename "$DMG") to draft $TAG"
