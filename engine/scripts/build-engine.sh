#!/usr/bin/env bash
# Builds libaudiocpp (audio.cpp C ABI) for macOS / Linux and stages it, together
# with the bundled VAD assets, into src-tauri/resources/engine.
#
# Builds the latest audio.cpp release tag (vX.Y.Z). Set AUDIOCPP_REF to build a
# specific tag instead.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/engine/audiocpp.conf"

SRC_DIR="$ROOT/engine/vendor/audio.cpp"
BACKEND=""
BUILD_TYPE="Release"
JOBS=""

usage() {
    cat <<'EOF'
Usage: engine/scripts/build-engine.sh [--backend metal|cpu|vulkan|cuda] [--debug] [-j N]

Defaults: metal on macOS, vulkan on Linux.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --backend) BACKEND="$2"; shift 2 ;;
        --debug) BUILD_TYPE="RelWithDebInfo"; shift ;;
        -j|--jobs) JOBS="$2"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown argument: $1" >&2; usage; exit 1 ;;
    esac
done

OS="$(uname -s)"
case "$OS" in
    Darwin)
        BACKEND="${BACKEND:-metal}"
        JOBS="${JOBS:-$(sysctl -n hw.logicalcpu)}"
        ;;
    Linux)
        BACKEND="${BACKEND:-vulkan}"
        JOBS="${JOBS:-$(nproc)}"
        ;;
    *)
        echo "Use engine/scripts/build-engine.ps1 on Windows." >&2
        exit 1
        ;;
esac

if [[ ! -d "$SRC_DIR/.git" ]]; then
    git clone --filter=blob:none --no-checkout "$AUDIOCPP_REPO" "$SRC_DIR"
fi
if [[ -z "${AUDIOCPP_REF:-}" ]]; then
    AUDIOCPP_REF="$(git ls-remote --tags --refs "$AUDIOCPP_REPO" 'v*' |
        sed 's#.*refs/tags/##' | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | sort -V | tail -n 1 || true)"
fi
if [[ -n "${AUDIOCPP_REF:-}" ]]; then
    git -C "$SRC_DIR" fetch --depth 1 origin "refs/tags/$AUDIOCPP_REF:refs/tags/$AUDIOCPP_REF"
    git -C "$SRC_DIR" checkout --detach "$AUDIOCPP_REF"
else
    # Offline: rebuild whatever release is already checked out.
    AUDIOCPP_REF="$(git -C "$SRC_DIR" describe --tags --exact-match 2>/dev/null || true)"
    [[ -n "$AUDIOCPP_REF" ]] || { echo "cannot reach $AUDIOCPP_REPO and no release is checked out" >&2; exit 1; }
    echo "warning: cannot reach $AUDIOCPP_REPO; building the checked-out $AUDIOCPP_REF" >&2
fi
echo "audio.cpp $AUDIOCPP_REF"

CMAKE_ARGS=(
    -DCMAKE_BUILD_TYPE="$BUILD_TYPE"
    -DAUDIOCPP_BUILD_C_API=ON
    -DAUDIOCPP_DEPLOYMENT_BUILD=ON
    -DAUDIOCPP_MODEL_SET=custom
    -DAUDIOCPP_MODELS="$AUDIOCPP_MODELS"
    -DAUDIOCPP_VERSION="${AUDIOCPP_REF#v}"
    -DENGINE_ENABLE_CUDA=OFF
    -DENGINE_ENABLE_HIP=OFF
    -DENGINE_ENABLE_VULKAN=OFF
    -DENGINE_ENABLE_METAL=OFF
    -DENGINE_ENABLE_NATIVE_CPU=OFF
)

case "$BACKEND" in
    metal) CMAKE_ARGS+=(-DENGINE_ENABLE_METAL=ON -DGGML_METAL_EMBED_LIBRARY=ON) ;;
    vulkan) CMAKE_ARGS+=(-DENGINE_ENABLE_VULKAN=ON) ;;
    cuda) CMAKE_ARGS+=(-DENGINE_ENABLE_CUDA=ON) ;;
    cpu) ;;
    *) echo "unsupported backend: $BACKEND" >&2; exit 1 ;;
esac

if [[ "$OS" == "Darwin" ]]; then
    # AppleClang ships no OpenMP; ggml's own thread pool is used instead.
    CMAKE_ARGS+=(
        -DENGINE_ENABLE_OPENMP=OFF
        -DGGML_OPENMP=OFF
        -DCMAKE_OSX_DEPLOYMENT_TARGET=13.3
        -DCMAKE_OSX_ARCHITECTURES="$(uname -m)"
    )
fi

BUILD_DIR="$ROOT/engine/build/$(uname -s | tr '[:upper:]' '[:lower:]')-$BACKEND"
cmake -S "$SRC_DIR" -B "$BUILD_DIR" "${CMAKE_ARGS[@]}"
cmake --build "$BUILD_DIR" --config "$BUILD_TYPE" --parallel "$JOBS" --target audiocpp

STAGE="$ROOT/src-tauri/resources/engine"
rm -rf "$STAGE"
mkdir -p "$STAGE/lib" "$STAGE/assets"
find "$BUILD_DIR/bin" -maxdepth 1 \( -name '*.dylib' -o -name '*.so*' \) -exec cp -P {} "$STAGE/lib/" \;
cp -R "$SRC_DIR/assets/framework/models/silero_vad" "$STAGE/assets/"
cp "$SRC_DIR/LICENSE" "$STAGE/LICENSE.audio.cpp"
cp "$SRC_DIR/include/audiocpp.h" "$ROOT/engine/audiocpp.h"

if [[ "$OS" == "Darwin" ]]; then
    install_name_tool -id "@rpath/libaudiocpp.dylib" "$STAGE/lib/libaudiocpp.dylib" 2>/dev/null || true
fi

echo
echo "Engine staged at $STAGE ($BACKEND)"
ls -lh "$STAGE/lib"
