#!/usr/bin/env bash

set -euo pipefail

if [[ "$#" -ne 3 ]]; then
  echo "usage: $0 <event-name> <base-ref> <head-ref>" >&2
  exit 2
fi

event_name="$1"
base="$2"
head="$3"

rust=false
web=false
docs=false
expensive=false

print_scope() {
  printf 'rust=%s\nweb=%s\ndocs=%s\nexpensive=%s\n' \
    "${rust}" "${web}" "${docs}" "${expensive}"
}

classify_path() {
  local path="$1"

  case "${path}" in
    Cargo.toml|Cargo.lock|rust-toolchain*|build.rs|crates/*|src/*|.sqlx/*)
      rust=true
      ;;
    deny.toml|tests/*|benches/*|examples/*|scripts/test_quality_audit.sh|scripts/test_quality_audit_test.sh|scripts/check_architecture_boundaries.sh|scripts/cargo-test|scripts/cargo_test_wrapper_test.sh)
      rust=true
      ;;
  esac

  case "${path}" in
    apps/web/*|crates/web/*|crates/protocol/*)
      web=true
      ;;
  esac

  case "${path}" in
    site/*)
      docs=true
      ;;
  esac

  case "${path}" in
    .github/workflows/ci.yml|scripts/ci_change_scope.sh|scripts/ci_change_scope_test.sh)
      rust=true
      web=true
      docs=true
      ;;
  esac
}

if [[ "${event_name}" == "workflow_dispatch" ]]; then
  rust=true
  web=true
  docs=true
  expensive=true
  print_scope
  exit 0
fi

if ! git rev-parse --verify --quiet "${head}^{commit}" >/dev/null; then
  echo "ci_change_scope: invalid head ref: ${head}" >&2
  exit 1
fi

if [[ -z "${base}" || "${base}" =~ ^0+$ ]]; then
  rust=true
  web=true
  docs=true
  print_scope
  exit 0
fi

if ! git rev-parse --verify --quiet "${base}^{commit}" >/dev/null; then
  echo "ci_change_scope: invalid base ref: ${base}" >&2
  exit 1
fi

changes_file="$(mktemp)"
trap 'rm -f "${changes_file}"' EXIT

if ! git diff --name-status -z --find-renames "${base}" "${head}" -- > "${changes_file}"; then
  echo "ci_change_scope: git diff failed for ${base}..${head}" >&2
  exit 1
fi

while IFS= read -r -d '' status; do
  case "${status}" in
    R*|C*)
      IFS= read -r -d '' old_path
      IFS= read -r -d '' new_path
      classify_path "${old_path}"
      classify_path "${new_path}"
      ;;
    *)
      IFS= read -r -d '' path
      classify_path "${path}"
      ;;
  esac
done < "${changes_file}"

print_scope
