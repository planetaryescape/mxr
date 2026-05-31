#!/usr/bin/env bash
set -euo pipefail

workflow=".github/workflows/ci.yml"

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
