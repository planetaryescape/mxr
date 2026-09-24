#!/usr/bin/env bash

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

mkdir -p "${tmp}/bin"

cat > "${tmp}/bin/cargo" <<'EOF'
#!/usr/bin/env bash
printf '%s\0' "$@" > "${CARGO_TEST_ARGS_FILE}"
printf '%s' "${CARGO_TARGET_DIR}" > "${CARGO_TEST_TARGET_FILE}"
exit "${CARGO_TEST_EXIT_CODE}"
EOF

for command in pgrep ps; do
  cat > "${tmp}/bin/${command}" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$(basename "$0")" >> "${PROCESS_INSPECTION_FILE}"
exit 0
EOF
done

chmod +x "${tmp}/bin/cargo" "${tmp}/bin/pgrep" "${tmp}/bin/ps"

args_file="${tmp}/args"
expected_args_file="${tmp}/expected-args"
target_file="${tmp}/target"
inspection_file="${tmp}/process-inspection"
stderr_file="${tmp}/stderr"

set +e
PATH="${tmp}/bin:${PATH}" \
  CARGO_TEST_ARGS_FILE="${args_file}" \
  CARGO_TEST_TARGET_FILE="${target_file}" \
  CARGO_TEST_EXIT_CODE=17 \
  PROCESS_INSPECTION_FILE="${inspection_file}" \
  "${root}/scripts/cargo-test" -p "crate with spaces" --workspace "test name" \
  2> "${stderr_file}"
status=$?
set -e

if [[ "${status}" -ne 17 ]]; then
  echo "cargo-test must preserve cargo's exit code; got ${status}" >&2
  exit 1
fi

printf '%s\0' test -p "crate with spaces" --workspace "test name" > "${expected_args_file}"
if ! cmp -s "${expected_args_file}" "${args_file}"; then
  echo "cargo-test changed argument boundaries" >&2
  exit 1
fi

if [[ "$(< "${target_file}")" != "${root}/target-cli" ]]; then
  echo "cargo-test must use the checkout-local target-cli directory" >&2
  exit 1
fi

if [[ -e "${inspection_file}" ]]; then
  echo "cargo-test inspected machine processes: $(< "${inspection_file}")" >&2
  exit 1
fi

if ! grep -Fq -- "warning: --workspace runs the full 14-crate suite" "${stderr_file}"; then
  echo "cargo-test must retain the --workspace warning" >&2
  exit 1
fi

echo "cargo_test_wrapper_test: ok"
