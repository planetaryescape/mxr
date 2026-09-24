#!/usr/bin/env bash

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT
git_binary="$(command -v git)"

commit_all() {
  git add -A
  git commit -q -m "$1"
}

reset_fixture() {
  git reset -q --hard "${baseline}"
  git clean -qfd
}

assert_scope() {
  local event_name="$1"
  local base="$2"
  local head="$3"
  local expected_rust="$4"
  local expected_web="$5"
  local expected_docs="$6"
  local expected_expensive="$7"
  local expected
  local actual

  expected="$(printf 'rust=%s\nweb=%s\ndocs=%s\nexpensive=%s' \
    "${expected_rust}" "${expected_web}" "${expected_docs}" "${expected_expensive}")"
  actual="$(bash scripts/ci_change_scope.sh "${event_name}" "${base}" "${head}")"

  if [[ "${actual}" != "${expected}" ]]; then
    echo "unexpected scope for ${event_name} ${base} ${head}" >&2
    printf 'expected:\n%s\nactual:\n%s\n' "${expected}" "${actual}" >&2
    exit 1
  fi
}

mkdir -p "${tmp}/repo"
cd "${tmp}/repo"
git init -q
git config user.email "ci@example.test"
git config user.name "CI"

mkdir -p scripts src apps/web/src crates/web/src crates/protocol/src site .github/workflows
cp "${root}/scripts/ci_change_scope.sh" scripts/ci_change_scope.sh
touch scripts/ci_change_scope_test.sh .github/workflows/ci.yml
touch src/main.rs apps/web/src/app.ts crates/web/src/lib.rs crates/protocol/src/lib.rs site/index.md
commit_all "baseline"
baseline="$(git rev-parse HEAD)"

echo "rust" > src/main.rs
commit_all "rust only"
assert_scope push "${baseline}" HEAD true false false false

reset_fixture
echo "app" > apps/web/src/app.ts
commit_all "app only"
assert_scope push "${baseline}" HEAD false true false false

reset_fixture
echo "bridge" > crates/web/src/lib.rs
commit_all "bridge only"
assert_scope push "${baseline}" HEAD true true false false

reset_fixture
echo "protocol" > crates/protocol/src/lib.rs
commit_all "protocol only"
assert_scope pull_request "${baseline}" HEAD true true false false

reset_fixture
echo "site" > site/index.md
commit_all "site only"
assert_scope push "${baseline}" HEAD false false true false

reset_fixture
echo "rust" > src/main.rs
echo "app" > apps/web/src/app.ts
echo "site" > site/index.md
commit_all "mixed paths"
assert_scope push "${baseline}" HEAD true true true false

reset_fixture
git rm -q apps/web/src/app.ts
commit_all "delete app path"
assert_scope push "${baseline}" HEAD false true false false

reset_fixture
git mv site/index.md apps/web/src/renamed.md
commit_all "rename across scopes"
assert_scope push "${baseline}" HEAD false true true false

reset_fixture
echo "space" > "apps/web/src/file with spaces.ts"
commit_all "path with spaces"
assert_scope push "${baseline}" HEAD false true false false

for control_file in .github/workflows/ci.yml scripts/ci_change_scope.sh scripts/ci_change_scope_test.sh; do
  reset_fixture
  echo " " >> "${control_file}"
  commit_all "change ${control_file}"
  assert_scope push "${baseline}" HEAD true true true false
done

reset_fixture
assert_scope push "" HEAD true true true false
assert_scope push "0000000000000000000000000000000000000000" HEAD true true true false
assert_scope workflow_dispatch "" "" true true true true

if bash scripts/ci_change_scope.sh push invalid-ref HEAD >/dev/null 2>&1; then
  echo "invalid base ref must fail" >&2
  exit 1
fi

if bash scripts/ci_change_scope.sh push "${baseline}" invalid-ref >/dev/null 2>&1; then
  echo "invalid head ref must fail" >&2
  exit 1
fi

if bash scripts/ci_change_scope.sh push "" invalid-ref >/dev/null 2>&1; then
  echo "missing base must not hide an invalid head ref" >&2
  exit 1
fi

mkdir -p "${tmp}/bin"
cat > "${tmp}/bin/git" <<'EOF'
#!/usr/bin/env bash
if [[ "$1" == "diff" ]]; then
  exit 23
fi
exec "${REAL_GIT}" "$@"
EOF
chmod +x "${tmp}/bin/git"

set +e
diff_failure_output="$(PATH="${tmp}/bin:${PATH}" REAL_GIT="${git_binary}" \
  bash scripts/ci_change_scope.sh push "${baseline}" HEAD 2> "${tmp}/diff-error")"
diff_failure_status=$?
set -e

if [[ "${diff_failure_status}" -eq 0 ]]; then
  echo "git diff failure must fail classification" >&2
  exit 1
fi

if [[ -n "${diff_failure_output}" ]]; then
  echo "git diff failure must not print a false-success scope" >&2
  exit 1
fi

if ! grep -Fq "ci_change_scope: git diff failed" "${tmp}/diff-error"; then
  echo "git diff failure must be visible" >&2
  exit 1
fi

echo "ci_change_scope_test: ok"
