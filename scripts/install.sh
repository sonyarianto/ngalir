#!/usr/bin/env bash
set -euo pipefail

NGALIR_VERSION="${NGALIR_VERSION:-latest}"
NGALIR_REPO="${NGALIR_REPO:-sonyarianto/ngalir}"

if [ "$NGALIR_VERSION" = "latest" ]; then
  NGALIR_TAG=$(curl -sSL "https://api.github.com/repos/$NGALIR_REPO/releases/latest" | grep '"tag_name"' | cut -d'"' -f4)
else
  NGALIR_TAG="$NGALIR_VERSION"
fi

ARCH=$(uname -m)
OS=$(uname -s | tr '[:upper:]' '[:lower:]')

case "$ARCH" in
  x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
  aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
  arm64)   TARGET="aarch64-unknown-linux-gnu" ;;
  *)       echo "unsupported architecture: $ARCH"; exit 1 ;;
esac

if [ "$OS" != "linux" ]; then
  echo "unsupported OS: $OS (only Linux is supported for binary install)"
  exit 1
fi

DOWNLOAD_URL="https://github.com/$NGALIR_REPO/releases/download/$NGALIR_TAG/ngalir-$NGALIR_TAG-$TARGET.tar.gz"

echo "Downloading ngalir $NGALIR_TAG ($TARGET)..."
curl -sSL "$DOWNLOAD_URL" -o /tmp/ngalir.tar.gz

INSTALL_DIR="${NGALIR_INSTALL_DIR:-/usr/local/bin}"
sudo mkdir -p "$INSTALL_DIR"
sudo tar xzf /tmp/ngalir.tar.gz -C "$INSTALL_DIR"
rm /tmp/ngalir.tar.gz

echo "ngalir $NGALIR_TAG installed to $INSTALL_DIR"
echo "Run 'ngalir --help' to get started."
