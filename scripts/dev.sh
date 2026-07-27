#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
mkdir -p data

# Build chaos-prober if Go is available (enables real proxy latency tests).
bash scripts/build-prober.sh || true

# Stop a previous local chaos-api if still running.
pkill -f 'target/.*/chaos-api' 2>/dev/null || true

cargo build -p chaos-api
cargo run -p chaos-api &
API_PID=$!
trap 'kill $API_PID 2>/dev/null || true' EXIT

pnpm --dir apps/web dev --port 5173
