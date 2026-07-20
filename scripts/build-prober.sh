#!/usr/bin/env bash
# Build the chaos-prober Go binary and place it at third_party/chaos-prober.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$SCRIPT_DIR/.."
PROBER_SRC="$ROOT/tools/chaos-prober"
OUT="$ROOT/third_party/chaos-prober"

if ! command -v go &>/dev/null; then
  echo "chaos-prober: Go not found, skipping build" >&2
  exit 0
fi

echo "Building chaos-prober..."
cd "$PROBER_SRC"
go build -o "$OUT" .
echo "chaos-prober built → $OUT ($(du -h "$OUT" | cut -f1))"
