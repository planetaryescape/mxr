#!/usr/bin/env bash

set -euo pipefail

artifact="${MXR_LIVE_PROVIDER_EVIDENCE:-${RUNNER_TEMP:-${TMPDIR:-/tmp}}/mxr-live-provider-smoke-evidence.$(date -u +%Y%m%dT%H%M%SZ).jsonl}"
mkdir -p "$(dirname "${artifact}")"
: >"${artifact}"

emit() {
  local extra="${4-}"
  if [[ -z "${extra}" ]]; then
    extra='{}'
  fi
  python3 - "$artifact" "$1" "$2" "$3" "$extra" <<'PY'
import json, sys, time
path, provider, status, required, raw_extra = sys.argv[1:6]
extra = json.loads(raw_extra or '{}')
row = {
    "ts": time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()),
    "provider": provider,
    "status": status,
    "required_env": required.split(',') if required else [],
}
row.update(extra)
with open(path, 'a', encoding='utf-8') as f:
    f.write(json.dumps(row, sort_keys=True) + '\n')
print(json.dumps(row, sort_keys=True))
PY
}

has_all() {
  for name in "$@"; do
    if [[ -z "${!name:-}" ]]; then
      return 1
    fi
  done
}

required_csv() {
  local IFS=,
  echo "$*"
}

gmail_env=(MXR_GMAIL_TEST_CLIENT_ID MXR_GMAIL_TEST_CLIENT_SECRET MXR_GMAIL_TEST_REFRESH_TOKEN)
imap_env=(MXR_IMAP_SMOKE_HOST MXR_IMAP_SMOKE_USERNAME MXR_IMAP_SMOKE_PASSWORD)
smtp_env=(MXR_SMTP_SMOKE_HOST MXR_SMTP_SMOKE_USERNAME MXR_SMTP_SMOKE_PASSWORD MXR_SMTP_SMOKE_TO)

if has_all "${gmail_env[@]}"; then
  emit gmail creds_available "$(required_csv "${gmail_env[@]}")" '{"live_smoke":"live_gmail_e2e"}'
  scripts/cargo-test -p mxr --test live_gmail_e2e -- --ignored --nocapture
  emit gmail live_smoke_passed "$(required_csv "${gmail_env[@]}")" '{"live_smoke":"live_gmail_e2e"}'
else
  emit gmail skipped_missing_creds "$(required_csv "${gmail_env[@]}")"
fi

if has_all "${imap_env[@]}"; then
  emit imap unavailable_no_live_smoke "$(required_csv "${imap_env[@]}")" '{"reason":"no committed network-safe IMAP live smoke test yet"}'
else
  emit imap skipped_missing_creds "$(required_csv "${imap_env[@]}")"
fi

if has_all "${smtp_env[@]}"; then
  emit smtp unavailable_no_live_smoke "$(required_csv "${smtp_env[@]}")" '{"reason":"no committed network-safe SMTP live smoke test yet"}'
else
  emit smtp skipped_missing_creds "$(required_csv "${smtp_env[@]}")"
fi

echo "live provider smoke evidence artifact: ${artifact}"
