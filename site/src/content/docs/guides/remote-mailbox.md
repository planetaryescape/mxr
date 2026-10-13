---
title: Read and reply from another computer
description: Run a foreground host gateway, pair each client, and save a private HTTPS connection for the CLI, TUI and MCP.
---

One computer runs the daemon and owns its mail providers, database and search index. Other computers can pair with it and read or search that mailbox through private HTTPS, and request host approval to reply and send. The host must stay awake, online and running mxr.

Default pairings support mailbox reading, search and attachment downloads in the CLI, TUI and MCP. An explicitly approved send grant adds draft preparation, portable uploads, previews, sending and outcome recovery. Mail changes, remote browser sessions and unattended service setup remain future stages. Unsupported remote operations return an error.

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

The gateway binds only to loopback and uses this computer's local daemon. A saved remote selection does not change which daemon it serves. Keep both terminals open. Use the same mxr version on host and clients; this connection requires IPC protocol 15. Upgrade the host and clients together; old pairings retain read-only authority.

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

`mail_send` authorizes only the draft, preview, portable attachment and reviewed send operations required for composing and sending. It does not grant host configuration, credentials, filesystem access, process control, recovery administration or activity history. Remote MCP uses this registration-wide grant; local MCP profiles do not narrow it.

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

## Switch one client back to its local daemon

```sh
mxr connect --local --dry-run --format json
mxr connect --local --confirm --format json
mxr connect --status --format json
```

Switching retires only this installation's registration. Other independently paired clients continue using the host. Local selection takes effect even when the host is offline; the result reports pending deregistration, and the client retains enough information to retry it. Later client use or `mxr connect --status` retries pending cleanup. An unreachable host leaves the intent pending.

Switching does not copy the host's database to this computer or move drafts between workspaces. A local mailbox still needs its own local configuration. Cancelling an unfinished pairing with `--local --confirm` also retains cleanup until the host acknowledges it.

## Revoke access and recover a changed host

On the host:

```sh
mxr serve clients --format json
mxr serve clients revoke CLIENT_ID --dry-run --format json
mxr serve clients revoke CLIENT_ID --confirm --format json
```

The next operation on an already-open remote connection checks the current registration and rejects a revoked credential. Work admitted before revocation may finish. Revocation does not cancel previously accepted work.

Client credentials are stored in an owner-only local file using mxr's existing plaintext credential policy. Do not copy that file to make another client; pair the new installation independently. The host keeps verifiers rather than the client secrets. Logs and structured command output omit secrets.

A restored host workspace invalidates prior registrations. A changed host, workspace or protocol fails verification before a new saved selection is applied. Preserve the old client state and private work while investigating; switch away explicitly and pair again when the intended host is ready. See [pairing authority](/guides/pairing-authority/) and [backup recovery](/guides/backup-recovery/).
