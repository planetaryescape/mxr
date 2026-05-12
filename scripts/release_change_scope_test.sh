#!/usr/bin/env bash

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

assert_output() {
  local output="$1"
  local expected="$2"
  if ! grep -qx "${expected}" <<< "${output}"; then
    echo "missing expected output: ${expected}" >&2
    echo "${output}" >&2
    return 1
  fi
}

commit_all() {
  local subject="$1"
  git add .
  git commit -q -m "${subject}"
}

reset_fixture() {
  git reset -q --hard "${baseline}"
  git clean -qfd
}

mkdir -p "${tmp}/repo"
cd "${tmp}/repo"
git init -q
git config user.email "ci@example.test"
git config user.name "CI"

mkdir -p scripts src crates/web/src docs
cp "${root}/scripts/release_change_scope.sh" scripts/release_change_scope.sh
cat > Cargo.toml <<'EOF'
[workspace]
members = ["crates/web"]

[workspace.package]
version = "0.1.0"
EOF
cat > Cargo.lock <<'EOF'
version = 4

[[package]]
name = "mxr"
version = "0.1.0"
EOF
cat > src/main.rs <<'EOF'
fn main() {}
EOF
cat > crates/web/src/lib.rs <<'EOF'
pub fn web() {}
EOF
cat > docs/notes.md <<'EOF'
# Notes
EOF
commit_all "baseline"
baseline="$(git rev-parse HEAD)"

echo "pub fn web_changed() {}" >> crates/web/src/lib.rs
commit_all "rust: touch web"
output="$(bash scripts/release_change_scope.sh "${baseline}" HEAD)"
assert_output "${output}" "cli_changed=true"
assert_output "${output}" "has_artifacts=true"

reset_fixture
perl -0pi -e 's/version = "0.1.0"/version = "0.1.1"/g' Cargo.toml Cargo.lock
commit_all "chore: version bump"
output="$(bash scripts/release_change_scope.sh "${baseline}" HEAD)"
assert_output "${output}" "cli_changed=false"
assert_output "${output}" "has_artifacts=false"

reset_fixture
perl -0pi -e 's/version = "0.1.0"/version = "0.1.1"/g' Cargo.toml Cargo.lock
commit_all "release: prepare mxr 0.1.1"
output="$(bash scripts/release_change_scope.sh "${baseline}" HEAD)"
assert_output "${output}" "cli_changed=true"
assert_output "${output}" "has_artifacts=true"

echo "release_change_scope_test: ok"
