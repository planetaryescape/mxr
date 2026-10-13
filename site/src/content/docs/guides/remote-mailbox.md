---
title: Read mail from another computer
description: Run a foreground host gateway, pair each client, and save a private HTTPS connection for the CLI, TUI and MCP.
---

One computer runs the daemon and owns its mail providers, database and search index. Other computers can pair with it and read or search that mailbox through private HTTPS. The host must stay awake, online and running mxr.

The remote connection supports mailbox reading and search in the CLI, TUI and MCP. Replies, mail changes, attachment downloads, remote web sessions and unattended service setup are separate stages. Unsupported remote operations return an error.

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

The gateway binds only to loopback and uses this computer's local daemon. A saved remote selection does not change which daemon it serves. Keep both terminals open. Use the same mxr version on host and clients; this connection requires IPC protocol 14.

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
