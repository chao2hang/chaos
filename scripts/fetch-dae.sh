#!/usr/bin/env bash
# Fetch the pinned dae release binary into third_party/dae/current[/arch]/dae
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [[ "$(uname -s)" != "Linux" ]]; then
  echo "error: dae is a Linux-only data plane, and chaos targets Linux only" >&2
  exit 1
fi

VERSION_FILE="$ROOT/third_party/dae/VERSION"

if [[ ! -f "$VERSION_FILE" ]]; then
  echo "error: missing $VERSION_FILE" >&2
  exit 1
fi

VER="$(tr -d '[:space:]' < "$VERSION_FILE")"
if [[ -z "$VER" ]]; then
  echo "error: empty version in $VERSION_FILE" >&2
  exit 1
fi

# Prefer explicit override for multi-arch packaging, else host arch.
host_arch="${CHAOS_DAE_ARCH:-$(uname -m)}"
case "$host_arch" in
  x86_64|amd64) arch="x86_64" ;;
  aarch64|arm64) arch="arm64" ;;
  armv7l|armv7) arch="armv7" ;;
  armv6l|armv6) arch="armv6" ;;
  i386|i686|x86) arch="x86_32" ;;
  riscv64) arch="riscv64" ;;
  loongarch64) arch="loongarch64" ;;
  s390x) arch="s390x" ;;
  ppc64|powerpc64) arch="powerpc64" ;;
  ppc64le|powerpc64le) arch="powerpc64le" ;;
  *)
    echo "error: unsupported architecture: $host_arch" >&2
    exit 1
    ;;
esac

# Layout:
#   third_party/dae/current/dae              (compat default for native host)
#   third_party/dae/current/<arch>/dae       (side-by-side multi-arch cache)
if [[ -n "${CHAOS_DAE_DEST_DIR:-}" ]]; then
  DEST_DIR="$CHAOS_DAE_DEST_DIR"
elif [[ -n "${CHAOS_DAE_ARCH:-}" ]]; then
  DEST_DIR="$ROOT/third_party/dae/current/${arch}"
else
  DEST_DIR="$ROOT/third_party/dae/current"
fi
DEST_BIN="$DEST_DIR/dae"

asset="dae-linux-${arch}.zip"
base_url="https://github.com/daeuniverse/dae/releases/download/${VER}"
url="${base_url}/${asset}"
dgst_url="${url}.dgst"

tmpdir="$(mktemp -d)"
cleanup() { rm -rf "$tmpdir"; }
trap cleanup EXIT

zip_path="${tmpdir}/${asset}"
dgst_path="${tmpdir}/${asset}.dgst"
extract_dir="${tmpdir}/extract"

echo "Fetching dae ${VER} (${arch})..."
curl -fsSL -o "$zip_path" "$url"
curl -fsSL -o "$dgst_path" "$dgst_url"

# dgst format: "<hex>  <filename>  <algo>"
expected_sha="$(awk '$3 == "sha256" { print $1; exit }' "$dgst_path")"
if [[ -z "$expected_sha" ]]; then
  echo "error: no sha256 entry in $dgst_url" >&2
  exit 1
fi

actual_sha="$(sha256sum "$zip_path" | awk '{ print $1 }')"
if [[ "$actual_sha" != "$expected_sha" ]]; then
  echo "error: sha256 mismatch for ${asset}" >&2
  echo "  expected: $expected_sha" >&2
  echo "  actual:   $actual_sha" >&2
  exit 1
fi
echo "sha256 ok: $actual_sha"

mkdir -p "$extract_dir"
unzip -q -o "$zip_path" -d "$extract_dir"

# Binary is named dae-linux-<arch> inside the zip.
src_bin="${extract_dir}/dae-linux-${arch}"
if [[ ! -f "$src_bin" ]]; then
  # Fallback: first executable-looking file named dae*
  src_bin="$(find "$extract_dir" -maxdepth 1 -type f -name 'dae*' ! -name '*.service' ! -name '*.dae' | head -n 1 || true)"
fi
if [[ -z "${src_bin:-}" || ! -f "$src_bin" ]]; then
  echo "error: dae binary not found in ${asset}" >&2
  ls -la "$extract_dir" >&2 || true
  exit 1
fi

mkdir -p "$DEST_DIR"
install -m 755 "$src_bin" "$DEST_BIN"

# Also keep a host-default copy when building for the current machine arch.
if [[ -n "${CHAOS_DAE_ARCH:-}" ]]; then
  native="$(uname -m)"
  case "$native" in
    x86_64|amd64) native_arch="x86_64" ;;
    aarch64|arm64) native_arch="arm64" ;;
    *) native_arch="" ;;
  esac
  if [[ -n "$native_arch" && "$arch" == "$native_arch" ]]; then
    mkdir -p "$ROOT/third_party/dae/current"
    install -m 755 "$src_bin" "$ROOT/third_party/dae/current/dae"
  fi
fi

echo "Installed: $DEST_BIN"
"$DEST_BIN" --version 2>/dev/null || "$DEST_BIN" --help 2>&1 | head -n 5 || true
