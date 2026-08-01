#!/usr/bin/env bash
# Build a self-contained Debian package (and FHS tar.gz) for chaos.
#
# Prerequisites:
#   - Rust toolchain (cargo)
#   - Go toolchain (for the bundled chaos-prober latency tester)
#   - Node.js 20+ with pnpm
#   - dae binary: CHAOS_DAE_ARCH=... ./scripts/fetch-dae.sh
#
# Usage:
#   ./packaging/debian/build.sh              # native arch
#   CHAOS_ARCH=amd64 ./packaging/debian/build.sh
#   CHAOS_ARCH=arm64 ./packaging/debian/build.sh   # requires aarch64 runner or target
#
# Output:
#   dist/chaos_<version>_<arch>.deb
#   dist/chaos_<version>_linux_<arch>.tar.gz
#   dist/SHA256SUMS (appended)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
DIST_DIR="$ROOT_DIR/dist"
VERSION="${CHAOS_VERSION:-0.1.7}"

# Normalize architecture: debian name (amd64|arm64)
HOST_DEB="$(dpkg --print-architecture 2>/dev/null || true)"
if [[ -z "$HOST_DEB" ]]; then
  case "$(uname -m)" in
    x86_64|amd64) HOST_DEB=amd64 ;;
    aarch64|arm64) HOST_DEB=arm64 ;;
    *) HOST_DEB=amd64 ;;
  esac
fi
ARCH="${CHAOS_ARCH:-$HOST_DEB}"
case "$ARCH" in
  amd64|x86_64) ARCH=amd64; DAE_ARCH=x86_64; RUST_TARGET=x86_64-unknown-linux-gnu ;;
  arm64|aarch64) ARCH=arm64; DAE_ARCH=arm64; RUST_TARGET=aarch64-unknown-linux-gnu ;;
  *)
    echo "error: unsupported CHAOS_ARCH=$ARCH (use amd64 or arm64)" >&2
    exit 1
    ;;
esac

PKG_ROOT="$DIST_DIR/pkg-root"
PKG_NAME="chaos"
PKG_DIR="$PKG_ROOT/${PKG_NAME}_${VERSION}_${ARCH}"

echo "==> Building chaos $VERSION ($ARCH / rust $RUST_TARGET)"

rm -rf "$PKG_DIR"
mkdir -p "$DIST_DIR" "$PKG_DIR"

# --- Step 1: Build Rust binary ---
cd "$ROOT_DIR"
if [[ "$ARCH" == "$HOST_DEB" ]]; then
  echo "==> Compiling chaos-api (release, native)..."
  cargo build --release -p chaos-api
  API_BIN="$ROOT_DIR/target/release/chaos-api"
else
  echo "==> Compiling chaos-api (release, --target $RUST_TARGET)..."
  if ! rustup target list --installed 2>/dev/null | grep -qx "$RUST_TARGET"; then
    if command -v rustup >/dev/null 2>&1; then
      rustup target add "$RUST_TARGET"
    else
      echo "error: rust target $RUST_TARGET not installed and rustup unavailable" >&2
      exit 1
    fi
  fi
  cargo build --release -p chaos-api --target "$RUST_TARGET"
  API_BIN="$ROOT_DIR/target/${RUST_TARGET}/release/chaos-api"
fi

if command -v strip >/dev/null 2>&1; then
  strip --strip-unneeded "$API_BIN" || true
fi

# --- Step 2: Build web assets ---
echo "==> Building web assets..."
cd "$ROOT_DIR/apps/web"
pnpm install --frozen-lockfile
pnpm build

# --- Step 3: Build chaos-prober (real proxy latency tester) ---
echo "==> Building chaos-prober ($ARCH)..."
if ! command -v go >/dev/null 2>&1; then
  echo "error: Go toolchain is required to build chaos-prober" >&2
  exit 1
fi
case "$ARCH" in
  amd64) PROBER_GOARCH=amd64 ;;
  arm64) PROBER_GOARCH=arm64 ;;
esac
CHAOS_PROBER_OUT="$ROOT_DIR/third_party/chaos-prober" CHAOS_PROBER_GOARCH="$PROBER_GOARCH" \
  "$ROOT_DIR/scripts/build-prober.sh"

# --- Step 4: Resolve dae binary ---
DAE_CANDIDATES=(
  "$ROOT_DIR/third_party/dae/current/${DAE_ARCH}/dae"
  "$ROOT_DIR/third_party/dae/current/dae"
)
DAE_BIN=""
for candidate in "${DAE_CANDIDATES[@]}"; do
  if [[ -f "$candidate" ]]; then
    DAE_BIN="$candidate"
    break
  fi
done
if [[ -z "$DAE_BIN" ]]; then
  echo "==> dae binary missing; fetching ${DAE_ARCH}..."
  CHAOS_DAE_ARCH="$DAE_ARCH" "$ROOT_DIR/scripts/fetch-dae.sh" || true
  for candidate in "${DAE_CANDIDATES[@]}"; do
    if [[ -f "$candidate" ]]; then
      DAE_BIN="$candidate"
      break
    fi
  done
fi

# --- Step 5: Assemble package tree ---
echo "==> Assembling package..."

install -Dm755 "$API_BIN" "$PKG_DIR/usr/lib/chaos/bin/chaos-api"

if [[ -n "$DAE_BIN" ]]; then
  install -Dm755 "$DAE_BIN" "$PKG_DIR/usr/lib/chaos/bin/dae"
else
  echo "WARNING: dae binary not found. Package will report dae_binary_missing at runtime."
fi

