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
apps_web=false
docs=false
expensive=false
classifier=false

print_scope() {
  printf 'rust=%s\nweb=%s\napps_web=%s\ndocs=%s\nexpensive=%s\nclassifier=%s\n' \
    "$rust" "$web" "$apps_web" "$docs" "$expensive" "$classifier"
}

classify_path() {
  local path="$1"

  case "$path" in
    site/scripts/cli-reference-examples.json)
      # CLI-help tests parse these shared examples with the real clap definition.
      rust=true
      ;;
    Cargo.toml|Cargo.lock|rust-toolchain*|build.rs|crates/*|src/*|.sqlx/*)
      rust=true
      ;;
    deny.toml|tests/*|benches/*|examples/*|scripts/test_quality_audit.sh|scripts/test_quality_audit_test.sh|scripts/check_architecture_boundaries.sh|scripts/s08c_headless_smoke.py)
      rust=true
      ;;
  esac

  # The full-set classifier eval grades the 50,000-email demo through the
  # crates that generate, sync, classify, place and grade mail, so it runs
  # when any of them changes. UI, transport and provider crates cannot move
  # an answer.
  case "$path" in
    crates/provider-fake/*|crates/daemon/*|crates/store/*|crates/protocol/*|crates/core/*|crates/todo/*|crates/records/*|crates/deliveries/*|crates/updates/*|crates/reading/*|crates/reader/*|crates/relationship/*|crates/rules/*|crates/safety/*|crates/sync/*|crates/mail-parse/*|crates/config/*|crates/llm/*|Cargo.toml|Cargo.lock|.sqlx/*)
      classifier=true
      ;;
  esac

  case "$path" in
    apps/web/*)
      apps_web=true
      web=true
      ;;
    crates/web/*|crates/protocol/*)
      web=true
      ;;
    # The Raycast extension typechecks against the web app's generated types.
    apps/raycast/*)
      web=true
      ;;
    # Web unit tests check these docs against the web keymap and TUI parity.
    site/src/content/docs/reference/keybindings.md|docs/reference/tui-keymap.json)
      web=true
      ;;
  esac

  case "$path" in
    site/*)
      docs=true
      ;;
  esac

  case "$path" in
    .github/workflows/*|scripts/ci_change_scope.sh|scripts/ci_change_scope_test.sh|scripts/ci_workflow_test.sh)
      rust=true
      web=true
      docs=true
      classifier=true
      ;;
  esac
}

if [[ "$event_name" == "workflow_dispatch" ]]; then
  rust=true
  web=true
  apps_web=true
  docs=true
  expensive=true
  classifier=true
  print_scope
  exit 0
fi

if ! git rev-parse --verify --quiet --end-of-options "${head}^{commit}" >/dev/null; then
  echo "ci_change_scope: invalid head ref: ${head}" >&2
  exit 1
fi

if [[ -z "$base" || "$base" =~ ^0+$ ]]; then
  # The first push has no usable comparison point, so run every normal gate.
  rust=true
  web=true
  apps_web=true
  docs=true
  classifier=true
  print_scope
  exit 0
fi

if ! git rev-parse --verify --quiet --end-of-options "${base}^{commit}" >/dev/null; then
  echo "ci_change_scope: invalid base ref: ${base}" >&2
  exit 1
fi

changes_file="$(mktemp)"
trap 'rm -f "$changes_file"' EXIT

if ! git diff --name-status -z --find-renames "$base" "$head" -- >"$changes_file"; then
  echo "ci_change_scope: git diff failed for ${base}..${head}" >&2
  exit 1
fi

while IFS= read -r -d '' status; do
  case "$status" in
    R*|C*)
      IFS= read -r -d '' old_path
      IFS= read -r -d '' new_path
      classify_path "$old_path"
      classify_path "$new_path"
      ;;
    *)
      IFS= read -r -d '' path
      classify_path "$path"
      ;;
  esac
done <"$changes_file"

print_scope
