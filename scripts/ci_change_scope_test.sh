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
  local expected_rust="$4" expected_web="$5" expected_apps_web="$6" expected_docs="$7" expected_expensive="$8"
  local expected_classifier="${9:-false}"
  local expected actual

  expected="$(printf 'rust=%s\nweb=%s\napps_web=%s\ndocs=%s\nexpensive=%s\nclassifier=%s' \
    "$expected_rust" "$expected_web" "$expected_apps_web" "$expected_docs" "$expected_expensive" \
    "$expected_classifier")"
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
mkdir -p scripts src apps/web/src crates/web/src crates/protocol/src crates/provider-fake/src \
  crates/daemon/src crates/tui/src crates/config/src crates/llm/src site .github/workflows
cp "$root/scripts/ci_change_scope.sh" scripts/ci_change_scope.sh
cp "$root/scripts/ci_change_scope_test.sh" scripts/ci_change_scope_test.sh
cp "$root/scripts/ci_workflow_test.sh" scripts/ci_workflow_test.sh
touch .github/workflows/ci.yml .github/workflows/openapi-conformance.yml \
  .github/workflows/release.yml src/main.rs apps/web/src/app.ts crates/web/src/lib.rs \
  crates/protocol/src/lib.rs crates/provider-fake/src/lib.rs crates/daemon/src/lib.rs \
  crates/tui/src/lib.rs crates/config/src/lib.rs crates/llm/src/lib.rs Cargo.toml site/index.md
commit_all baseline
baseline="$(git rev-parse HEAD)"

echo rust >src/main.rs
commit_all 'Rust-only change'
assert_scope push "$baseline" HEAD true false false false false

reset_fixture
echo web >apps/web/src/app.ts
commit_all 'web-only change'
assert_scope push "$baseline" HEAD false true true false false

reset_fixture
echo protocol >crates/protocol/src/lib.rs
commit_all 'protocol change'
assert_scope pull_request "$baseline" HEAD true true false false false true

# The full-set classifier eval runs for the crates that generate, classify
# and grade mail, and not for a UI crate.
for classifier_path in crates/provider-fake/src/lib.rs crates/daemon/src/lib.rs crates/config/src/lib.rs crates/llm/src/lib.rs; do
  reset_fixture
  echo change >"$classifier_path"
  commit_all "classifier path $classifier_path"
  assert_scope pull_request "$baseline" HEAD true false false false false true
done

# The root manifest is the daemon crate's and the workspace's dependency
# manifest, so a dependency or feature change there can move an answer.
reset_fixture
echo change >Cargo.toml
commit_all 'root manifest change'
assert_scope pull_request "$baseline" HEAD true false false false false true

reset_fixture
echo tui >crates/tui/src/lib.rs
commit_all 'tui change'
assert_scope pull_request "$baseline" HEAD true false false false false false

reset_fixture
echo web >crates/web/src/lib.rs
commit_all 'web crate change'
assert_scope push "$baseline" HEAD true true false false false

reset_fixture
echo docs >site/index.md
commit_all 'site-only change'
assert_scope push "$baseline" HEAD false false false true false

reset_fixture
mkdir -p site/src/content/docs/reference
echo docs >site/src/content/docs/reference/keybindings.md
commit_all 'keybindings reference change'
assert_scope pull_request "$baseline" HEAD false true false true false

reset_fixture
mkdir -p apps/raycast/src
echo raycast >apps/raycast/src/now.tsx
commit_all 'Raycast extension change'
assert_scope pull_request "$baseline" HEAD false true false false false

reset_fixture
mkdir -p docs/reference
echo '{}' >docs/reference/tui-keymap.json
commit_all 'TUI keymap reference change'
assert_scope pull_request "$baseline" HEAD false true false false false

reset_fixture
mkdir -p site/scripts
echo '{}' >site/scripts/cli-reference-examples.json
commit_all 'CLI reference examples change'
assert_scope pull_request "$baseline" HEAD true false false true false

