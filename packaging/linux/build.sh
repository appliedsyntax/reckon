#!/usr/bin/env bash
# Builds Linux packages (run on Linux):
#   dist/reckon-<v>-linux-<arch>.tar.gz   portable archive
#   dist/reckon_<v>_<debarch>.deb         Debian/Ubuntu package (if dpkg-deb exists)
#
# Build dependencies (Debian/Ubuntu):
#   sudo apt install build-essential pkg-config libxkbcommon-dev libxkbcommon-x11-dev \
#     libwayland-dev libvulkan-dev libfontconfig-dev libfreetype-dev libx11-xcb-dev \
#     libxcb1-dev libssl-dev libzstd-dev
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
ARCH="$(uname -m)"
OUT="$ROOT/dist"
WORK="$ROOT/target/package-linux"
rm -rf "$WORK" && mkdir -p "$WORK" "$OUT"

cargo build --release -p reckon-app -p reckon-cli

install_tree() { # $1 = prefix root
  install -Dm755 target/release/reckon "$1/bin/reckon"
  install -Dm755 target/release/reckon-app "$1/bin/reckon-app"
  install -Dm644 packaging/linux/reckon.desktop "$1/share/applications/reckon.desktop"
  install -Dm644 packaging/linux/reckon.png "$1/share/icons/hicolor/256x256/apps/reckon.png"
  install -Dm644 LICENSE "$1/share/doc/reckon/LICENSE"
}

# Portable tarball
NAME="reckon-${VERSION}-linux-${ARCH}"
install_tree "$WORK/$NAME"
cp README.md "$WORK/$NAME/"
tar -C "$WORK" -czf "$OUT/$NAME.tar.gz" "$NAME"
echo "$OUT/$NAME.tar.gz"

# Debian package
if command -v dpkg-deb >/dev/null; then
  DEBARCH="$(dpkg --print-architecture)"
  PKG="$WORK/deb"
  install_tree "$PKG/usr"
  mkdir -p "$PKG/DEBIAN"
  cat > "$PKG/DEBIAN/control" <<EOF
Package: reckon
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${DEBARCH}
Depends: libc6, libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libvulkan1, libfontconfig1, libfreetype6, libx11-xcb1, libxcb1
Maintainer: The Reckon Authors <noreply@example.invalid>
Description: Natural-language calculator
 Write math in plain words and see results next to each line. Includes the
 Reckon desktop app (reckon-app) and command-line tool (reckon).
EOF
  dpkg-deb --build --root-owner-group "$PKG" "$OUT/reckon_${VERSION}_${DEBARCH}.deb"
  echo "$OUT/reckon_${VERSION}_${DEBARCH}.deb"
fi
