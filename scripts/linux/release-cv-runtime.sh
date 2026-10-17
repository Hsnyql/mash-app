#!/usr/bin/env bash
set -euo pipefail

AWS_PROFILE="${AWS_PROFILE:-mash}"
R2_PREFIX="${R2_PREFIX:-mash}"
LONG_CACHE_CONTROL="${LONG_CACHE_CONTROL:-public, max-age=31536000, immutable}"
PLATFORM="linux-x86_64"

fail() {
  echo "error: $*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"
}

VERSION="${1:-}"
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "runtime version must look like x.y.z"
[[ -n "${R2_ENDPOINT:-}" ]] || fail "R2_ENDPOINT is required"
[[ -n "${R2_BUCKET:-}" ]] || fail "R2_BUCKET is required"
[[ -n "${RELEASE_BASE_URL:-}" ]] || fail "RELEASE_BASE_URL is required"

for command in aws curl git node sha256sum stat uname; do
  require_cmd "$command"
done
[[ "$(uname -s)" == "Linux" && "$(uname -m)" == "x86_64" ]] || fail "run this script on Linux x86_64"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"

[[ -z "$(git status --porcelain)" ]] || fail "worktree must be clean before publishing"
TAG="cv-runtime/$PLATFORM/$VERSION"
git rev-parse -q --verify "refs/tags/$TAG" >/dev/null && fail "tag already exists: $TAG"

export MASH_CV_RUNTIME_VERSION="$VERSION"
"$SCRIPT_DIR/build-cv.sh" --runtime-only

ARTIFACT="mash-cv-runtime-$PLATFORM-v$VERSION.zip"
DIST_PATH="$REPO_ROOT/sidecar/cv-runtime/dist/$ARTIFACT"
[[ -f "$DIST_PATH" ]] || fail "runtime artifact not found: $DIST_PATH"
SHA256="$(sha256sum "$DIST_PATH" | awk '{print $1}')"
KEY="${R2_PREFIX#/}/runtime/mash-cv/runtime/$PLATFORM/$ARTIFACT"
KEY="${KEY#/}"
URL="${RELEASE_BASE_URL%/}/$KEY"

AWS_ARGS=(--endpoint-url "$R2_ENDPOINT")
if [[ -n "$AWS_PROFILE" ]]; then AWS_ARGS+=(--profile "$AWS_PROFILE"); fi
aws s3 cp "$DIST_PATH" "s3://$R2_BUCKET/$KEY" "${AWS_ARGS[@]}" --cache-control "$LONG_CACHE_CONTROL"
curl --fail --location --silent --show-error --head "$URL" >/dev/null

node scripts/shared/release-state.mjs version runtime "$VERSION" "$PLATFORM"
node scripts/shared/release-state.mjs artifact runtime "$VERSION" "$PLATFORM" "$URL" "$SHA256"
git add versions.toml src-tauri/resources/runtime-manifest.json
git commit -m "Release mash-cv runtime $VERSION ($PLATFORM)"
git tag "$TAG"

echo "Released mash-cv runtime $VERSION ($PLATFORM)"
echo "  artifact: $URL"
echo "  sha256:   $SHA256"
