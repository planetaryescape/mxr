---
title: Web app
description: How the mxr web SPA is served, how it auto-authenticates on the local machine, and how to point it at a remote daemon.
---

The mxr web app is a React SPA that talks to the daemon through the
same HTTP/WebSocket bridge as the TUI, CLI, and desktop app. It's
embedded into the `mxr` binary when built with `--features web-ui` and
served at the daemon's bridge port.

## Quick start

```bash
# In one terminal
mxr daemon --foreground

# In another
mxr web
```

`mxr web` starts or reopens the detached local bridge, opens your
default browser to `http://127.0.0.1:42829`, then returns control to the
terminal. Run `mxr web` again to reopen it, or `mxr web stop` to stop
the detached bridge. On the same machine the SPA auto-authenticates
against the daemon — **no token paste prompt**.

If you'd rather just see the URL, pass `--no-open` or `--print-url`.

## How auto-authentication works

The bridge exposes `GET /api/v1/auth/local-token`, which returns the
bridge bearer token **only** to callers whose TCP peer is a loopback
IP. On first load the SPA calls that endpoint, stores the token in
`localStorage`, and proceeds. If the token in `localStorage` becomes
stale the SPA repeats the handshake automatically.

The endpoint returns `404` (not 401) when:

- `[bridge].auto_local_token = false` in `~/.config/mxr/config.toml`, or
- the caller is not on the same machine (the bridge is bound to a
  non-loopback address and the request originates from a different host).

That means cross-network scanners can't even tell the endpoint exists.

To disable the same-machine handshake — strict bearer auth even on
loopback, useful on multi-user machines — set:

```toml
[bridge]
auto_local_token = false
```

In that mode the SPA falls back to a paste-token panel at
`/settings/token`.

## Port behavior

The bridge prefers port **42829**. On `EADDRINUSE` it walks up to the
next free port (up to 32 attempts). The actual bound port is written
to `<config_dir>/bridge-port` (`~/.config/mxr/bridge-port` on Linux,
`~/Library/Application Support/mxr/bridge-port` on macOS) so:

- The Vite dev proxy (`apps/web/`) reads it to know where to send `/api`.
- Scripts can read it instead of hardcoding `42829`.
- `mxr status` and `mxr web --print-url` reflect the actual port.

Detached `mxr web` also records `<data_dir>/web.pid`,
`<data_dir>/web.port`, and `<data_dir>/web.host` so later `mxr web`
runs reopen the same process and `mxr web stop` can terminate it.

Pass `mxr web --strict-port` to opt out of retries and fail fast.

## Remote-host mode

When the daemon runs on a VPS, open the browser pointed at it:

```bash
mxr web --remote-host mxr.example.com
```

This **does not bind a local bridge**. It reads the per-host token from
`~/.config/mxr/bridge-tokens/<host>.token` (mode 0600 — place it there
yourself) and opens the browser to `https://<host>/#token=<token>`.

Requirements on the remote side:

- TLS termination (Caddy / nginx / Cloudflare). The bridge itself does
  not yet terminate TLS.
- `[bridge].cors_allowlist` includes your browser's origin.
- `[bridge].host_allowlist` includes the public hostname (defends
  against DNS rebinding).
- `[bridge].auto_local_token = false` is recommended — the loopback
  check on the bridge already refuses non-loopback peers, but disabling
  the endpoint outright is one fewer surface to reason about.

## Development against a running daemon

Inside `apps/web/`:

```bash
npm run dev
```

Vite serves the SPA at `http://localhost:5173`, proxying `/api/*` and
the WebSocket to the bridge. It discovers the bridge port via
`<config_dir>/bridge-port`, so port retries on the daemon side just
work — no manual reconfig.

Set `MXR_BRIDGE_URL=http://127.0.0.1:9000` to override.

## Comparing surfaces

| Surface | When you'd use it |
|---|---|
| **CLI** (`mxr ...`) | Scripts, automation, agents, one-off ops. |
| **TUI** (`mxr` no args) | Daily keyboard-driven mail triage in the terminal. |
| **Web app** (`mxr web`) | Multi-account mail in the browser — same daemon, vim-compatible compose editor, full keyboard model. |
| **Desktop app** | Native window around the same web stack. Same auth, same data. |

The web app is the youngest surface; the CLI is the canonical one. If a
feature only exists in the web app it's incomplete by mxr's product
rules — see the [why-mxr guide](/guides/why-mxr/).

## See also

- [HTTP bridge reference](/reference/bridge/) — auth, endpoint table, OpenAPI spec.
- [`mxr web` CLI reference](/reference/cli/web/) — every flag and what it does.
- [Config reference](/reference/config/#bridge) — `[bridge]` keys including `auto_local_token` and `port`.
- [Desktop app guide](/guides/desktop-app/) — the Electron-shelled sibling.
