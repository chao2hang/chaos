#!/usr/bin/env bash
# Build a self-contained Debian package for chaos.
#
# Prerequisites:
#   - Rust toolchain (cargo)
#   - Node.js 20+ with pnpm
#   - dae binary fetched: ./scripts/fetch-dae.sh
#
# Usage:
#   ./packaging/debian/build.sh
#
# Output:
#   dist/chaos_<version>_<arch>.deb

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
DIST_DIR="$ROOT_DIR/dist"
VERSION="${CHAOS_VERSION:-0.1.0}"
ARCH="$(dpkg --print-architecture 2>/dev/null || echo amd64)"

PKG_ROOT="$DIST_DIR/pkg-root"
PKG_NAME="chaos"
PKG_DIR="$PKG_ROOT/${PKG_NAME}_${VERSION}_${ARCH}"

echo "==> Building chaos $VERSION ($ARCH)"

# Clean previous build
rm -rf "$PKG_ROOT"
mkdir -p "$DIST_DIR"

# --- Step 1: Build Rust binary ---
echo "==> Compiling chaos-api (release)..."
cd "$ROOT_DIR"
cargo build --release -p chaos-api

# --- Step 2: Build web assets ---
echo "==> Building web assets..."
cd "$ROOT_DIR/apps/web"
pnpm install --frozen-lockfile
pnpm build

# --- Step 3: Assemble package tree ---
echo "==> Assembling package..."

# Binary
install -Dm755 "$ROOT_DIR/target/release/chaos-api" "$PKG_DIR/usr/lib/chaos/bin/chaos-api"

# dae binary
if [ -f "$ROOT_DIR/third_party/dae/current/dae" ]; then
    install -Dm755 "$ROOT_DIR/third_party/dae/current/dae" "$PKG_DIR/usr/lib/chaos/bin/dae"
else
    echo "WARNING: dae binary not found. Run ./scripts/fetch-dae.sh first."
    echo "         Package will be built without dae (runtime reports dae_binary_missing)."
fi

# Web assets
mkdir -p "$PKG_DIR/usr/share/chaos/web"
cp -r "$ROOT_DIR/apps/web/build/." "$PKG_DIR/usr/share/chaos/web/"

# Locales
mkdir -p "$PKG_DIR/usr/share/chaos/locales"
cp "$ROOT_DIR/locales/"*.json "$PKG_DIR/usr/share/chaos/locales/"

# Systemd service
install -Dm644 "$SCRIPT_DIR/chaos.service" "$PKG_DIR/lib/systemd/system/chaos.service"

# Default config
install -Dm644 "$SCRIPT_DIR/chaos.env" "$PKG_DIR/etc/chaos/chaos.env"

# DEBIAN control
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

cat > "$PKG_DIR/DEBIAN/postinst" <<'EOF'
#!/bin/sh
set -e
systemctl daemon-reload
systemctl enable chaos.service || true
echo "chaos installed. Start with: sudo systemctl start chaos"
echo "Dashboard: http://127.0.0.1:2030"
EOF
chmod 755 "$PKG_DIR/DEBIAN/postinst"

cat > "$PKG_DIR/DEBIAN/prerm" <<'EOF'
#!/bin/sh
set -e
systemctl stop chaos.service 2>/dev/null || true
systemctl disable chaos.service 2>/dev/null || true
EOF
chmod 755 "$PKG_DIR/DEBIAN/prerm"

# --- Step 4: Build package ---
if command -v dpkg-deb &>/dev/null; then
    echo "==> Building .deb..."
    dpkg-deb --build --root-owner-group "$PKG_DIR" "$DIST_DIR/${PKG_NAME}_${VERSION}_${ARCH}.deb"
    echo ""
    echo "==> Done: $DIST_DIR/${PKG_NAME}_${VERSION}_${ARCH}.deb"
    echo "    Install: sudo dpkg -i $DIST_DIR/${PKG_NAME}_${VERSION}_${ARCH}.deb"
    echo "    Start:   sudo systemctl enable --now chaos"
else
    echo "==> dpkg-deb not found, building tar.gz..."
    tar -czf "$DIST_DIR/${PKG_NAME}_${VERSION}_linux_${ARCH}.tar.gz" -C "$PKG_ROOT" "${PKG_NAME}_${VERSION}_${ARCH}"
    echo ""
    echo "==> Done: $DIST_DIR/${PKG_NAME}_${VERSION}_linux_${ARCH}.tar.gz"
    echo "    Extract: sudo tar -xzf $DIST_DIR/${PKG_NAME}_${VERSION}_linux_${ARCH}.tar.gz -C /"
    echo "    Start:   sudo /usr/lib/chaos/bin/chaos-api"
fi
