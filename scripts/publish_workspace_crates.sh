#!/usr/bin/env bash

set -euo pipefail

require_token() {
  if [[ -z "${CARGO_REGISTRY_TOKEN:-}" ]]; then
    echo "CARGO_REGISTRY_TOKEN is required" >&2
    exit 1
  fi
}

workspace_version() {
  awk -F'"' '/^\[workspace.package\]/{flag=1; next} flag && /^version = /{print $2; exit}' Cargo.toml
}

sync_store_sqlx_cache() {
  if [[ -d .sqlx ]]; then
    mkdir -p crates/store/.sqlx
    rsync -a --delete .sqlx/ crates/store/.sqlx/
  fi
}

publish_or_skip_existing() {
  local logfile
  local attempt=1
  local max_attempts=5

  while true; do
    logfile="$(mktemp)"

    if "$@" 2>&1 | tee "${logfile}"; then
      rm -f "${logfile}"
      return 0
    fi

    if grep -q "already exists on crates.io index" "${logfile}"; then
      rm -f "${logfile}"
      return 0
    fi

    if grep -q "429 Too Many Requests" "${logfile}" && (( attempt < max_attempts )); then
      local retry_after
      local now
      local sleep_for

      retry_after="$(sed -nE 's/.*Please try again after (.*) and see.*/\1/p' "${logfile}" | tail -n 1)"
      rm -f "${logfile}"

      if [[ -n "${retry_after}" ]]; then
        now="$(date -u +%s)"
        sleep_for="$(( $(date -u -d "${retry_after}" +%s) - now + 5 ))"
      else
        sleep_for=75
      fi

      if (( sleep_for < 5 )); then
        sleep_for=5
      fi

      echo "crates.io rate limit hit; sleeping ${sleep_for}s before retry ${attempt}/${max_attempts}" >&2
      sleep "${sleep_for}"
      attempt=$((attempt + 1))
      continue
    fi

    cat "${logfile}" >&2
    rm -f "${logfile}"
    return 1
  done
}

ensure_async_imap_available() {
  local crate="mxr-async-imap"
  local manifest="vendor/async-imap/Cargo.toml"
  local version
  version="$(awk -F'"' '/^version = /{print $2; exit}' "${manifest}")"
  echo "Publishing ${crate}..."
  publish_or_skip_existing cargo publish --manifest-path "${manifest}" --locked
  wait_for_crate "${crate}" "${version}"
}

wait_for_crate() {
  local crate="$1"
  local version="$2"
  local attempt=1
  local max_attempts=24

  while (( attempt <= max_attempts )); do
    if cargo info "${crate}@${version}" --registry crates-io >/dev/null 2>&1; then
      return 0
    fi

    echo "waiting for ${crate}@${version} to appear in crates.io index (${attempt}/${max_attempts})" >&2
    sleep 5
    attempt=$((attempt + 1))
  done

  echo "timed out waiting for ${crate}@${version} to appear in crates.io index" >&2
  return 1
}

publish() {
  local crate="$1"
  local version="$2"

  echo "Publishing ${crate}..."
  publish_or_skip_existing cargo publish -p "${crate}" --locked
  wait_for_crate "${crate}" "${version}"
}

require_token

VERSION="$(workspace_version)"
if [[ -z "${VERSION}" ]]; then
  echo "failed to read workspace version from Cargo.toml" >&2
  exit 1
fi

sync_store_sqlx_cache

ensure_async_imap_available
publish mxr "${VERSION}"
