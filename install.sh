#!/usr/bin/env bash
set -euo pipefail

REPO="${MXR_REPO:-planetaryescape/mxr}"
VERSION="${1:-latest}"
INSTALL_DIR="${MXR_INSTALL_DIR:-$HOME/.local/bin}"

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
  Darwin) platform="macos" ;;
  Linux) platform="linux" ;;
  *)
    echo "Unsupported OS: $os" >&2
    exit 1
    ;;
esac

case "$arch" in
  arm64|aarch64) target_arch="aarch64" ;;
  x86_64|amd64) target_arch="x86_64" ;;
  *)
    echo "Unsupported architecture: $arch" >&2
    exit 1
    ;;
esac

if [[ "$VERSION" == "latest" ]]; then
  archive="mxr-latest-${platform}-${target_arch}.tar.gz"
  url="https://github.com/${REPO}/releases/latest/download/${archive}"
else
  version="${VERSION#v}"
  archive="mxr-v${version}-${platform}-${target_arch}.tar.gz"
  url="https://github.com/${REPO}/releases/download/v${version}/${archive}"
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

echo "Downloading $url"
curl -fsSL "$url" -o "$tmp_dir/$archive"
tar -xzf "$tmp_dir/$archive" -C "$tmp_dir"

mkdir -p "$INSTALL_DIR"
install "$tmp_dir/mxr" "$INSTALL_DIR/mxr"

echo "Installed mxr to $INSTALL_DIR/mxr"
echo "Run: $INSTALL_DIR/mxr version"
