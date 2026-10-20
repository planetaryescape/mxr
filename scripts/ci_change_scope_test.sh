#!/usr/bin/env bash

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
git_binary="$(command -v git)"

commit_all() {
  git add -A
  git commit -q -m "$1"
}

reset_fixture() {
  git reset -q --hard "$baseline"
  git clean -qfd
}

assert_scope() {
  local event_name="$1" base="$2" head="$3"
  local expected_rust="$4" expected_web="$5" expected_docs="$6" expected_expensive="$7"
  local expected actual

  expected="$(printf 'rust=%s\nweb=%s\ndocs=%s\nexpensive=%s' \
    "$expected_rust" "$expected_web" "$expected_docs" "$expected_expensive")"
  actual="$(bash scripts/ci_change_scope.sh "$event_name" "$base" "$head")"
  if [[ "$actual" != "$expected" ]]; then
    printf 'unexpected scope for %s %s %s\nexpected:\n%s\nactual:\n%s\n' \
      "$event_name" "$base" "$head" "$expected" "$actual" >&2
    exit 1
  fi
}

mkdir -p "$tmp/repo"
cd "$tmp/repo"
git init -q
git config user.email ci@example.test
git config user.name 'CI test'
mkdir -p scripts src apps/web/src crates/web/src crates/protocol/src site .github/workflows
cp "$root/scripts/ci_change_scope.sh" scripts/ci_change_scope.sh
cp "$root/scripts/ci_change_scope_test.sh" scripts/ci_change_scope_test.sh
cp "$root/scripts/ci_workflow_test.sh" scripts/ci_workflow_test.sh
touch .github/workflows/ci.yml .github/workflows/openapi-conformance.yml \
  .github/workflows/release.yml src/main.rs apps/web/src/app.ts crates/web/src/lib.rs \
  crates/protocol/src/lib.rs site/index.md
commit_all baseline
baseline="$(git rev-parse HEAD)"

echo rust >src/main.rs
commit_all 'Rust-only change'
assert_scope push "$baseline" HEAD true false false false

reset_fixture
echo web >apps/web/src/app.ts
commit_all 'web-only change'
assert_scope push "$baseline" HEAD false true false false

reset_fixture
echo protocol >crates/protocol/src/lib.rs
commit_all 'protocol change'
assert_scope pull_request "$baseline" HEAD true true false false

reset_fixture
echo web >crates/web/src/lib.rs
commit_all 'web crate change'
assert_scope push "$baseline" HEAD true true false false

reset_fixture
echo docs >site/index.md
commit_all 'site-only change'
assert_scope push "$baseline" HEAD false false true false

reset_fixture
mkdir -p site/scripts
echo '{}' >site/scripts/cli-reference-examples.json
commit_all 'CLI reference examples change'
assert_scope pull_request "$baseline" HEAD true false true false

reset_fixture
for workflow_file in .github/workflows/ci.yml .github/workflows/openapi-conformance.yml .github/workflows/release.yml; do
  reset_fixture
  echo workflow >"$workflow_file"
  commit_all "workflow change $workflow_file"
  assert_scope push "$baseline" HEAD true true true false
done

reset_fixture
printf '\n# classifier change\n' >>scripts/ci_change_scope.sh
commit_all 'classifier change'
assert_scope push "$baseline" HEAD true true true false

reset_fixture
printf '\n# workflow test change\n' >>scripts/ci_workflow_test.sh
commit_all 'workflow test change'
assert_scope push "$baseline" HEAD true true true false

reset_fixture
echo rust >src/main.rs
echo web >apps/web/src/app.ts
echo docs >site/index.md
commit_all 'mixed change'
assert_scope push "$baseline" HEAD true true true false

reset_fixture
git rm -q apps/web/src/app.ts
commit_all 'delete web path'
assert_scope push "$baseline" HEAD false true false false

reset_fixture
git mv site/index.md apps/web/src/renamed.md
commit_all 'rename between scopes'
if ! git diff --name-status --find-renames "$baseline" HEAD | grep -Eq '^R[0-9]+[[:space:]]+site/index\.md[[:space:]]+apps/web/src/renamed\.md$'; then
  echo 'rename fixture must be recognized as a rename' >&2
  exit 1
fi
assert_scope push "$baseline" HEAD false true true false

reset_fixture
echo spaced >'apps/web/src/file with spaces.ts'
commit_all 'path with spaces'
assert_scope push "$baseline" HEAD false true false false

reset_fixture
assert_scope push '' HEAD true true true false
assert_scope push 0000000000000000000000000000000000000000 HEAD true true true false
assert_scope workflow_dispatch '' '' true true true true

if bash scripts/ci_change_scope.sh push invalid-ref HEAD >/dev/null 2>&1; then
  echo 'invalid base ref must fail' >&2
  exit 1
fi
if bash scripts/ci_change_scope.sh push "$baseline" invalid-ref >/dev/null 2>&1; then
  echo 'invalid head ref must fail' >&2
  exit 1
fi
if bash scripts/ci_change_scope.sh push '' invalid-ref >/dev/null 2>&1; then
  echo 'missing base must not hide an invalid head ref' >&2
  exit 1
fi

mkdir -p "$tmp/bin"
cat >"$tmp/bin/git" <<'EOF'
#!/usr/bin/env bash
if [[ "$1" == diff ]]; then
  exit 23
fi
exec "$REAL_GIT" "$@"
EOF
chmod +x "$tmp/bin/git"
set +e
diff_output="$(PATH="$tmp/bin:$PATH" REAL_GIT="$git_binary" \
  bash scripts/ci_change_scope.sh push "$baseline" HEAD 2>"$tmp/diff-error")"
diff_status=$?
set -e
if [[ "$diff_status" -eq 0 || -n "$diff_output" ]] || \
  ! grep -Fq 'ci_change_scope: git diff failed' "$tmp/diff-error"; then
  echo 'git diff failure must return an error without emitting a scope' >&2
  exit 1
fi

echo 'ci_change_scope_test: ok'
