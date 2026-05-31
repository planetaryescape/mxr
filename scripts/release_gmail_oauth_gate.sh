#!/usr/bin/env bash

set -euo pipefail

client_id="${GMAIL_CLIENT_ID:-}"
client_secret="${GMAIL_CLIENT_SECRET:-}"
verified="${GMAIL_OAUTH_VERIFICATION_CONFIRMED:-}"

set_build_env() {
  local output_client_id="$1"
  local output_client_secret="$2"

  if [[ -n "${GITHUB_ENV:-}" ]]; then
    {
      echo "GMAIL_CLIENT_ID=${output_client_id}"
      echo "GMAIL_CLIENT_SECRET=${output_client_secret}"
    } >> "${GITHUB_ENV}"
  fi
}

if [[ -z "${client_id}" && -z "${client_secret}" ]]; then
  set_build_env "" ""
  echo "Bundled Gmail OAuth client omitted; BYOC remains the production Gmail path."
  exit 0
fi

if [[ -z "${client_id}" || -z "${client_secret}" ]]; then
  echo "::error ::GMAIL_CLIENT_ID and GMAIL_CLIENT_SECRET must be set together or both omitted." >&2
  exit 1
fi

if [[ "${verified}" != "true" ]]; then
  set_build_env "" ""
  echo "::warning ::Bundled Gmail OAuth credentials are present but not verified; omitting them from release artifacts. BYOC remains the production Gmail path." >&2
  exit 0
fi

set_build_env "${client_id}" "${client_secret}"
echo "Bundled Gmail OAuth verification gate passed."
