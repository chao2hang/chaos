#!/usr/bin/env bash
# Build the chaos-prober Go binary.
# Default output: third_party/chaos-prober for development.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PROBER_SRC="$ROOT/tools/chaos-prober"
OUT="${CHAOS_PROBER_OUT:-$ROOT/third_party/chaos-prober}"
GOOS="${CHAOS_PROBER_GOOS:-linux}"
GOARCH="${CHAOS_PROBER_GOARCH:-$(go env GOARCH 2>/dev/null || true)}"

if ! command -v go >/dev/null 2>&1; then
  echo "chaos-prober: Go not found, skipping build" >&2
  exit 0
fi

if [[ -z "$GOARCH" ]]; then
  case "$(uname -m)" in
    x86_64|amd64) GOARCH=amd64 ;;
    aarch64|arm64) GOARCH=arm64 ;;
    *) echo "chaos-prober: unsupported machine architecture" >&2; exit 1 ;;
  esac
fi

echo "Building chaos-prober for $GOOS/$GOARCH..."
mkdir -p "$(dirname "$OUT")"
(
  cd "$PROBER_SRC"
  CGO_ENABLED=0 GOOS="$GOOS" GOARCH="$GOARCH" go build -trimpath -ldflags="-s -w" -o "$OUT" .
)
echo "chaos-prober built → $OUT ($(du -h "$OUT" | cut -f1))"
