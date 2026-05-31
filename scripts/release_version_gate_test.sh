#!/usr/bin/env bash

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
gate="${root}/scripts/release_version_gate.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

write_manifest() {
  local version="$1"
  cat > "${tmp}/Cargo.toml" <<EOF
[workspace.package]
version = "${version}"
EOF
}

expect_ok() {
  local label="$1"
  shift
  if ! bash "${gate}" "$@" "${tmp}/Cargo.toml" >"${tmp}/out" 2>&1; then
    echo "expected ok: ${label}" >&2
    cat "${tmp}/out" >&2
    return 1
  fi
}

expect_fail() {
  local label="$1"
  shift
  if bash "${gate}" "$@" "${tmp}/Cargo.toml" >"${tmp}/out" 2>&1; then
    echo "expected failure: ${label}" >&2
    cat "${tmp}/out" >&2
    return 1
  fi
}

write_manifest "1.2.3"
expect_ok "tag matches Cargo workspace version" v1.2.3
expect_fail "tag version differs from Cargo workspace version" v1.2.4
expect_fail "release ref must be a version tag" main

write_manifest "2.0.0-beta.1"
expect_ok "pre-release tag matches Cargo workspace version" v2.0.0-beta.1

cat > "${tmp}/Cargo.toml" <<'EOF'
[workspace]
members = []
EOF
expect_fail "workspace package version is required" v1.2.3

echo "release_version_gate_test: ok"