install -Dm755 "$ROOT_DIR/third_party/chaos-prober" "$PKG_DIR/usr/lib/chaos/bin/chaos-prober"

mkdir -p "$PKG_DIR/usr/share/chaos/web"
cp -r "$ROOT_DIR/apps/web/build/." "$PKG_DIR/usr/share/chaos/web/"

mkdir -p "$PKG_DIR/usr/share/chaos/locales"
cp "$ROOT_DIR/locales/"*.json "$PKG_DIR/usr/share/chaos/locales/"

install -Dm644 "$SCRIPT_DIR/chaos.service" "$PKG_DIR/lib/systemd/system/chaos.service"
install -Dm644 "$SCRIPT_DIR/chaos.env" "$PKG_DIR/etc/chaos/chaos.env"

# Placeholder for state dir ownership (created by postinst / systemd StateDirectory)
mkdir -p "$PKG_DIR/var/lib/chaos"

mkdir -p "$PKG_DIR/DEBIAN"
cat > "$PKG_DIR/DEBIAN/control" <<EOF
Package: $PKG_NAME
Version: $VERSION
Architecture: $ARCH
Maintainer: chaos <chaos@localhost>
Depends: libc6, ca-certificates
Description: Modern control plane for dae
 Chaos is a modern proxy control plane: Rust REST API + SvelteKit console
 + vendored dae data plane. Full replacement for daed as an installable product.
EOF

cat > "$PKG_DIR/DEBIAN/conffiles" <<'EOF'
/etc/chaos/chaos.env
EOF

cat > "$PKG_DIR/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
mkdir -p /var/lib/chaos/dae /var/lib/chaos/backups
chmod 700 /var/lib/chaos || true

# Containers / cloud images often have no systemd as PID 1.
have_systemd=0
if command -v systemctl >/dev/null 2>&1; then
  if [ -d /run/systemd/system ] || [ "$(cat /proc/1/comm 2>/dev/null)" = "systemd" ]; then
    have_systemd=1
  fi
fi

if [ "$have_systemd" = 1 ]; then
  systemctl daemon-reload || true
  systemctl enable chaos.service || true
  echo "chaos installed. Start with: sudo systemctl start chaos"
else
  echo "chaos installed (no systemd detected)."
  echo "Start manually:"
  echo "  set -a; . /etc/chaos/chaos.env; set +a"
  echo "  /usr/lib/chaos/bin/chaos-api"
fi
echo "Dashboard: http://127.0.0.1:2030"
EOF
chmod 755 "$PKG_DIR/DEBIAN/postinst"

cat > "$PKG_DIR/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
if command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ]; then
  systemctl stop chaos.service 2>/dev/null || true
  systemctl disable chaos.service 2>/dev/null || true
fi
EOF
chmod 755 "$PKG_DIR/DEBIAN/prerm"

cat > "$PKG_DIR/DEBIAN/postrm" <<'EOF'
#!/bin/sh
set -e
if [ "$1" = "purge" ]; then
  if command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ]; then
    systemctl daemon-reload 2>/dev/null || true
  fi
  echo "Note: /var/lib/chaos was left in place. Remove manually if desired."
fi
EOF
chmod 755 "$PKG_DIR/DEBIAN/postrm"

# --- Step 6: Build .deb and FHS tar.gz ---
DEB_OUT="$DIST_DIR/${PKG_NAME}_${VERSION}_${ARCH}.deb"
TAR_OUT="$DIST_DIR/${PKG_NAME}_${VERSION}_linux_${ARCH}.tar.gz"

if command -v dpkg-deb &>/dev/null; then
  echo "==> Building .deb..."
  dpkg-deb --build --root-owner-group "$PKG_DIR" "$DEB_OUT"
  echo "==> Done: $DEB_OUT"
else
  echo "WARNING: dpkg-deb not found; skipping .deb"
fi

echo "==> Building FHS tar.gz..."
# Archive root is FHS paths (./usr/..., ./etc/...) so tar -xzf ... -C / works.
# Exclude DEBIAN control metadata (deb-only).
tar -czf "$TAR_OUT" -C "$PKG_DIR" --exclude=DEBIAN .
echo "==> Done: $TAR_OUT"

(
  cd "$DIST_DIR"
  : > SHA256SUMS.tmp
  for f in "${PKG_NAME}_${VERSION}_${ARCH}.deb" "${PKG_NAME}_${VERSION}_linux_${ARCH}.tar.gz"; do
    if [[ -f "$f" ]]; then
      sha256sum "$f" >> SHA256SUMS.tmp
    fi
  done
  if [[ -f SHA256SUMS ]]; then
    # Drop previous lines for this arch/version, keep others.
    grep -v " ${PKG_NAME}_${VERSION}_${ARCH}\.deb\$" SHA256SUMS \
      | grep -v " ${PKG_NAME}_${VERSION}_linux_${ARCH}\.tar\.gz\$" \
      > SHA256SUMS.keep 2>/dev/null || true
    cat SHA256SUMS.keep SHA256SUMS.tmp > SHA256SUMS 2>/dev/null || cat SHA256SUMS.tmp > SHA256SUMS
    rm -f SHA256SUMS.keep SHA256SUMS.tmp
  else
    mv SHA256SUMS.tmp SHA256SUMS
  fi
  echo "==> Checksums written to dist/SHA256SUMS"
)

echo ""
echo "Install (deb): sudo dpkg -i $DEB_OUT"
echo "Install (tar): sudo tar -xzf $TAR_OUT -C /"
echo "Start:         sudo systemctl enable --now chaos"
