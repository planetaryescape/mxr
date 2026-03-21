#!/usr/bin/env bash

set -euo pipefail

require_token() {
  if [[ -z "${CARGO_REGISTRY_TOKEN:-}" ]]; then
    echo "CARGO_REGISTRY_TOKEN is required" >&2
    exit 1
  fi
}

publish_or_skip_existing() {
  local logfile
  logfile="$(mktemp)"

  if "$@" 2>&1 | tee "${logfile}"; then
    rm -f "${logfile}"
    return 0
  fi

  if grep -q "already exists on crates.io index" "${logfile}"; then
    rm -f "${logfile}"
    return 0
  fi

  cat "${logfile}" >&2
  rm -f "${logfile}"
  return 1
}

publish_async_imap() {
  local crate="mxr-async-imap"
  local version="0.10.5"
  local tmpdir

  tmpdir="$(mktemp -d)"
  rsync -a \
    --exclude 'Cargo.toml.orig' \
    --exclude '.cargo_vcs_info.json' \
    vendor/async-imap/ "${tmpdir}/"

  echo "Publishing ${crate}..."
  (
    cd "${tmpdir}"
    publish_or_skip_existing cargo publish --locked
  )
  wait_for_crate "${crate}" "${version}"
}

wait_for_crate() {
  return 0
}

publish() {
  local crate="$1"
  local version="$2"

  echo "Publishing ${crate}..."
  publish_or_skip_existing cargo publish -p "${crate}" --locked
  wait_for_crate "${crate}" "${version}"
}

require_token

publish_async_imap
publish mxr-core 0.3.1
publish mxr-protocol 0.3.1
publish mxr-config 0.3.1
publish mxr-test-support 0.3.1
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
