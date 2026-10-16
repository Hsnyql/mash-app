#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

SIGNED=false
if [[ "${1:-}" == "--signed" ]]; then
  SIGNED=true
elif [[ -n "${1:-}" ]]; then
  echo "usage: $0 [--signed]" >&2
  exit 2
fi

cd "$REPO_ROOT"
"$SCRIPT_DIR/prepare-linux-adb.sh"

if [[ "$SIGNED" == true ]]; then
  pnpm tauri build --bundles appimage,deb
else
  CONFIG_PATH="$REPO_ROOT/src-tauri/target/linux-unsigned.json"
  mkdir -p "$(dirname "$CONFIG_PATH")"
  cat >"$CONFIG_PATH" <<'JSON'
{"bundle":{"createUpdaterArtifacts":false}}
JSON
  pnpm tauri build --bundles appimage,deb --config "$CONFIG_PATH"
fi
