#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
CACHE_DIR="$REPO_ROOT/src-tauri/target/linux-adb"
ARCHIVE="$CACHE_DIR/platform-tools_r36.0.2-linux.zip"
URL="https://dl.google.com/android/repository/platform-tools_r36.0.2-linux.zip"
EXPECTED_SHA256="3afdea91441815ab41254193df0343d92c1b1c0d0237165c3a345c8af8891c31"
DESTINATION="$REPO_ROOT/src-tauri/resources/adb/linux"

command -v curl >/dev/null 2>&1 || { echo "missing required command: curl" >&2; exit 1; }
command -v sha256sum >/dev/null 2>&1 || { echo "missing required command: sha256sum" >&2; exit 1; }
command -v unzip >/dev/null 2>&1 || { echo "missing required command: unzip" >&2; exit 1; }

mkdir -p "$CACHE_DIR" "$DESTINATION/lib64"
if [[ ! -f "$ARCHIVE" ]]; then
  curl --fail --location --retry 2 --output "$ARCHIVE" "$URL"
fi

ACTUAL_SHA256="$(sha256sum "$ARCHIVE" | awk '{print $1}')"
if [[ "$ACTUAL_SHA256" != "$EXPECTED_SHA256" ]]; then
  echo "Linux ADB archive SHA-256 mismatch: got $ACTUAL_SHA256, want $EXPECTED_SHA256" >&2
  exit 1
fi

unzip -oq "$ARCHIVE" -d "$CACHE_DIR"
cp "$CACHE_DIR/platform-tools/adb" "$DESTINATION/adb"
cp "$CACHE_DIR/platform-tools/lib64/libc++.so" "$DESTINATION/lib64/libc++.so"
cp "$CACHE_DIR/platform-tools/NOTICE.txt" "$DESTINATION/NOTICE.txt"
chmod 0755 "$DESTINATION/adb"

echo "Linux ADB 36.0.2 installed and verified."
