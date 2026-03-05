#!/usr/bin/env bash
set -e

REPO="wei6bin/cc-speedy"
BIN_DIR="${BIN_DIR:-/usr/local/bin}"

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"

case "${OS}-${ARCH}" in
  Linux-x86_64)   TARGET="x86_64-unknown-linux-musl" ;;
  Darwin-arm64)   TARGET="aarch64-apple-darwin" ;;
  Darwin-x86_64)  TARGET="x86_64-apple-darwin" ;;
  *)
    echo "Unsupported platform: ${OS}-${ARCH}"
    exit 1
    ;;
esac

# Get latest release tag
TAG=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\(.*\)".*/\1/')

echo "Installing cc-speedy ${TAG} for ${TARGET}..."

URL="https://github.com/${REPO}/releases/download/${TAG}/cc-speedy-${TARGET}.tar.gz"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

curl -fsSL "$URL" | tar xz -C "$TMP"
chmod +x "$TMP/cc-speedy-${TARGET}"

if [ -w "$BIN_DIR" ]; then
  mv "$TMP/cc-speedy-${TARGET}" "$BIN_DIR/cc-speedy"
else
  sudo mv "$TMP/cc-speedy-${TARGET}" "$BIN_DIR/cc-speedy"
fi

echo "Installed to $BIN_DIR/cc-speedy"
echo "Registering SessionEnd hook..."
cc-speedy install
echo "Done. Run: cc-speedy"
