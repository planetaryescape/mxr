#!/usr/bin/env bash

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
gate="${root}/scripts/pre-pr-rust-gate"

expect_map() {
  local label="$1"
  local expected="$2"
  local actual
  actual="$(printf '%s\n' "${@:3}" | bash "${gate}" --map | paste -sd, -)"
  if [[ "${actual}" != "${expected}" ]]; then
    echo "expected [${expected}] got [${actual}]: ${label}" >&2
    return 1
  fi
}

expect_map "crate source maps to its package" "mxr-llm" \
  crates/llm/src/lib.rs
expect_map "crate manifest maps to its package" "mxr-llm" \
  crates/llm/Cargo.toml
expect_map "root sources map to the mxr binary crate" "mxr" \
  src/main.rs tests/cli_help.rs benches/search.rs
expect_map "workspace lock change must still build the shipped binary" "mxr" \
  Cargo.lock
expect_map "several crates dedupe and sort" "mxr,mxr-deliveries,mxr-llm" \
  crates/llm/src/lib.rs crates/deliveries/src/extract.rs src/main.rs crates/llm/src/background.rs
expect_map "docs, scripts, web app and examples map to nothing" "" \
  docs/blueprint/17-release-pipeline.md scripts/pre-pr-rust-gate apps/web/src/app.tsx \
  examples/adapter-skeleton/src/lib.rs .github/workflows/ci.yml
expect_map "unknown crate directory is ignored" "" \
  crates/does-not-exist/src/lib.rs

echo "pre-pr-rust-gate tests passed"
