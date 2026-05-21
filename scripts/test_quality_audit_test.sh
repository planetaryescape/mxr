#!/usr/bin/env bash

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT
strict_fail_out="${tmp}/strict-fail.out"
strict_pass_out="${tmp}/strict-pass.out"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  if ! grep -Fq "${needle}" <<< "${haystack}"; then
    echo "missing expected output: ${needle}" >&2
    echo "${haystack}" >&2
    return 1
  fi
}

mkdir -p "${tmp}/repo"
cd "${tmp}/repo"
git init -q
git config user.email "ci@example.test"
git config user.name "CI"

mkdir -p scripts crates/demo/src apps/web/src
cp "${root}/scripts/test_quality_audit.sh" scripts/test_quality_audit.sh

cat > crates/demo/src/lib.rs <<'EOF'
#[test]
fn accepts_anything() {
    assert!(true);
}
EOF

cat > apps/web/src/Widget.test.tsx <<'EOF'
import { expect, test, vi } from "vitest";

vi.mock("./api", () => ({ load: vi.fn() }));

test("renders widget", () => {
  expect("id").toBeTruthy();
});
EOF

output="$(bash scripts/test_quality_audit.sh)"
assert_contains "${output}" "Wrote target/test-quality/audit.csv"
assert_contains "$(cat target/test-quality/audit.md)" "web-src"
assert_contains "$(cat target/test-quality/audit.md)" "ceremony_heavy"

if bash scripts/test_quality_audit.sh --strict >"${strict_fail_out}" 2>&1; then
  echo "strict audit should fail on vacuous tests" >&2
  exit 1
fi
assert_contains "$(cat "${strict_fail_out}")" "strict mode failed"

cat > crates/demo/src/lib.rs <<'EOF'
pub fn classify_count(count: usize) -> &'static str {
    if count == 0 {
        "empty"
    } else if count > 99 {
        "overflow"
    } else {
        "normal"
    }
}

#[test]
fn classify_count_distinguishes_empty_normal_and_overflow_cases() {
    assert_eq!(classify_count(0), "empty");
    assert_eq!(classify_count(42), "normal");
    assert_eq!(classify_count(100), "overflow");
}
EOF

rm -f apps/web/src/Widget.test.tsx
bash scripts/test_quality_audit.sh --strict >"${strict_pass_out}"
assert_contains "$(cat "${strict_pass_out}")" "Wrote target/test-quality/audit.md"

echo "test_quality_audit_test: ok"
