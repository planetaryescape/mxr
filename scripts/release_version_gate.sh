#!/usr/bin/env bash

set -euo pipefail

tag="${1:-}"
manifest="${2:-Cargo.toml}"

if [[ -z "${tag}" ]]; then
  echo "usage: $0 <tag> [Cargo.toml]" >&2
  exit 2
fi

if [[ ! "${tag}" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$ ]]; then
  echo "::error ::Release ref must be a semver tag like v1.2.3; got '${tag}'." >&2
  exit 1
fi

cargo_version="$(
  awk '
    /^\[/ {
      in_workspace_package = ($0 == "[workspace.package]")
      next
    }
    in_workspace_package && /^[[:space:]]*version[[:space:]]*=/ {
      line = $0
      sub(/^[[:space:]]*version[[:space:]]*=[[:space:]]*"/, "", line)
      sub(/".*$/, "", line)
      print line
      exit
    }
  ' "${manifest}"
)"

if [[ -z "${cargo_version}" ]]; then
  echo "::error ::Could not find workspace.package.version in ${manifest}." >&2
  exit 1
fi

tag_version="${tag#v}"
if [[ "${tag_version}" != "${cargo_version}" ]]; then
  echo "::error ::Release tag ${tag} does not match Cargo workspace version ${cargo_version}." >&2
  exit 1
fi

echo "Release version gate passed: ${tag} matches Cargo workspace version ${cargo_version}."
