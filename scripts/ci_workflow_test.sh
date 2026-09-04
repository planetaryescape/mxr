#!/usr/bin/env bash
set -euo pipefail

workflow=".github/workflows/ci.yml"

changes_block="$(sed -n '/^  changes:/,/^  deny:/p' "${workflow}")"
classifier_command="scripts/ci_change_scope.sh \"\${EVENT_NAME}\" \"\${base}\" \"\${head}\" >> \"\${GITHUB_OUTPUT}\""
if ! grep -Fq "${classifier_command}" <<< "${changes_block}"; then
    echo "CI must execute the tested change-scope classifier." >&2
    exit 1
fi

if grep -Fq 'git diff --name-only' <<< "${changes_block}"; then
    echo "CI must not keep a second inline change classifier." >&2
    exit 1
fi

release_gates_block="$(sed -n '/^  release-gates:/,/^  fmt:/p' "${workflow}")"
for test_script in scripts/cargo_test_wrapper_test.sh scripts/ci_change_scope_test.sh scripts/ci_workflow_test.sh; do
    if ! grep -Fq "bash ${test_script}" <<< "${release_gates_block}"; then
        echo "Release Gate Scripts must run ${test_script}." >&2
        exit 1
    fi
done

web_tests_block="$(sed -n '/^  web-tests:/,/^  web-e2e-smoke:/p' "${workflow}")"
if ! grep -Fq "if: needs.changes.outputs.web == 'true' || needs.changes.outputs.expensive == 'true'" <<< "${web_tests_block}"; then
    echo "Web Unit Tests must use the web change-scope output." >&2
    exit 1
fi

if ! grep -Fq 'node-version: 24' <<< "${web_tests_block}"; then
    echo "Web Unit Tests must use Node 24." >&2
    exit 1
fi

if ! grep -Fq 'id: install' <<< "${web_tests_block}"; then
    echo "Web Unit Tests must identify the shared install prerequisite." >&2
    exit 1
fi

for command in \
    'npm audit --audit-level=moderate' \
    'npm run typecheck' \
    'npm run lint' \
    'npm run test'; do
    if ! grep -Fq "run: ${command}" <<< "${web_tests_block}"; then
        echo "Web Unit Tests must run ${command}." >&2
        exit 1
    fi
done

independent_condition="if: \${{ !cancelled() && steps.install.outcome == 'success' }}"
condition_count="$(grep -Fc "${independent_condition}" <<< "${web_tests_block}")"
if [[ "${condition_count}" -ne 4 ]]; then
    echo "Audit, typecheck, lint, and unit tests must run independently after install." >&2
    exit 1
fi

if grep -Fq 'continue-on-error:' <<< "${web_tests_block}"; then
    echo "Web Unit Tests failures must retain their normal failure outcomes." >&2
    exit 1
fi

if ! grep -Fq 'go install github.com/zricethezav/gitleaks/v8@v8.30.1' "$workflow"; then
    echo "CI must install gitleaks from the module path declared by upstream go.mod." >&2
    exit 1
fi

sqlx_block="$(sed -n '/name: SQLx Offline/,/  docs:/p' "$workflow")"
if ! grep -Fq 'libasound2-dev libdbus-1-dev pkg-config' <<<"$sqlx_block"; then
    echo "SQLx Offline must install ALSA, DBus, and pkg-config deps before workspace checks." >&2
    exit 1
fi

echo "ci_workflow_test: ok"
