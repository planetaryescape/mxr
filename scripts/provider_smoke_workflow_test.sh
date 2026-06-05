#!/usr/bin/env bash

set -euo pipefail

workflow=".github/workflows/provider-live-smoke.yml"

if [[ ! -f "${workflow}" ]]; then
  echo "missing workflow: ${workflow}" >&2
  exit 1
fi

install_count="$({ grep -F "libasound2-dev libdbus-1-dev pkg-config" "${workflow}" || true; } | wc -l | tr -d ' ')"
if [[ "${install_count}" -lt 2 ]]; then
  echo "provider smoke workflow must install libasound2-dev, libdbus-1-dev, and pkg-config before Rust provider smoke jobs" >&2
  exit 1
fi

proof_script="scripts/v1_launch_proof.sh"
if [[ ! -x "${proof_script}" ]]; then
  echo "missing executable deterministic v1 launch proof script: ${proof_script}" >&2
  exit 1
fi

live_evidence_script="scripts/live_provider_smoke_evidence.sh"
if [[ ! -x "${live_evidence_script}" ]]; then
  echo "missing executable live provider evidence script: ${live_evidence_script}" >&2
  exit 1
fi

if ! grep -Fq "scripts/v1_launch_proof.sh" "${workflow}"; then
  echo "provider smoke workflow must run the deterministic v1 launch proof" >&2
  exit 1
fi

release_workflow=".github/workflows/release.yml"
if [[ ! -f "${release_workflow}" ]] || ! grep -Fq "scripts/v1_launch_proof.sh" "${release_workflow}"; then
  echo "release workflow must run or reference the deterministic v1 launch proof" >&2
  exit 1
fi

for wf in "${workflow}" "${release_workflow}"; do
  if ! grep -Fq "scripts/live_provider_smoke_evidence.sh" "${wf}"; then
    echo "${wf} must record explicit live provider credential evidence" >&2
    exit 1
  fi
done

echo "provider_smoke_workflow_test: ok"
