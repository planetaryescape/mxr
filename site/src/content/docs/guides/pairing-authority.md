---
title: Inspect and revoke local pairing authority
description: Preview local pairing approvals and revoke registered credentials. Remote hosting is not yet available.
---

If the local daemon is stopped, run it in a separate terminal:

```sh
env -u MXR_DAEMON_ADDR mxr daemon --foreground
```

Then inspect its pairing authority from your working terminal:

```sh
mxr serve status --format json
mxr serve pair list --format json
mxr serve clients --format json
```

Inspect the current computer's pairing authority through its local Unix socket. These commands ignore `MXR_DAEMON_ADDR` and do not start or restart a daemon. Before the first pairing request, `host_id` is `null`; inspecting status or a list does not create authority.

This release provides pairing and revocation foundations. Remote mailbox access, HTTPS hosting, saved client connections and service installation are not yet available. Running `mxr serve` without a subcommand returns an explicit error.

## Approval previews the exact request

```sh
mxr serve pair approve REQUEST_ID --dry-run --format json
mxr serve pair approve REQUEST_ID --confirm --format json
```

Compare the full `challenge` with the requesting client's displayed challenge before confirming. The preview includes the request ID, display name, requested `capabilities`, expiry, approval state and failed-attempt count. The supported grant is `mail_read`. A later write capability requires a new explicit approval.

Omitting `--confirm` also previews. `--dry-run` and `--confirm` are mutually exclusive. A request expires after ten minutes; up to sixteen pending requests may exist, and five failed possession proofs lock a request. The current CLI lists and approves requests created through typed IPC; it does not provide a remote pairing screen or create client secrets.

Approval alone creates no registration. Claiming proves possession of the two distinct client-generated secrets, then atomically creates one registration. Retrying a successful claim returns the same active registration, including after the original response was lost. A revoked or expired registration cannot be recovered by replaying that claim.

## Revocation commits before it acknowledges

```sh
mxr serve clients revoke CLIENT_ID --dry-run --format json
mxr serve clients revoke CLIENT_ID --confirm --format json
mxr serve clients --format json
```

Preview the exact client ID before confirming. The committed result includes `revoked_at`. Revocation removes mail authority and clears its verifier and display name; it does not cancel accepted mail work. Repeating revocation is safe.

Mail credentials last ninety days. A separate cleanup credential can only revoke its own registration. Its verifier remains after expiry and revocation so an offline client can retry cleanup when a response was lost. Cleanup credentials never authorize mail access. No child registrations exist in this foundation.

## Restart preserves authority and portable restore clears it

A daemon restart preserves its host identity, pending approvals until their expiry, and unexpired registrations. A portable backup excludes host authority, pending pairings and registration verifiers. Restore clears remote authority and rotates the workspace recovery generation while keeping the existing paused-owner recovery protections. Pair again after restore.

CLI JSON, logs and activity records do not contain client secrets or verifiers. The server stores only SHA-256 verifiers. Protocol 13 is required for these new commands; upgrade the local binary and daemon together. Remote transports continue to reject incompatible protocols without restarting the foreign host.

See the [CLI reference](/reference/cli/serve/) for command arguments and the [backup and recovery guide](/guides/backup-recovery/) for restored workspace activation.
