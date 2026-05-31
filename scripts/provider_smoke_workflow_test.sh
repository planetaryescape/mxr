#!/usr/bin/env bash

set -euo pipefail

workflow=".github/workflows/provider-live-smoke.yml"

if [[ ! -f "${workflow}" ]]; then
  echo "missing workflow: ${workflow}" >&2
  exit 1
fi

install_count="$({ grep -F "libasound2-dev pkg-config" "${workflow}" || true; } | wc -l | tr -d ' ')"
if [[ "${install_count}" -lt 2 ]]; then
  echo "provider smoke workflow must install libasound2-dev and pkg-config before Rust provider smoke jobs" >&2
  exit 1
fi

echo "provider_smoke_workflow_test: ok"
