# Recover cached mail while old work stays held

Stop every process using the source profile, then preview a backup. Offline maintenance coordinates with mxr versions that implement the workspace lease. An older executable does not participate; stop it and do not restart it during maintenance. Downgrading a restored profile is unsupported.

```sh
mxr backup create --source-data "$SOURCE_DATA" \
  --source-config "$SOURCE_CONFIG/config.toml" --output "$BUNDLE"
mxr backup create --source-data "$SOURCE_DATA" \
  --source-config "$SOURCE_CONFIG/config.toml" --output "$BUNDLE" --confirm
mxr backup inspect "$BUNDLE"
```

The commands print JSON. Preview performs the same inventory, file and database validation as creation. Omit `--source-config` to include DB account metadata only; mxr never substitutes your current default config. The bundle preserves cached mail, durable local state, draft text and managed attachment bytes. It omits credentials, authorization tokens, activity and its search/filter data, logs, models and derived indexes. Preserve the bundle privately: its mail and drafts remain personal data.

Required legacy files outside the source workspace cause an incomplete-backup error. Import those files as managed draft resources before retrying. Missing files, unfinished uploads, changed resource hashes, symlinks and unsupported schemas also stop creation. Never treat an interrupted staging directory as a bundle.

## Restore into an absent or empty directory

```sh
mxr backup restore "$BUNDLE" --destination "$RESTORED_DATA"
mxr backup restore "$BUNDLE" --destination "$RESTORED_DATA" --confirm
export MXR_DATA_DIR="$RESTORED_DATA"
export MXR_CONFIG_DIR="$RESTORED_DATA"
mxr daemon --foreground
```

Run subsequent client commands in another terminal with those same environment variables. The restored `config.toml` disables accounts and external processors. Reprovision credentials and connection settings separately. When a source config was supplied, only audited account connection metadata and locale/default-account settings were retained; credential references were replaced.

```sh
mxr recovery status --format json
mxr search 'label:inbox' --format json
```

The workspace ID remains the same and its recovery generation changes. Cached reads and local draft editing remain available. Sync, IDLE, provider mutations, authentication probes, automatic rules/hooks, model calls and scheduled dispatch stay paused. A stale client owner is refused, preserving its private work.

Recovery status lists held drafts and retired intentions. Retained archive evidence preserves original item outcomes. Provider-draft links are detached from restored originals so upstream deletion or edits cannot replace local recovery content. The original draft IDs remain permanently held even after activation; old schedules, rules and archive intentions do not resume.

## Activate new work after stopping the source owner

A snapshot may predate a successful send. Missing evidence does not establish that a provider rejected it. Inspect retained outcomes and keep uncertainty blocked. Stop the prior automation owner before confirming activation; mxr does not discover or elect owners across machines.

Copy the owner from this restored profile's recovery status:

```sh
OWNER=$(mxr recovery status --format json | jq -c '.recovery | {workspace_id,recovery_generation}')
mxr recovery activate --expected-workspace "$OWNER" --format json
mxr recovery activate --expected-workspace "$OWNER" \
  --source-owner-stopped --confirm --format json
```

Activation permits new work and never releases an original draft ID for sending. Preview preparing a separate local draft from a held original, then explicitly confirm the same owner and revision:

```sh
mxr recovery prepare-draft "$ORIGINAL_DRAFT_ID" \
  --expected-revision "$ORIGINAL_REVISION" --expected-workspace "$OWNER" --format json
mxr recovery prepare-draft "$ORIGINAL_DRAFT_ID" \
  --expected-revision "$ORIGINAL_REVISION" --expected-workspace "$OWNER" \
  --confirm --format json
```

Preparation checks the attachment bytes during both preview and execution. It creates a fresh draft and resource ownership, retains the original content/evidence, and returns the same prepared ID on retries. Review the new draft through the ordinary send preview before sending. It carries no old schedule, provider link, send outcome or Message-ID.

Native TUI diagnostics and the web recovery panel expose these same previews and explicit confirmations. MCP exposes `mxr_recovery_status`, `mxr_recovery_activate` and `mxr_recovery_prepare_draft`; its configured profile must permit the action and all affected accounts. Restricted status responses omit other accounts' held subjects and retained evidence.

Recovery requests use protocol 12. Upgrade CLI, native TUI, MCP and daemon together. Published protocol-11 peers are rejected explicitly; selected targets never fall back to a different local daemon. The web bridge stays beside its daemon and uses that daemon's protocol.

## Verify with synthetic profiles

```sh
python3 scripts/verify-backup-recovery.py --binary target-cli/debug/mxr \
  --released-binary /opt/homebrew/bin/mxr --source "$SOURCE_IDENTITY" \
  --runtime target/s09-evidence/acceptance
```

The harness retains the bundle and stopped destination for independent client acceptance. It uses isolated homes, fake mail and owned processes; its result records byte hashes, identity changes, paused loop behavior and newly reviewed send evidence.
