---
title: Use one hosted mailbox from your other devices
description: Pair each client with a private HTTPS host, approve its mail permissions, and open the same mailbox in the CLI, TUI, MCP, web or desktop app.
---

One computer runs the daemon and owns its mail providers, database and search index. Other computers pair with it through private HTTPS and request host approval for the mail workflows they need. The host must stay awake, online and running mxr.

Default pairings support reading, search and portable attachment downloads. Add host-approved grants for sending, mail organization and AI features. Web and desktop sessions inherit the grants of the paired installation that opens them. Host configuration, credentials, executable rules, process control, recovery administration and activity history stay on the host.

## Run the daemon and gateway on the host

Configure the host's mail account through the normal [quick start](/getting-started/quick-start/). Keep the daemon running in one terminal:

```sh
env -u MXR_DAEMON_ADDR mxr daemon --foreground
```

In a second terminal, preview the gateway address and then start it:

```sh
mxr serve --foreground --listen 127.0.0.1:18741 --dry-run --format json
mxr serve --foreground --listen 127.0.0.1:18741
```

The gateway binds only to loopback and uses this computer's local daemon. A saved remote selection does not change which daemon it serves. Keep both terminals open. Use the same mxr version on host and clients; the host daemon must report the same IPC protocol version as the client (`protocol_version` in `mxr status --format json`). Upgrade the host and clients together; old pairings retain read-only authority.

`mxr daemon` and `mxr restart` manage the current computer's local daemon even when a saved remote connection is selected. Run host lifecycle commands on the host computer.

## Forward private HTTPS to the gateway

Connect the host and clients to the same Tailscale network. With MagicDNS and HTTPS certificates enabled, run Tailscale Serve in another host terminal:

```sh
tailscale serve --https=8443 http://127.0.0.1:18741
```

