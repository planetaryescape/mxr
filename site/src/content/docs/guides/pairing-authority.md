---
title: Inspect and revoke local pairing authority
description: Preview local pairing approvals and revoke registered credentials on the daemon host.
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

`hosting_available` reports whether the installed build supports serving remote clients. It does not report whether a foreground gateway is running or reachable.

Pairing and revocation authority first shipped in version 0.6.94. The [remote mailbox guide](/guides/remote-mailbox/) adds a foreground gateway and saved client connections for reading and search. Service installation remains separate work. Use `mxr serve --foreground` to run the gateway; bare `mxr serve` does not start it.

## Approval previews the exact request

```sh
mxr serve pair approve REQUEST_ID --dry-run --format json
mxr serve pair approve REQUEST_ID --confirm --format json
```

Compare the full `challenge` with the requesting client's displayed challenge before confirming. The preview includes the request ID, display name, requested `capabilities`, expiry, approval state and failed-attempt count. The supported grant is `mail_read`. A later write capability requires a new explicit approval.

Omitting `--confirm` also previews. `--dry-run` and `--confirm` are mutually exclusive. A request expires after ten minutes; up to sixteen pending requests may exist, and five failed possession proofs lock a request. Clients create requests with `mxr connect HTTPS_ORIGIN --confirm`; approval stays on the host's local socket.

Approval alone creates no registration. Claiming proves possession of the two distinct client-generated secrets, then atomically creates one registration. Retrying a successful claim returns the same active registration, including after the original response was lost. A revoked or expired registration cannot be recovered by replaying that claim.

## Revocation commits before it acknowledges

```sh
mxr serve clients revoke CLIENT_ID --dry-run --format json
mxr serve clients revoke CLIENT_ID --confirm --format json
mxr serve clients --format json
```

Preview the exact client ID before confirming. The committed result includes `revoked_at`. Revocation removes mail authority and clears its verifier and display name; it does not cancel accepted mail work. Repeating revocation is safe.

Mail credentials last ninety days. A separate cleanup credential can only revoke its own registration. Its verifier remains after expiry and revocation so an offline client can retry cleanup when a response was lost. Cleanup credentials never authorize mail access. Browser child registrations are not yet supported.

## Restart preserves authority and portable restore clears it

A daemon restart preserves its host identity, pending approvals until their expiry, and unexpired registrations. A portable backup excludes host authority, pending pairings and registration verifiers. Restore clears remote authority and rotates the workspace recovery generation while keeping the existing paused-owner recovery protections. Pair again after restore.

CLI JSON, logs and activity records do not contain client secrets or verifiers. The server stores only SHA-256 verifiers. The foreground gateway and saved connections require protocol 14; upgrade the local binary and daemon together. Remote transports reject incompatible protocols without restarting the foreign host.

See the [CLI reference](/reference/cli/serve/) for command arguments and the [backup and recovery guide](/guides/backup-recovery/) for restored workspace activation.
