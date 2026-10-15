#!/usr/bin/env bash
# Write Finder metadata directly; no AppleScript/Automation permission required.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ $# != 2 ]]; then
    echo "Usage: $0 /path/Murmur.app /path/output.dmg" >&2
    exit 2
fi
APP="$1"
OUT="$2"
[[ -d "$APP/Contents" ]] || { echo "Missing app bundle: $APP" >&2; exit 1; }
# Never replace a previously validated installer before the new image is ready.
[[ ! -e "$OUT" ]] || { echo "Output already exists: $OUT" >&2; exit 1; }
VENV="$ROOT/src-tauri/target/dmg-tools"
[[ -x "$VENV/bin/python" ]] || python3 -m venv "$VENV"
if ! "$VENV/bin/python" -c 'import importlib.metadata; assert importlib.metadata.version("dmgbuild") == "1.6.5"' >/dev/null 2>&1; then
    "$VENV/bin/python" -m pip install -r "$ROOT/scripts/dmg-requirements.txt"
fi
mkdir -p "$(dirname "$OUT")"
"$VENV/bin/dmgbuild" -s "$ROOT/scripts/dmg-settings.py" -D "root=$ROOT" -D "app=$APP" "Murmur" "$OUT"
