#!/usr/bin/env bash
# Build chaos packages inside Docker for a target arch (amd64 or arm64).
# Requires: docker with buildx/binfmt for cross-arch (arm64 on x86_64 hosts).
#
# Usage:
#   ./packaging/debian/build-in-docker.sh arm64
#   ./packaging/debian/build-in-docker.sh amd64
#   CHAOS_VERSION=0.1.0 ./packaging/debian/build-in-docker.sh arm64
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
ARCH="${1:-arm64}"
VERSION="${CHAOS_VERSION:-0.1.3}"

case "$ARCH" in
  amd64|x86_64) ARCH=amd64; PLATFORM=linux/amd64; DAE_ARCH=x86_64 ;;
  arm64|aarch64) ARCH=arm64; PLATFORM=linux/arm64; DAE_ARCH=arm64 ;;
  *)
    echo "usage: $0 amd64|arm64" >&2
    exit 1
    ;;
esac

if ! command -v docker >/dev/null 2>&1; then
  echo "error: docker is required" >&2
  exit 1
fi

echo "==> Fetching dae ${DAE_ARCH} on host (if network available)..."
if [[ -x "$ROOT_DIR/scripts/fetch-dae.sh" ]]; then
  CHAOS_DAE_ARCH="$DAE_ARCH" "$ROOT_DIR/scripts/fetch-dae.sh" || {
    echo "warning: fetch-dae failed; package may lack dae binary" >&2
  }
fi

IMAGE="rust:1.83-slim"
echo "==> Building package in $IMAGE ($PLATFORM) as chaos $VERSION ($ARCH)"

docker run --rm --platform "$PLATFORM" \
  -v "$ROOT_DIR:/src" \
  -w /src \
  -e CHAOS_VERSION="$VERSION" \
  -e CHAOS_ARCH="$ARCH" \
  -e CARGO_HOME=/src/.cargo-target/docker-cargo-home \
  -e CARGO_TARGET_DIR=/src/.cargo-target/docker-"$ARCH" \
  "$IMAGE" \
  bash -lc '
    set -euo pipefail
    apt-get update
    apt-get install -y --no-install-recommends \
      pkg-config libssl-dev ca-certificates curl unzip dpkg-dev build-essential \
      nodejs npm
    # Node 20+ preferred; if distro node is old, use corepack/npx pnpm via npm
    npm install -g pnpm@10
    # Ensure workspace deps
    if [[ ! -f pnpm-lock.yaml ]]; then
      echo "error: missing pnpm-lock.yaml" >&2
      exit 1
    fi
    # Use packaging build script (native arch inside container == target arch)
    chmod +x packaging/debian/build.sh scripts/fetch-dae.sh || true
    # Prefer already-fetched dae from host bind-mount
    if [[ ! -f third_party/dae/current/'"$DAE_ARCH"'/dae && ! -f third_party/dae/current/dae ]]; then
      CHAOS_DAE_ARCH='"$DAE_ARCH"' ./scripts/fetch-dae.sh || true
    fi
    CHAOS_VERSION="$CHAOS_VERSION" CHAOS_ARCH="$CHAOS_ARCH" ./packaging/debian/build.sh
  '

echo ""
echo "==> Host artifacts:"
ls -lh "$ROOT_DIR/dist"/chaos_"${VERSION}"*"${ARCH}"* 2>/dev/null || ls -lh "$ROOT_DIR/dist" | tail -20
file "$ROOT_DIR/dist/pkg-root/chaos_${VERSION}_${ARCH}/usr/lib/chaos/bin/chaos-api" 2>/dev/null || true
