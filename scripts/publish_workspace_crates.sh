#!/usr/bin/env bash

set -euo pipefail

require_token() {
  if [[ -z "${CARGO_REGISTRY_TOKEN:-}" ]]; then
    echo "CARGO_REGISTRY_TOKEN is required" >&2
    exit 1
  fi
}

crate_published() {
  local crate="$1"
  local version="$2"

  cargo info "${crate}" --registry crates-io 2>/dev/null | grep -q "^version: ${version}$"
}

publish_async_imap() {
  local crate="mxr-async-imap"
  local version="0.10.5"
  local tmpdir

  if crate_published "${crate}" "${version}"; then
    echo "Skipping ${crate} ${version}; already published."
    return 0
  fi

  tmpdir="$(mktemp -d)"
  rsync -a \
    --exclude 'Cargo.toml.orig' \
    --exclude '.cargo_vcs_info.json' \
    vendor/async-imap/ "${tmpdir}/"

  echo "Publishing ${crate}..."
  (
    cd "${tmpdir}"
    cargo publish --locked
  )
  wait_for_crate "${crate}" "${version}"
}

wait_for_crate() {
  local crate="$1"
  local version="$2"

  for _ in $(seq 1 30); do
    if crate_published "${crate}" "${version}"; then
      return 0
    fi
    echo "Waiting for ${crate} ${version} to appear on crates.io..."
    sleep 10
  done

  echo "Timed out waiting for ${crate} ${version} on crates.io" >&2
  exit 1
}

publish() {
  local crate="$1"
  local version="$2"

  if crate_published "${crate}" "${version}"; then
    echo "Skipping ${crate} ${version}; already published."
    return 0
  fi

  echo "Publishing ${crate}..."
  cargo publish -p "${crate}" --locked
  wait_for_crate "${crate}" "${version}"
}

require_token

publish_async_imap
publish mxr-core 0.3.1
publish mxr-protocol 0.3.1
publish mxr-config 0.3.1
publish mxr-store 0.3.1
publish mxr-search 0.3.1
publish mxr-reader 0.3.1
publish mxr-compose 0.3.1
publish mxr-provider-fake 0.3.1
publish mxr-provider-gmail 0.3.1
publish mxr-provider-smtp 0.3.1
publish mxr-provider-imap 0.3.1
publish mxr-export 0.3.1
publish mxr-rules 0.3.1
publish mxr-sync 0.3.1
publish mxr-tui 0.3.1
publish mxr 0.3.1
