#!/usr/bin/env bash
# Exercises scripts/publish_github_release.sh against a fake `gh` that keeps
# release state in files and logs every call, so ordering and which state each
# failure leaves behind are asserted without touching GitHub.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
script="$here/publish_github_release.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/bin"
cat > "$work/bin/gh" <<'FAKE'
#!/usr/bin/env bash
# state dir layout: $FAKE/state = absent|draft|published ; $FAKE/assets/<name> = size ; $FAKE/log
set -euo pipefail
echo "$*" >> "$FAKE/log"
state="$(cat "$FAKE/state")"
case "$1 $2" in
  "release view")
    [[ -f "$FAKE/lookup_error" ]] && { echo "HTTP 502" >&2; exit 1; }
    [[ "$state" == absent ]] && { echo "release not found" >&2; exit 1; }
    if [[ "$*" == *isDraft* ]]; then [[ "$state" == draft ]] && echo true || echo false
    else for f in "$FAKE"/assets/*; do [[ -e "$f" ]] && echo "$(basename "$f") $(cat "$f")"; done; true; fi ;;
  "release create") echo draft > "$FAKE/state" ;;
  "release edit")
    if [[ "$*" == *--draft=false* ]]; then
      [[ "$state" == draft ]] || { echo "edit on non-draft" >&2; exit 1; }
      echo published > "$FAKE/state"
    fi ;;
  "release upload")
    [[ "$state" == draft ]] || { echo "upload to non-draft" >&2; exit 1; }
    shift 3
    for a in "$@"; do
      [[ -f "$a" ]] || continue
      n="$(basename "$a")"
      # Real --clobber deletes the existing asset first, then uploads.
      rm -f "$FAKE/assets/$n" "$FAKE/content/$n"
      [[ -f "$FAKE/upload_fails" ]] && { echo "upload failed" >&2; exit 1; }
      if [[ -f "$FAKE/short_upload" ]]; then echo 1 > "$FAKE/assets/$n"
      else wc -c < "$a" | tr -d ' ' > "$FAKE/assets/$n"; fi
      cp "$a" "$FAKE/content/$n"
    done ;;
  "release download")
    while [[ $# -gt 0 ]]; do [[ "$1" == --dir ]] && dir="$2"; shift; done
    cp "$FAKE"/content/*.sha256 "$dir"/ ;;
  *) echo "unexpected gh call: $*" >&2; exit 1 ;;
esac
FAKE
chmod +x "$work/bin/gh"

fail() { echo "FAIL: $1" >&2; exit 1; }

setup() { # <state>
  export FAKE="$work/fake"; rm -rf "$FAKE" "$work/art"
  mkdir -p "$FAKE/assets" "$FAKE/content" "$work/art"
  echo "$1" > "$FAKE/state"; : > "$FAKE/log"
  echo notes > "$work/notes.md"
  printf 'tarball' > "$work/art/mxr-v1-linux.tar.gz"
  printf 'sum' > "$work/art/mxr-v1-linux.tar.gz.sha256"
}
run() { PATH="$work/bin:$PATH" bash "$script" v1 "$work/notes.md" "$@"; }
state() { cat "$FAKE/state"; }
log() { cat "$FAKE/log"; }

# 1. no release yet: draft created, assets uploaded, then published as Latest; publish is last.
setup absent
run "$work/art" >/dev/null
[[ "$(state)" == published ]] || fail "absent: not published"
grep -q -- 'release create v1 --draft --verify-tag' "$FAKE/log" || fail "absent: not created as draft with --verify-tag"
[[ "$(grep -n 'release upload' "$FAKE/log" | cut -d: -f1)" -lt "$(grep -n 'draft=false --latest' "$FAKE/log" | cut -d: -f1)" ]] || fail "publish before upload"
[[ "$(tail -n1 "$FAKE/log")" == *"--draft=false --latest"* ]] || fail "publish is not the last call"
[[ -e "$FAKE/assets/mxr-v1-linux.tar.gz.sha256" ]] || fail "sha256 asset missing"

# 2. release-please's draft exists: notes refreshed, no create.
setup draft
run "$work/art" >/dev/null
[[ "$(state)" == published ]] || fail "draft: not published"
! grep -q 'release create' "$FAKE/log" || fail "draft: created a second release"

# 3. upload fails: stays draft, never published.
setup draft; touch "$FAKE/upload_fails"
! run "$work/art" >/dev/null 2>&1 || fail "upload failure did not fail the script"
[[ "$(state)" == draft ]] || fail "upload failure: not left as draft"

# 4. re-run after a partial upload (one asset attached) completes idempotently.
setup draft; echo 7 > "$FAKE/assets/mxr-v1-linux.tar.gz"
run "$work/art" >/dev/null
[[ "$(state)" == published ]] || fail "rerun: not published"
grep -q -- '--clobber' "$FAKE/log" || fail "rerun: upload without --clobber"

# 5. a silently truncated upload (right name, wrong size) blocks publish.
setup draft; touch "$FAKE/short_upload"
! run "$work/art" >/dev/null 2>&1 || fail "size mismatch did not fail"
[[ "$(state)" == draft ]] || fail "size mismatch: published anyway"

# 5b. --clobber deletes before re-uploading: a failure mid-way leaves a draft missing that asset, never a published one.
setup draft; echo 7 > "$FAKE/assets/mxr-v1-linux.tar.gz"; touch "$FAKE/upload_fails"
! run "$work/art" >/dev/null 2>&1 || fail "clobber failure did not fail"
[[ "$(state)" == draft ]] || fail "clobber failure: published"
[[ ! -e "$FAKE/assets/mxr-v1-linux.tar.gz" ]] || fail "fake gh does not model delete-before-upload"

# 6. already published and complete: no mutation, success.
setup published
publish_same() { printf 7 > "$FAKE/assets/mxr-v1-linux.tar.gz"; printf 3 > "$FAKE/assets/mxr-v1-linux.tar.gz.sha256"; cp "$work/art/mxr-v1-linux.tar.gz.sha256" "$FAKE/content/"; }
publish_same
run "$work/art" >/dev/null
! grep -Eq 'release (edit|upload|create)' "$FAKE/log" || fail "published: mutated a published release"

# 6b. published with equal names and sizes but different checksum content (rebuilt tarball): fail, no writes.
setup published
publish_same; printf 'xyz' > "$FAKE/content/mxr-v1-linux.tar.gz.sha256"
! run "$work/art" >/dev/null 2>&1 || fail "checksum mismatch on published release did not fail"
! grep -Eq 'release (edit|upload|create)' "$FAKE/log" || fail "checksum mismatch: mutated"

# 7. already published but incomplete: fail, no mutation.
setup published
! run "$work/art" >/dev/null 2>&1 || fail "incomplete published release succeeded"
! grep -Eq 'release (edit|upload|create)' "$FAKE/log" || fail "incomplete published: mutated"

# 8. no artifacts (docs-only tag): published but not Latest.
setup draft
run >/dev/null
[[ "$(state)" == published ]] || fail "no-artifacts: not published"
grep -q -- '--draft=false --latest=false' "$FAKE/log" || fail "no-artifacts: marked Latest"

# 9. lookup error that is not "not found": no duplicate draft created.
setup absent; touch "$FAKE/lookup_error"
! run "$work/art" >/dev/null 2>&1 || fail "lookup error did not fail"
! grep -q 'release create' "$FAKE/log" || fail "lookup error: created a release"

echo "publish_github_release_test: ok"
