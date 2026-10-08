#!/usr/bin/env bash
# Publishes a GitHub release only after its assets are attached.
#
# release-please creates the release as a DRAFT (release-please-config.json:
# draft + force-tag-creation). This script finds that draft (or creates one if
# release-please has not), uploads the assets, checks every expected asset is
# attached with the right size, and only then publishes the release. Nothing
# before the final `gh release edit --draft=false` is visible to
# `releases/latest`, install.sh or the Homebrew tap.
#
# Re-running is safe: uploads use --clobber, which is only ever reached while
# the release is still a draft. A published release is never modified because
# published releases are meant to be immutable.
#
# usage: publish_github_release.sh <tag> <notes-file> [<artifacts-dir>]
#   With an artifacts dir the release is published as Latest. Without one
#   (docs-only/version-only tags that ship no binaries) it is published but NOT
#   marked Latest, so `releases/latest` keeps resolving to a release that has
#   tarballs.
set -euo pipefail

if [[ $# -lt 2 || $# -gt 3 ]]; then
  echo "usage: $0 <tag> <notes-file> [<artifacts-dir>]" >&2
  exit 2
fi

tag="$1"
notes_file="$2"
artifacts_dir="${3:-}"

[[ -s "$notes_file" ]] || { echo "notes file missing or empty: $notes_file" >&2; exit 1; }

assets=()
if [[ -n "$artifacts_dir" ]]; then
  while IFS= read -r f; do assets+=("$f"); done < <(find "$artifacts_dir" -maxdepth 1 -type f | sort)
  if [[ ${#assets[@]} -eq 0 ]]; then
    echo "no files in artifacts dir: $artifacts_dir" >&2
    exit 1
  fi
fi

file_size() { wc -c < "$1" | tr -d ' '; }

# Fails with a message naming each asset that is missing or the wrong size.
verify_assets() {
  local remote missing=0 f name size
  remote="$(gh release view "$tag" --json assets --jq '.assets[] | "\(.name) \(.size)"')"
  for f in "${assets[@]}"; do
    name="$(basename "$f")"
    size="$(file_size "$f")"
    if ! grep -Fxq "$name $size" <<<"$remote"; then
      echo "asset missing or wrong size on $tag: $name ($size bytes)" >&2
      missing=1
    fi
  done
  return "$missing"
}

# Only a genuine "not found" means no release exists; any other failure (auth,
# network) must stop here rather than create a duplicate draft.
if ! state="$(gh release view "$tag" --json isDraft --jq '.isDraft' 2>&1)"; then
  if grep -qi 'release not found' <<<"$state"; then
    state="absent"
  else
    echo "could not look up release $tag: $state" >&2
    exit 1
  fi
fi

case "$state" in
  absent)
    # --verify-tag: never let `gh` create a tag; release-please owns tags.
    gh release create "$tag" --draft --verify-tag --title "$tag" --notes-file "$notes_file"
    ;;
  true)
    gh release edit "$tag" --notes-file "$notes_file"
    ;;
  false)
    # Already published (full re-run after success, or a release made before
    # this flow). Never touch it; just confirm it is complete.
    if verify_assets; then
      echo "$tag is already published with all expected assets; nothing to do."
      exit 0
    fi
    echo "$tag is already published but incomplete. Published releases are not modified; ship a fix as a new version." >&2
    exit 1
    ;;
  *)
    echo "unexpected draft state for $tag: $state" >&2
    exit 1
    ;;
esac

if [[ ${#assets[@]} -gt 0 ]]; then
  gh release upload "$tag" "${assets[@]}" --clobber
  verify_assets
  gh release edit "$tag" --draft=false --latest
else
  gh release edit "$tag" --draft=false --latest=false
fi

echo "$tag published."
