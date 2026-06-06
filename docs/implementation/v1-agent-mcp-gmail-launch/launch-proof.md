# V1 launch proof

`scripts/v1_launch_proof.sh` is the deterministic, no-network launch gate for v1.
It creates an isolated `MXR_DATA_DIR`/`MXR_CONFIG_DIR`, configures the fake sync/send provider, invokes a real `mxr` binary, auto-starts the daemon, syncs, searches, reads, saves a draft, proves a real daemon IPC request tagged `source=agent` can read/save drafts but cannot send or mutate destructively, lists MCP tools, calls MCP read, proves MCP send is blocked without `confirm=true`, sends the same draft through the MCP gated path, and previews/applies a mutation.

## Local run

From the repo root after building the binary:

```bash
cargo build -p mxr
MXR_BIN=target/debug/mxr bash scripts/v1_launch_proof.sh
```

To validate a release artifact, unpack it and point `MXR_BIN` at the unpacked binary:

```bash
MXR_BIN=/path/to/release/mxr bash scripts/v1_launch_proof.sh
```

The script prints and writes JSONL proof rows to `MXR_PROOF_ARTIFACT` when set, otherwise to a temp-file path. Rows include step names, status, ids, and counts only; they intentionally omit credentials and full message/draft bodies.

## Live provider evidence

The deterministic proof never needs secrets. Optional live provider checks are CI-safe:

```bash
bash scripts/live_provider_smoke_evidence.sh
```

The evidence script writes JSONL rows to `MXR_LIVE_PROVIDER_EVIDENCE` (or a temp path) and prints `skipped_missing_creds` for each provider whose credential set is incomplete. It records env var names only, never secret values.

- Gmail API/OAuth: requires `MXR_GMAIL_TEST_CLIENT_ID`, `MXR_GMAIL_TEST_CLIENT_SECRET`, and `MXR_GMAIL_TEST_REFRESH_TOKEN`. When all are present the script runs `scripts/cargo-test -p mxr --test live_gmail_e2e -- --ignored --nocapture` against a dedicated throwaway Google account and emits `live_smoke_passed` only after it succeeds.
- IMAP: requires `MXR_IMAP_SMOKE_HOST`, `MXR_IMAP_SMOKE_USERNAME`, and `MXR_IMAP_SMOKE_PASSWORD`; missing values produce an explicit `skipped_missing_creds` row. If all values are present before a committed network-safe IMAP smoke exists, the script emits explicit `unavailable_no_live_smoke` evidence instead of silent success.
- SMTP: requires `MXR_SMTP_SMOKE_HOST`, `MXR_SMTP_SMOKE_USERNAME`, `MXR_SMTP_SMOKE_PASSWORD`, and `MXR_SMTP_SMOKE_TO`; missing values produce an explicit `skipped_missing_creds` row. If all values are present before a committed network-safe SMTP smoke exists, the script emits explicit `unavailable_no_live_smoke` evidence instead of silent success.

Release and provider smoke workflows run/reference the deterministic proof and live-provider evidence so launch status is visible before cutting v1.