Use the HTTPS origin printed by Serve, such as `https://mail-host.example-tailnet.ts.net:8443`. Choose an unused HTTPS port if the host already serves something else. Leave the foreground command running during use. Tailscale supplies the TLS certificate; mxr still requires each client to pair. See [Tailscale Serve's command reference](https://tailscale.com/docs/reference/tailscale-cli/serve/) for its HTTPS configuration and how to stop a specific forwarding rule.

Clients verify HTTPS certificates. The connection URL must be an HTTPS origin, with no username, password, path, query or fragment. Credentials are sent separately; redirects do not carry them to another origin.

## Pair each client with local host approval

On the client, preview the change:

```sh
mxr connect https://mail-host.example-tailnet.ts.net:8443 --name "My Mac" --dry-run --format json
```

Start pairing:

```sh
mxr connect https://mail-host.example-tailnet.ts.net:8443 --name "My Mac" --confirm --format json
```

The result includes a request ID and a full challenge. The connection is not selected yet. On the host, list requests and preview the matching request:

```sh
mxr serve pair list --format json
mxr serve pair approve REQUEST_ID --dry-run --format json
```

Compare the complete challenge on both computers. If they match, approve on the host:

```sh
mxr serve pair approve REQUEST_ID --confirm --format json
```

Repeat the client's confirmed `mxr connect` command to claim the registration and save the connection. It reports `selection_applied: true` after verifying the host identity, workspace and protocol. Repeating it again reuses that registration. Pair another client from its own installation; give it a distinct name so the host's list is useful.

A registration grants the supported reads across the host's mailbox accounts. Remote MCP uses that registration's grant; the host's local MCP profile restrictions do not narrow it. Pair only clients that should have this access.

Omitting `--confirm` previews without contacting the host or creating credentials. `--dry-run` and `--confirm` conflict. Unset `MXR_DAEMON_ADDR` before changing a saved connection; that environment variable temporarily overrides saved selection.

## Request host approval before sending

Preview the send grant on the client, then start the request:

```sh
mxr connect https://mail-host.example-tailnet.ts.net:8443 --allow-send --dry-run --format json
mxr connect https://mail-host.example-tailnet.ts.net:8443 --allow-send --confirm --format json
```

The result lists `mail_read` and `mail_send`. On the host, compare the full challenge and requested grants, then preview and approve the request as in the pairing steps. Repeat the client's confirmed command with `--allow-send` to claim it. An upgrade keeps the old read connection selected until the approved registration and workspace handshake are saved. It then retires that installation's old registration; other clients remain connected. A pending, refused or failed upgrade leaves the old selection intact.

`mail_send` authorizes draft, preview, portable attachment, calendar response, scheduled-send and reviewed send operations. It does not grant host configuration, credentials, filesystem access, process control, recovery administration or activity history. Remote MCP uses this registration-wide grant; local MCP profiles do not narrow it.

## Approve mail organization and AI features

Request the grants your client needs, then compare and approve the full challenge on the host:

```sh
mxr connect https://mail-host.example-tailnet.ts.net:8443 \
  --allow-send --allow-triage --allow-ai --dry-run --format json
mxr connect https://mail-host.example-tailnet.ts.net:8443 \
  --allow-send --allow-triage --allow-ai --confirm --format json
```

| Client flag | Host-approved grant | Workflows |
| --- | --- | --- |
| None | `mail_read` | Read, search, modes, todos, received attachments and mailbox status |
| `--allow-send` | `mail_send` | Drafts, portable uploads, signatures, snippets, previews, sending, scheduled sending and outcome recovery |
| `--allow-triage` | `mail_triage` | Archive, labels, snooze, mode moves, todo changes, reviewed batches and sync |
| `--allow-ai` | `mail_ai` | Summaries, writing help, mail questions and the assistant |

The grants are independent. AI access follows the host's configured models and privacy policy; it does not grant sending or mail changes. Applying writing help to a stored draft also needs `mail_send`. Assistant tools check the originating registration and its grants for every operation, including after a turn starts. Assistant history belongs to that paired installation: its browser sessions share the history, while local host conversations and other installations remain separate. Re-pairing creates a new registration and does not adopt the previous registration's conversations. Continue a remote conversation from its paired client; the host refuses to resume it with local tool privileges.

Model availability reflects the host. Remote status omits its endpoint addresses, environment-variable names and credential locations. Without `mail_ai`, the client reports AI as disabled. Configure models and keys on the host.

Use the same preview and confirmation flows as you do locally. For example, preview a mode move before committing it:

```sh
mxr reclassify MESSAGE_ID reading --dry-run --format json
mxr reclassify MESSAGE_ID reading --format json
```

## Save, preview and send a plain-text reply

Write the reply in your local `$EDITOR`, save it and review the exact saved revision:

```sh
mxr reply MESSAGE_ID --draft --attach ./report.pdf --format json
mxr send DRAFT_ID --dry-run --format json
```

The preview contains the draft, effective sender, attachment lengths and SHA-256 hashes, `expected_workspace`, `send_identity` and deterministic safety results. A preview writes no host draft, upload, safety audit or override token. `requires_save: true` means edited text or attachment declarations still need saving; preview the saved draft again before sending.

Send with the account, revision, operation ID and workspace from that review:

```sh
mxr send DRAFT_ID --account ACCOUNT_ID --expected-revision REVIEWED_REVISION \
  --operation-id ORIGINAL_OPERATION_ID \
  --expected-workspace '<OWNER_JSON_FROM_PREVIEW>' --format json
mxr send-outcome ORIGINAL_OPERATION_ID --account ACCOUNT_ID \
  --expected-workspace '<OWNER_JSON_FROM_PREVIEW>' --format json
```

Keep the original identity after a disconnect or host restart. An `accepted` outcome means the provider accepted the send; retrying that operation with the retained account returns the accepted result even after the draft was removed. `unknown` remains uncertain: query it and do not resend under a new operation ID. `not_accepted` requires a fresh review before a new attempt.

Outgoing attachment paths are read on the client and uploaded as bounded bytes. The host opens only managed resources. Reading, previewing and sending legacy drafts with host paths or inline path-backed assets are refused remotely; recreate them with portable uploads. Received attachments download to the client:

```sh
mxr attachments download MESSAGE_ID --dir ./downloads --format json
```

In the TUI, open a message, press `r`, edit locally, review **Draft Ready**, and press `s` to confirm sending. Opening mail still does not implicitly mark it read remotely. MCP provides `mxr_prepare_reply`, `mxr_save_draft`, the draft attachment tools, `mxr_draft_preview`, `mxr_send_draft` and `mxr_send_outcome` for the same journey; see the [MCP reference](/reference/mcp/).

## Open the hosted web or desktop app

On the host, restart the foreground gateway with the exact HTTPS origin printed by Tailscale Serve:

```sh
mxr serve --foreground --listen 127.0.0.1:18741 \
  --public-origin https://mail-host.example-tailnet.ts.net:8443 --dry-run --format json
mxr serve --foreground --listen 127.0.0.1:18741 \
  --public-origin https://mail-host.example-tailnet.ts.net:8443
```

The proxy must preserve the original `Host` header. Requests from another browser origin are refused. Keep the TLS proxy private to your tailnet.

On the paired client, open the selected host:

```sh
mxr web
```

Enter the single-use code printed in the terminal into the host's dedicated `/api/v1/remote/browser-launch` connection form. This form always opens, including when the browser profile already has another parent registration's session cookie. The code expires after two minutes. The browser receives a secure, HTTP-only session cookie lasting up to twelve hours; the native mail credential stays on the paired client. Neither credential appears in the launch URL. Run `mxr web` again to replace an expired code or session.

Launch the [desktop app](/guides/desktop-app/) on the paired computer to use the same selected host. It exchanges its single-use code at the selected HTTPS origin automatically. External links still open in your normal browser.

Browser sessions use the same mail grants and workspace checks as terminal clients. Inline images and received attachments travel as bounded bytes; compose attachments use portable uploads. Switching the parent installation away or revoking it denies subsequent browser requests and closes its event connection. Other independently paired clients keep their access.

## The CLI, TUI and MCP use the saved connection

```sh
mxr connect --status --format json
mxr search "is:unread" --format json
mxr cat MESSAGE_ID --format json
mxr
mxr mcp serve
```

Each new client process uses the saved selection. The remote TUI opens the mailbox; search and the reader use the same host. Connection status reports the selected target and pending deregistration count. It does not claim a live session: `session_state` is `not_inspected`.

The client never starts a local daemon as a fallback when the selected remote is unavailable. Restore host or network access and retry. If another process changes selection while a client is open, close and reopen that client before continuing. Keep private work with its original workspace.

## Recover a pending pairing without switching mailboxes

Inspect the current target and pending request first:

```sh
mxr connect --status --dry-run --format json
mxr connect --cancel-pairing --dry-run --format json
mxr connect --cancel-pairing --confirm --format json
```

`--cancel-pairing` retires only the unfinished request. It preserves the selected registration and retains cleanup intent if the host cannot acknowledge cancellation.

For an expired challenge, request a fresh one with the same intended grants:

```sh
mxr connect https://mail-host.example-tailnet.ts.net:8443 \
  --allow-send --allow-triage --allow-ai --restart-pairing --dry-run --format json
mxr connect https://mail-host.example-tailnet.ts.net:8443 \
  --allow-send --allow-triage --allow-ai --restart-pairing --confirm --format json
```

Compare and approve the new challenge on the host, then repeat the ordinary confirmed connection command with the same grants, **without** `--restart-pairing`, to claim it. Repeating the restart flag would retire that challenge and request another one. The previous selected mailbox stays selected until the new registration is approved, claimed and verified.

## Forget cleanup only when local retries should stop

Connection status reports `pending_deregistration_count` and `cleanup_retry_at` (Unix seconds). Failed cleanup attempts share a persisted delay across client processes, starting at thirty seconds and doubling up to one hour. Retiring another registration or pending pairing makes cleanup due again.

If an old host has been permanently removed or restored and can no longer acknowledge its former credentials, inspect the exact local intents before forgetting them:

```sh
mxr connect --forget-cleanup --dry-run --format json
mxr connect --forget-cleanup --confirm --format json
```

Forgetting preserves the selected connection and drops only the matching local retry intents captured by that invocation. It does not verify revocation on the old host. Revoke there separately if it is reachable. An invalid legacy pending request can first be retired with `--cancel-pairing`, then its cleanup intent can be forgotten; forgetting itself needs no network connection.

## Switch one client back to its local daemon

```sh
mxr connect --local --dry-run --format json
mxr connect --local --confirm --format json
mxr connect --status --format json
```

Switching retires only this installation's registration. Other independently paired clients continue using the host. Local selection takes effect even when the host is offline; the result reports pending deregistration, and the client retains enough information to retry it. Later client use or `mxr connect --status` retries pending cleanup when its persisted retry time is due. An unreachable host leaves the intent pending.

Switching does not copy the host's database to this computer or move drafts between workspaces. A local mailbox still needs its own local configuration. Cancelling an unfinished pairing with `--local --confirm` also retains cleanup until the host acknowledges it.

## Revoke access and recover a changed host

On the host:

```sh
mxr serve clients --format json
mxr serve clients revoke CLIENT_ID --dry-run --format json
mxr serve clients revoke CLIENT_ID --confirm --format json
```

The next operation on an already-open remote connection checks the current registration and rejects a revoked credential. Work admitted before revocation may finish. Revocation does not cancel previously accepted work.

Client credentials are stored in an owner-only local file using mxr's existing plaintext credential policy. Do not copy that file to make another client; pair the new installation independently. The host keeps verifiers rather than the client secrets. Logs and connection-status output omit native credentials. `mxr web` deliberately prints a short-lived browser code for the connection form; treat that code as private until it is consumed or expires.

A restored host workspace invalidates prior registrations. A changed host, workspace or protocol fails verification before a new saved selection is applied. Preserve the old client state and private work while investigating; switch away explicitly and pair again when the intended host is ready. See [pairing authority](/guides/pairing-authority/) and [backup recovery](/guides/backup-recovery/).
