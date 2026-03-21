#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <version> <artifacts-dir> <output-formula>" >&2
  exit 1
fi

version="$1"
artifacts_dir="$2"
output_formula="$3"
template="packaging/homebrew/mxr.rb"

require_checksum() {
  local archive="$1"
  local file="$artifacts_dir/$archive.sha256"
  if [[ ! -f "$file" ]]; then
    echo "missing checksum file: $file" >&2
    exit 1
  fi
  awk '{print $1}' "$file"
}

macos_aarch64_sha="$(require_checksum "mxr-v${version}-macos-aarch64.tar.gz")"
macos_x86_64_sha="$(require_checksum "mxr-v${version}-macos-x86_64.tar.gz")"
linux_x86_64_sha="$(require_checksum "mxr-v${version}-linux-x86_64.tar.gz")"

mkdir -p "$(dirname "$output_formula")"

sed \
  -e "s/__VERSION__/${version}/g" \
  -e "s/__SHA256_MACOS_AARCH64__/${macos_aarch64_sha}/g" \
  -e "s/__SHA256_MACOS_X86_64__/${macos_x86_64_sha}/g" \
  -e "s/__SHA256_LINUX_X86_64__/${linux_x86_64_sha}/g" \
  "$template" > "$output_formula"