reset_fixture
for workflow_file in .github/workflows/ci.yml .github/workflows/openapi-conformance.yml .github/workflows/release.yml; do
  reset_fixture
  echo workflow >"$workflow_file"
  commit_all "workflow change $workflow_file"
  assert_scope push "$baseline" HEAD true true false true false true
done

reset_fixture
printf '\n# scope script change\n' >>scripts/ci_change_scope.sh
commit_all 'scope script change'
assert_scope push "$baseline" HEAD true true false true false true

reset_fixture
printf '\n# workflow test change\n' >>scripts/ci_workflow_test.sh
commit_all 'workflow test change'
assert_scope push "$baseline" HEAD true true false true false true

reset_fixture
echo rust >src/main.rs
echo web >apps/web/src/app.ts
echo docs >site/index.md
commit_all 'mixed change'
assert_scope push "$baseline" HEAD true true true true false

reset_fixture
git rm -q apps/web/src/app.ts
commit_all 'delete web path'
assert_scope push "$baseline" HEAD false true true false false

reset_fixture
git mv site/index.md apps/web/src/renamed.md
commit_all 'rename between scopes'
if ! git diff --name-status --find-renames "$baseline" HEAD | grep -Eq '^R[0-9]+[[:space:]]+site/index\.md[[:space:]]+apps/web/src/renamed\.md$'; then
  echo 'rename fixture must be recognized as a rename' >&2
  exit 1
fi
assert_scope push "$baseline" HEAD false true true true false

reset_fixture
echo spaced >'apps/web/src/file with spaces.ts'
commit_all 'path with spaces'
assert_scope push "$baseline" HEAD false true true false false

# A release-please merge only bumps versions: the workspace version, each
# workspace crate's Cargo.lock entry, the manifest, CHANGELOG and the OpenAPI
# document under site/. It runs the docs build and nothing that compiles.
write_release_files() {
  local version="$1" serde_version="$2" serde_checksum="$3"
  printf '[workspace.package]\nversion = "%s"\nedition = "2021"\n\n[workspace.dependencies]\nserde = "%s"\n' \
    "$version" "$serde_version" >Cargo.toml
  printf '# generated\nversion = 4\n\n[[package]]\nname = "mxr"\nversion = "%s"\ndependencies = [\n "mxr-core",\n "serde",\n]\n\n[[package]]\nname = "mxr-core"\nversion = "%s"\n\n[[package]]\nname = "serde"\nversion = "%s"\nsource = "registry+https://github.com/rust-lang/crates.io-index"\nchecksum = "%s"\n' \
    "$version" "$version" "$serde_version" "$serde_checksum" >Cargo.lock
  printf '{\n  ".": "%s"\n}\n' "$version" >.release-please-manifest.json
  mkdir -p site/public
  printf '{\n  "info": {\n    "version": "%s"\n  }\n}\n' "$version" >site/public/openapi.json
  echo "## $version" >>CHANGELOG.md
}

reset_fixture
write_release_files 0.6.1 1.0.1 aaaa
commit_all 'before the release'
release_base="$(git rev-parse HEAD)"
write_release_files 0.6.2 1.0.1 aaaa
commit_all 'chore(main): release 0.6.2'
assert_scope push "$release_base" HEAD false false false true false false

# The same release with a real dependency bump in the lock file is Rust.
git reset -q --hard "$release_base"
write_release_files 0.6.2 1.0.2 bbbb
commit_all 'chore(main): release 0.6.2'
assert_scope push "$release_base" HEAD true false false true false true

# So is a manifest change beyond the version, whatever the commit says.
git reset -q --hard "$release_base"
write_release_files 0.6.2 1.0.1 aaaa
printf 'tokio = "1"\n' >>Cargo.toml
commit_all 'chore(main): release 0.6.2'
assert_scope push "$release_base" HEAD true false false true false true

reset_fixture
assert_scope push '' HEAD true true true true false true
assert_scope push 0000000000000000000000000000000000000000 HEAD true true true true false true
assert_scope workflow_dispatch '' '' true true true true true true

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
