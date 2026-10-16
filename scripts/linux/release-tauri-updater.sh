#!/usr/bin/env bash
set -euo pipefail

AWS_PROFILE="${AWS_PROFILE:-mash}"
R2_PREFIX="${R2_PREFIX:-mash}"
RELEASE_CHANNEL="${RELEASE_CHANNEL:-stable}"
LONG_CACHE_CONTROL="${LONG_CACHE_CONTROL:-public, max-age=31536000, immutable}"
LATEST_CACHE_CONTROL="${LATEST_CACHE_CONTROL:-no-cache}"

fail() {
  echo "error: $*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

[[ -n "${R2_ENDPOINT:-}" ]] || fail "R2_ENDPOINT is required"
[[ -n "${R2_BUCKET:-}" ]] || fail "R2_BUCKET is required"
[[ -n "${RELEASE_BASE_URL:-}" ]] || fail "RELEASE_BASE_URL is required"

for command in aws curl git node pnpm uname; do
  require_cmd "$command"
done

[[ "$(uname -s)" == "Linux" ]] || fail "run this script on Linux"
[[ "$(uname -m)" == "x86_64" ]] || fail "Linux updater currently supports x86_64 only"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

TAG="$(git describe --tags --exact-match 2>/dev/null || true)"
[[ "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([+-][0-9A-Za-z.-]+)*$ ]] || fail "current commit must be an exact vX.Y.Z tag"
[[ -z "$(git status --porcelain)" ]] || fail "worktree must be clean before publishing"

VERSION="${TAG#v}"
CONFIG_VERSION="$(node -e 'const fs = require("fs"); const c = JSON.parse(fs.readFileSync("src-tauri/tauri.conf.json", "utf8")); process.stdout.write(c.version || "");')"
[[ "$CONFIG_VERSION" == "$VERSION" ]] || fail "tag version $VERSION does not match tauri.conf.json version $CONFIG_VERSION"

R2_PREFIX="${R2_PREFIX#/}"
R2_PREFIX="${R2_PREFIX%/}"
RELEASE_BASE_URL="${RELEASE_BASE_URL%/}"
TARGET="linux-x86_64"
VERSION_PREFIX="$R2_PREFIX/releases/versions/$TAG/$TARGET"
VERSION_LATEST_KEY="$R2_PREFIX/releases/versions/$TAG/latest.json"
CHANNEL_LATEST_KEY="$R2_PREFIX/releases/$RELEASE_CHANNEL/latest.json"
WORK_DIR="$REPO_ROOT/src-tauri/target/release-updater/$TAG/$TARGET"
mkdir -p "$WORK_DIR"

pnpm tauri build --bundles appimage
BUNDLE_DIR="$REPO_ROOT/src-tauri/target/release/bundle"
ARTIFACT_FILE="$(node scripts/shared/updater-artifact.mjs "$BUNDLE_DIR" src-tauri/tauri.conf.json "$TARGET")"
SIG_FILE="$ARTIFACT_FILE.sig"
[[ -f "$SIG_FILE" ]] || fail "missing updater signature: $SIG_FILE"

ARTIFACT_NAME="$(basename "$ARTIFACT_FILE")"
SIG_NAME="$(basename "$SIG_FILE")"
ARTIFACT_KEY="$VERSION_PREFIX/$ARTIFACT_NAME"
SIG_KEY="$VERSION_PREFIX/$SIG_NAME"
ARTIFACT_URL="$RELEASE_BASE_URL/$ARTIFACT_KEY"
SIGNATURE="$(tr -d '\r\n' < "$SIG_FILE")"
PUB_DATE="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
LATEST_JSON="$WORK_DIR/latest.json"

aws_args=(--endpoint-url "$R2_ENDPOINT")
if [[ -n "$AWS_PROFILE" ]]; then aws_args+=(--profile "$AWS_PROFILE"); fi

aws s3 cp "$ARTIFACT_FILE" "s3://$R2_BUCKET/$ARTIFACT_KEY" "${aws_args[@]}" --cache-control "$LONG_CACHE_CONTROL"
aws s3 cp "$SIG_FILE" "s3://$R2_BUCKET/$SIG_KEY" "${aws_args[@]}" --cache-control "$LONG_CACHE_CONTROL"

node scripts/shared/updater-manifest.mjs "$LATEST_JSON" "$VERSION" "$PUB_DATE" \
  "$TARGET" "$ARTIFACT_URL" "$SIGNATURE"
aws s3 cp "$LATEST_JSON" "s3://$R2_BUCKET/$VERSION_LATEST_KEY" "${aws_args[@]}" \
  --cache-control "$LATEST_CACHE_CONTROL" --content-type application/json
aws s3 cp "$LATEST_JSON" "s3://$R2_BUCKET/$CHANNEL_LATEST_KEY" "${aws_args[@]}" \
  --cache-control "$LATEST_CACHE_CONTROL" --content-type application/json

echo "Published Linux updater $TAG"
echo "  artifact: $ARTIFACT_URL"
