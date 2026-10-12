---
title: Back up and restore a workspace
description: Create a consistent local backup and restore mail and drafts with provider work paused until you review it.
---

Use mxr's backup commands instead of copying a live database. Stop every
process using the source profile before previewing or creating a backup. Older
mxr executables do not participate in the workspace lease; do not restart one
during backup or restore.

## Preview and create a backup

Set `SOURCE_DATA` to the stopped profile's data directory and `SOURCE_CONFIG`
to its config directory. Choose a new bundle path for `BUNDLE` and an absent
or empty destination for `RESTORED_DATA`.

```bash
mxr backup create --source-data "$SOURCE_DATA" \
  --source-config "$SOURCE_CONFIG/config.toml" --output "$BUNDLE"
mxr backup create --source-data "$SOURCE_DATA" \
  --source-config "$SOURCE_CONFIG/config.toml" --output "$BUNDLE" --confirm
mxr backup inspect "$BUNDLE"
```

The first command validates and previews the inventory. The confirmed command
creates the bundle. The bundle contains cached mail, durable local state, draft
text, and managed attachment bytes. It excludes credentials, authorization
tokens, activity data, logs, model files, and derived indexes. Treat the bundle
as private mail data.

If you omit `--source-config`, the bundle contains database account metadata
only. mxr does not substitute the current default config. Required legacy files
outside the source workspace, unfinished uploads, changed resource hashes,
symlinks, missing files, and unsupported schemas stop backup creation.

## Restore into a new profile

Restore only into an absent or empty destination. Preview first, then confirm:

```bash
mxr backup restore "$BUNDLE" --destination "$RESTORED_DATA"
mxr backup restore "$BUNDLE" --destination "$RESTORED_DATA" --confirm
export MXR_DATA_DIR="$RESTORED_DATA"
export MXR_CONFIG_DIR="$RESTORED_DATA"
mxr daemon --foreground
```

Leave the daemon running in that terminal. Run the following client commands
in another terminal with the same `MXR_DATA_DIR` and `MXR_CONFIG_DIR` values.

The restored profile keeps the workspace ID and receives a new recovery
generation. Its accounts and external processors are disabled; credential
references are removed. Reprovision connection settings and credentials
separately.

Cached reads and local draft editing remain available. Provider sync, IDLE,
provider mutations, authentication probes, rules, hooks, model calls, and
scheduled dispatch stay paused. Original draft IDs remain held, and old
schedules and archive intentions do not resume. Check the restored state:

```bash
mxr recovery status --format json
mxr search 'label:inbox' --format json
```

## Activate only after the old owner has stopped

Snapshots may predate a successful send. Missing evidence does not prove that
the provider rejected it. Inspect retained outcomes and resolve uncertainty
before activating. mxr does not discover or elect owners across machines; stop
the previous automation owner first.

Use the exact workspace and recovery generation returned by the restored
profile. Preview activation, then confirm it with the same owner:

```bash
OWNER=$(mxr recovery status --format json | jq -c '.recovery | {workspace_id,recovery_generation}')
mxr recovery activate --expected-workspace "$OWNER" --format json
mxr recovery activate --expected-workspace "$OWNER" \
  --source-owner-stopped --confirm --format json
```

Activation permits new work but never releases an original draft ID for
sending. To continue a held draft, preview a separate local draft and confirm
the same source revision and owner. Set `ORIGINAL_DRAFT_ID` and
`ORIGINAL_REVISION` from the held draft shown by `mxr recovery status`:

```bash
mxr recovery prepare-draft "$ORIGINAL_DRAFT_ID" \
  --expected-revision "$ORIGINAL_REVISION" \
  --expected-workspace "$OWNER" --format json
mxr recovery prepare-draft "$ORIGINAL_DRAFT_ID" \
  --expected-revision "$ORIGINAL_REVISION" \
  --expected-workspace "$OWNER" --confirm --format json
```

The original held draft retains its evidence and attachment bytes. The
prepared draft is separate and has no old schedule, provider link, send
outcome, or Message-ID. Review it through the ordinary send preview before
sending. TUI, web, and MCP expose the same recovery previews and explicit
confirmations.

Upgrade the daemon, CLI, TUI, and MCP client together. Recovery uses IPC
protocol 12; protocol-11 peers are rejected explicitly. The web bridge stays
beside its daemon and uses that daemon's protocol. This workflow creates a local
backup; it does not configure remote HTTP access, a VPS, or a phone client.
