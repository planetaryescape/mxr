#!/usr/bin/env bash

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
gate="${root}/scripts/release_gmail_oauth_gate.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

expect_ok() {
  local label="$1"
  local env_file="${tmp}/${label//[^A-Za-z0-9_]/_}.env"
  shift
  if ! env -i PATH="${PATH}" GITHUB_ENV="${env_file}" "$@" bash "${gate}" >"${tmp}/out" 2>&1; then
    echo "expected ok: ${label}" >&2
    cat "${tmp}/out" >&2
    return 1
  fi
  GATE_ENV_FILE="${env_file}"
}

expect_fail() {
  local label="$1"
  shift
  if env -i PATH="${PATH}" "$@" bash "${gate}" >"${tmp}/out" 2>&1; then
    echo "expected failure: ${label}" >&2
    cat "${tmp}/out" >&2
    return 1
  fi
}

assert_build_env() {
  local expected_id="$1"
  local expected_secret="$2"
  grep -Fxq "GMAIL_CLIENT_ID=${expected_id}" "${GATE_ENV_FILE}"
  grep -Fxq "GMAIL_CLIENT_SECRET=${expected_secret}" "${GATE_ENV_FILE}"
}

expect_ok "no bundled Gmail credentials" \
  GMAIL_CLIENT_ID= GMAIL_CLIENT_SECRET=
assert_build_env "" ""

expect_fail "partial bundled Gmail credentials" \
  GMAIL_CLIENT_ID=client GMAIL_CLIENT_SECRET=

expect_ok "unverified bundled Gmail credentials are still bundled" \
  GMAIL_CLIENT_ID=client GMAIL_CLIENT_SECRET=secret
assert_build_env "client" "secret"

echo "release_gmail_oauth_gate_test: ok"
