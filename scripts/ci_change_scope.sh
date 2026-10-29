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

  # The full-set classifier eval takes about 20 minutes, so it runs only when
  # the code that decides a graded answer changes: the demo generator and its
  # labels, the detectors (and the reader text they read), the daemon's
  # post-sync classification (`post_sync_classification` in loops.rs and the
  # handlers it calls), and the eval itself. A store, sync, core, protocol,
  # config, llm or dependency change can still move an answer; only a manual
  # run (workflow_dispatch) catches that. The eval seeds no rules and skips the
  # relationship workers, and safety only checks drafts, so the rules,
  # relationship and safety crates can't.
  case "$path" in
    crates/provider-fake/src/fixtures*|crates/provider-fake/src/lib.rs)
      classifier=true
      ;;
    crates/todo/src/*|crates/records/src/*|crates/deliveries/src/*|crates/updates/src/*|crates/reading/src/*|crates/reader/src/*)
      classifier=true
      ;;
    crates/daemon/src/loops.rs|crates/daemon/src/handler/classify_eval.rs|crates/daemon/src/handler/classify_eval_asks.rs)
      classifier=true
      ;;
    crates/daemon/src/handler/deliveries.rs|crates/daemon/src/handler/todos.rs|crates/daemon/src/handler/todo_resolution*|crates/daemon/src/handler/asks_upgrade.rs|crates/daemon/src/handler/records.rs|crates/daemon/src/handler/reading/*|crates/daemon/src/handler/updates.rs)
      classifier=true
      ;;
    # Who and home: the stored assessment, where an arrival goes, and the
    # rules those read.
    crates/daemon/src/handler/assessments.rs|crates/daemon/src/handler/ask_assessments.rs|crates/daemon/src/handler/arrivals.rs|crates/daemon/src/handler/mail_kind.rs|crates/daemon/src/handler/conversation_shape.rs|crates/daemon/src/handler/mode_rules.rs|crates/daemon/src/handler/desk_lanes.rs)
      classifier=true
      ;;
    crates/daemon/src/handler/tests/classify_eval_*|crates/daemon/src/handler/tests/snapshots/mxr__handler__tests__classify_eval_full__*)
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

# A release-please merge bumps the workspace version in Cargo.toml and every
# workspace crate's entry in Cargo.lock (release-please-config.json). Nothing
# compiles differently, so those manifests alone don't start the Rust suites.
# Compared with the versions blanked, so a dependency, feature or checksum
# change in the same diff still counts.
normalize_lock() {
  awk 'BEGIN { RS = ""; ORS = "\n\n" }
    !/\nsource = / && /\nname = "mxr(-[a-z0-9-]+)?"\n/ { sub(/\nversion = "[^"]*"/, "\nversion = \"*\"") }
    { print }'
}

normalize_manifest() {
  awk '/^\[/ { section = $0 }
    (section == "[package]" || section == "[workspace.package]") && /^version[[:space:]]*=[[:space:]]*"/ { $0 = "version = \"*\"" }
    { print }'
}

version_only_manifest() {
  local path="$1" normalize before after
  case "$path" in
    Cargo.lock) normalize=normalize_lock ;;
    Cargo.toml | crates/*/Cargo.toml) normalize=normalize_manifest ;;
    *) return 1 ;;
  esac
  before="$(git show "${base}:${path}")" || return 1
  after="$(git show "${head}:${path}")" || return 1
  [[ "$("$normalize" <<<"$before")" == "$("$normalize" <<<"$after")" ]]
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
      if [[ "$status" == M ]] && version_only_manifest "$path"; then
        continue
      fi
      classify_path "$path"
      ;;
  esac
done <"$changes_file"

print_scope
