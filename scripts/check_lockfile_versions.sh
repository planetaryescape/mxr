#!/usr/bin/env bash
# Every package that lives in this repository takes the workspace version, and
# release-please bumps all of them in Cargo.lock at each release (see the
# Cargo.lock entry in release-please-config.json). A branch that adds a crate
# carries that crate's lock entry at the version it was cut from. If a release
# lands before the branch merges, main gets the new crate one version behind
# the rest and every `--locked` build fails. The branch's CI cannot see it: it
# ran before the release.
#
# Usage:
#   scripts/check_lockfile_versions.sh                    # this checkout
#   scripts/check_lockfile_versions.sh --merge-with REF   # what merging HEAD into REF gives
#
# Run the second form right before `gh pr merge`, after `git fetch origin main`.
# It reads the merge from `git merge-tree`, so nothing is checked out or built.

set -euo pipefail

# Vendored crates keep their own version; release-please excludes them too.
excluded="hnsw_rs"

tree=""
if [[ "${1:-}" == "--merge-with" ]]; then
  ref="${2:?usage: check_lockfile_versions.sh --merge-with <ref>}"
  set +e
  merged="$(git merge-tree --write-tree --name-only HEAD "${ref}")"
  status=$?
  set -e
  if [[ ${status} -ne 0 ]]; then
    echo "HEAD does not merge cleanly into ${ref}; rebase first. Conflicted files:" >&2
    tail -n +2 <<< "${merged}" | sed '/^$/,$d; s/^/  /' >&2
    exit 1
  fi
  tree="$(head -n 1 <<< "${merged}")"
elif [[ $# -gt 0 ]]; then
  echo "usage: check_lockfile_versions.sh [--merge-with <ref>]" >&2
  exit 2
fi

read_file() {
  if [[ -n "${tree}" ]]; then
    git cat-file -p "${tree}:$1"
  else
    cat "$1"
  fi
}

workspace_version="$(read_file Cargo.toml | awk '
  /^\[/ { in_package = ($0 == "[workspace.package]"); next }
  in_package && /^version[[:space:]]*=/ {
    sub(/^version[[:space:]]*=[[:space:]]*"/, ""); sub(/".*/, ""); print; exit
  }')"
if [[ -z "${workspace_version}" ]]; then
  echo "Cargo.toml has no [workspace.package] version" >&2
  exit 1
fi

# A lock entry with no `source` is a package in this repository.
stale="$(read_file Cargo.lock | awk -v want="${workspace_version}" -v excluded="${excluded}" '
  function flush() {
    if (name != "" && !from_registry && name != excluded && version != want) {
      print name " " version
    }
    name = ""; version = ""; from_registry = 0
  }
  # Only [[package]] tables; [[patch.unused]] and [metadata] also have names.
  /^\[/ { flush(); in_package = ($0 == "[[package]]"); next }
  !in_package { next }
  /^name = "/ { name = $3; gsub(/"/, "", name); next }
  /^version = "/ { version = $3; gsub(/"/, "", version); next }
  /^source = / { from_registry = 1; next }
  END { flush() }')"

where="Cargo.lock"
if [[ -n "${tree}" ]]; then
  where="Cargo.lock after merging HEAD into ${ref}"
fi
if [[ -n "${stale}" ]]; then
  echo "${where}: in-repo packages not at the workspace version ${workspace_version}:" >&2
  sed 's/^/  /' <<< "${stale}" >&2
  echo "Rebase onto the target branch, run \`cargo update --workspace --offline\` (it rewrites only these versions), and commit Cargo.lock." >&2
  exit 1
fi
echo "${where}: every in-repo package is at ${workspace_version}"
