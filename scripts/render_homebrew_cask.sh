#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <version> <artifacts-dir> <output-cask>" >&2
  exit 1
fi

version="$1"
artifacts_dir="$2"
output_cask="$3"
template="packaging/homebrew/mxr-desktop.rb"

checksum_file="$artifacts_dir/mxr-desktop-${version}-aarch64.dmg.sha256"
if [[ ! -f "$checksum_file" ]]; then
  echo "missing checksum file: $checksum_file" >&2
  exit 1
fi
sha="$(awk '{print $1}' "$checksum_file")"
if [[ ! "$sha" =~ ^[0-9a-f]{64}$ ]]; then
  echo "not a sha256 in $checksum_file: $sha" >&2
  exit 1
fi

mkdir -p "$(dirname "$output_cask")"

sed \
  -e "s/__VERSION__/${version}/g" \
  -e "s/__SHA256_MACOS_AARCH64__/${sha}/g" \
  "$template" > "$output_cask"
