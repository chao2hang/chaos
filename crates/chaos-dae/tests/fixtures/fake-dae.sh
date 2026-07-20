#!/usr/bin/env bash
# Mock dae binary for chaos-dae tests (no eBPF).
echo "fake-dae $*" >> "${FAKE_DAE_LOG:-/tmp/fake-dae.log}"
if [[ "${1:-}" == "validate" ]]; then
  exit 0
fi
exit 0
