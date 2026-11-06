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

Pairing and revocation authority first shipped in version 0.6.94. The [remote mailbox guide](/guides/remote-mailbox/) adds a foreground gateway and saved client connections for reading, mail workflows, browser sessions and explicitly approved replies. Use `mxr serve --foreground` to run the gateway; bare `mxr serve` does not start it.

## Approval previews the exact request

```sh
mxr serve pair approve REQUEST_ID --dry-run --format json
mxr serve pair approve REQUEST_ID --confirm --format json
```

Compare the full `challenge` with the requesting client's displayed challenge before confirming. The preview includes the request ID, display name, requested `capabilities`, expiry, approval state and failed-attempt count. Default requests contain `mail_read`. Add `--allow-send` for `mail_send` (drafts, portable uploads, previews and sending), `--allow-triage` for `mail_triage` (mail organization, modes, todos and sync), or `--allow-ai` for `mail_ai` (summaries, writing help and the assistant). Combine flags when a client needs several grants. Review both the challenge and grants before approving; an existing read-only registration cannot upgrade itself.

Omitting `--confirm` also previews. `--dry-run` and `--confirm` are mutually exclusive. A request expires after ten minutes; up to sixteen pending requests may exist, and five failed possession proofs lock a request. Clients create requests with `mxr connect HTTPS_ORIGIN --confirm`; approval stays on the host's local socket.

Approval alone creates no registration. Claiming proves possession of the two distinct client-generated secrets, then atomically creates one registration. Retrying a successful claim returns the same active registration, including after the original response was lost. A revoked or expired registration cannot be recovered by replaying that claim.

## Revocation commits before it acknowledges

```sh
mxr serve clients revoke CLIENT_ID --dry-run --format json
mxr serve clients revoke CLIENT_ID --confirm --format json
mxr serve clients --format json
```

Preview the exact client ID before confirming. The committed result includes `revoked_at`. Revocation removes mail authority and clears its verifier and display name; it does not cancel accepted mail work. Repeating revocation is safe.

Mail credentials last ninety days. A separate cleanup credential can only revoke its own registration. Its verifier remains after expiry and revocation so an offline client can retry cleanup when a response was lost. Cleanup credentials never authorize mail access. Browser sessions are children of the paired installation. A single-use code expires after two minutes; its secure, HTTP-only cookie lasts at most thirty days and cannot issue another child. Every browser request checks the parent registration and inherits its grants.

## Restart preserves authority and portable restore clears it

A daemon restart preserves its host identity, pending approvals until their expiry, and unexpired registrations. A portable backup excludes host authority, pending pairings, registration verifiers, browser codes and browser session verifiers. Restore clears remote authority and rotates the workspace recovery generation while keeping the existing paused-owner recovery protections. Pair again after restore.

Connection-status JSON, logs and activity records omit native client secrets and verifiers. `mxr web` prints a private, short-lived browser code for the connection form. The server stores only SHA-256 verifiers. The host daemon and clients must use the same IPC protocol; upgrade the binaries together. Remote transports reject incompatible protocols without restarting the foreign host.

See the [CLI reference](/reference/cli/serve/) for command arguments and the [backup and recovery guide](/guides/backup-recovery/) for restored workspace activation.
