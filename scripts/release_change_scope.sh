#!/usr/bin/env bash

set -euo pipefail

base_ref="${1:-}"
head_ref="${2:-HEAD}"

emit_output() {
  local key="$1"
  local value="$2"
  if [[ -n "${GITHUB_OUTPUT:-}" ]]; then
    echo "${key}=${value}" >> "${GITHUB_OUTPUT}"
  fi
  echo "${key}=${value}"
}

if [[ -z "${base_ref}" ]]; then
  emit_output cli_changed true
  emit_output desktop_source_changed true
  emit_output desktop_changed true
  emit_output has_artifacts true
  exit 0
fi

changed_files="$(git diff --name-only "${base_ref}" "${head_ref}")"

relevant_diff() {
  local path="$1"
  local ignore_regex="$2"
  local diff_lines

  diff_lines="$(
    git diff --unified=0 "${base_ref}" "${head_ref}" -- "${path}" |
      awk '/^[+-]/ && !/^\+\+\+|^---/ {print}'
  )"

  [[ -z "${diff_lines}" ]] && return 1

  if printf '%s\n' "${diff_lines}" | grep -Eqv "${ignore_regex}"; then
    return 0
  fi

  return 1
}

cargo_toml_ignore='^[+-][[:space:]]*$|^[+-][[:space:]]*#|^[+-][[:space:]]*version([[:space:]]*=[[:space:]]*"[^"]+"|\.workspace[[:space:]]*=[[:space:]]*true)$|^[+-][[:space:]]*mxr-[^[:space:]=]+[[:space:]]*=[[:space:]]*\{[[:space:]]*path[[:space:]]*=[[:space:]]*"crates/[^"]+",[[:space:]]*version[[:space:]]*=[[:space:]]*"[^"]+".*\}$'
cargo_lock_ignore='^[+-][[:space:]]*$|^[+-][[:space:]]*version[[:space:]]*=[[:space:]]*"[^"]+"$'
package_json_ignore='^[+-][[:space:]]*"version":[[:space:]]*"[^"]+",?$'

cli_changed=false
desktop_source_changed=false
desktop_changed=false

while IFS= read -r path; do
  [[ -z "${path}" ]] && continue
  case "${path}" in
    crates/*/Cargo.toml|Cargo.toml|Cargo.lock|apps/desktop/package.json|apps/desktop/package-lock.json)
      ;;
    .github/workflows/release.yml|scripts/release_change_scope.sh)
      cli_changed=true
      ;;
    scripts/render_homebrew_formula.sh|scripts/publish_workspace_crates.sh)
      cli_changed=true
      ;;
    crates/*|vendor/*|.sqlx/*)
      cli_changed=true
      ;;
    apps/desktop/*)
      desktop_source_changed=true
      ;;
  esac
done <<< "${changed_files}"

if relevant_diff "Cargo.toml" "${cargo_toml_ignore}"; then
  cli_changed=true
fi

if relevant_diff "Cargo.lock" "${cargo_lock_ignore}"; then
  cli_changed=true
fi

while IFS= read -r manifest; do
  [[ -z "${manifest}" ]] && continue
  if relevant_diff "${manifest}" "${cargo_toml_ignore}"; then
    cli_changed=true
    break
  fi
done < <(printf '%s\n' "${changed_files}" | grep '^crates/.*/Cargo.toml$' || true)

if relevant_diff "apps/desktop/package.json" "${package_json_ignore}"; then
  desktop_source_changed=true
fi

if relevant_diff "apps/desktop/package-lock.json" "${package_json_ignore}"; then
  desktop_source_changed=true
fi

desktop_changed="${desktop_source_changed}"

has_artifacts=false
if [[ "${cli_changed}" == true || "${desktop_changed}" == true ]]; then
  has_artifacts=true
fi

emit_output cli_changed "${cli_changed}"
emit_output desktop_source_changed "${desktop_source_changed}"
emit_output desktop_changed "${desktop_changed}"
emit_output has_artifacts "${has_artifacts}"
