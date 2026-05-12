# HTTP Bridge

> **Audience:** contributors. This page is the **internal architecture + security model** for the bridge.
> Canonical endpoint reference for users lives at [/reference/bridge](https://mxr.planetaryescape.dev/reference/bridge/) (source: `site/src/content/docs/reference/bridge.md`).

`mxr daemon` exposes an HTTP/WebSocket surface so non-terminal clients —
browser apps, mobile clients, agents, scripts — can drive the same
mailbox the TUI and CLI do. As of **v0.5.0** this is a first-class,
supported API with an OpenAPI 3.1 spec.

## TL;DR

```bash
# Bridge starts automatically with the daemon.
mxr daemon --foreground

# Discover endpoints.
curl http://mxr.localhost:42829/api/v1/openapi.json | jq .info

# Liveness probe (no auth needed).
curl http://mxr.localhost:42829/api/v1/health

# Anything else needs the bearer token.
TOKEN="$(cat ~/.config/mxr/bridge-token)"
curl -H "Authorization: Bearer $TOKEN" \
     http://mxr.localhost:42829/api/v1/admin/status

# Interactive docs.
open http://mxr.localhost:42829/api/v1/docs
```

## Architecture

Two ways to run the bridge — both serve the same router code:

| Mode | When to use |
|---|---|
| **Managed task** (default) | `mxr daemon` starts the bridge automatically. One PID to monitor, one config source, one auth source. |
| **Detached standalone** (`mxr web`) | Failure isolation — `mxr web` starts or reopens a separate bridge process so the terminal can close without killing the browser UI. |

The default port is **42829** (loopback) — a high unprivileged port
chosen to avoid the common dev-server set (3000/5173/8000/8080/7777).
Configurable via `~/.config/mxr/config.toml`:

```toml
[bridge]
enabled = true                      # default true
bind = "127.0.0.1"
port = 42829                        # stable local web URL port
cors_allowlist = []                 # additive to localhost defaults
host_allowlist = []                 # additive to loopback (only honoured on non-loopback binds)
auto_local_token = true             # let loopback callers auto-fetch the token (see below)
# token_path = "..."                # default ~/.config/mxr/bridge-token
```

CLI overrides:

```bash
mxr daemon --no-bridge              # don't bind the bridge this run
mxr daemon --bridge-port 8080       # override port
mxr web                             # start/reopen detached bridge and open browser
mxr web stop                        # stop the detached bridge
mxr web --foreground                # debug the bridge in the current terminal
mxr web --port 9000                 # use a different fixed local port
mxr web --auto-port                 # try the next free port on conflict
```

**Port behavior.** The bridge uses a fixed local URL by default:
`http://mxr.localhost:42829`. Port conflicts fail fast so users know how
to get back home. Pass `mxr web --auto-port` to try the next available
port (up to 32 ports). The actual bound port is written to
`<config_dir>/bridge-port` so clients (the Vite dev proxy, scripts) can
discover it. Detached `mxr web` also writes
`<data_dir>/web.pid`, `<data_dir>/web.port`, and `<data_dir>/web.host`
so `mxr web` can reopen the running bridge and `mxr web stop` can stop
it.

## Authentication

Every route except `/api/v1/health` and `/api/v1/auth/local-token`
requires a bearer token. Token is in `~/.config/mxr/bridge-token` (mode
0600, generated on first daemon start). Rotate it by deleting that file
and restarting the daemon.

The bridge accepts the token via four mechanisms:

1. **`Authorization: Bearer <token>`** (preferred — what generated SDKs
   use, what Swagger UI's Authorize button does)
2. **`?token=<token>`** query string — fallback for `EventSource` and
   `curl` users who can't easily set headers
3. **`Sec-WebSocket-Protocol: bearer, <token>`** subprotocol — for
   browser WebSocket clients (browsers can't set arbitrary headers on
   WS upgrades)
4. **`x-mxr-bridge-token: <token>`** — v0.4.x compat header, kept
   through the v0.5 cycle, removed in v0.6

### Same-machine auto-handshake

`GET /api/v1/auth/local-token` is the one authenticated-but-unauthed
endpoint: it returns the bridge token **only** when the connecting TCP
peer is a loopback IP. It exists so a same-machine web client (the SPA
served by `mxr web`, the Vite dev server) can bootstrap without making
the user paste a token.

Both conditions must hold for the endpoint to return the token:

1. The operator hasn't disabled it (`[bridge].auto_local_token = true`,
   the default).
2. The connecting peer's IP is a loopback address — `127.0.0.0/8`,
   `::1`, or equivalent.

If either fails the endpoint returns **404**, never 401/403, so
cross-network scanners can't tell the endpoint exists. Set
`auto_local_token = false` for paranoid setups that want a strict
bearer handshake even on loopback (e.g. multi-user developer machines
where one user shouldn't auto-claim another's bridge token).

## URL conventions

```
/api/v1/admin/*       — daemon health, diagnostics, status
/api/v1/mail/*        — read, search, mutate, sync, compose
/api/v1/platform/*    — accounts, rules, saved searches, LLM, semantic, analytics
/api/v1/desktop/*     — client-specific UI shaping (transitional)
/api/v1/events        — WebSocket event stream (10 DaemonEvent variants)
/api/v1/health        — unauthenticated liveness probe
/api/v1/openapi.json  — OpenAPI 3.1 spec
/api/v1/docs          — Swagger UI
```

The IPC bucket layer matches mxr's internal IPC contract — see
[`docs/blueprint/16-addendum.md`](../blueprint/16-addendum.md) for the
boundary rationale.

## Versioning

Path-based — `/api/v1/...`. Additive changes within v1 are
non-breaking. Breaking changes ship as v2 with v1 maintained for one
minor release cycle.

## Migration from v0.4.x

The v0.4.x experimental bridge used flat paths (`/status`,
`/mailbox`, …) and the custom `x-mxr-bridge-token` header. Both are
preserved through the v0.5 cycle:

- **Flat paths** return `308 Permanent Redirect` to the new
  `/api/v1/<bucket>/...` location. The `308` (not `301`) is intentional
  — it preserves POST / DELETE method semantics.
- **`x-mxr-bridge-token`** keeps working as an auth path.

Both will be removed in v0.6. Migrate by:

1. Repath every fetch to `/api/v1/<bucket>/...` — see the OpenAPI spec
   for the mapping.
2. Switch the header to `Authorization: Bearer <token>`.

The desktop app (`apps/desktop`) sends both headers in v0.5 for
forward+backward compat.

## Examples

### `curl`

```bash
TOKEN="$(cat ~/.config/mxr/bridge-token)"

# List inbox threads
curl -H "Authorization: Bearer $TOKEN" \
     "http://mxr.localhost:42829/api/v1/mail/mailbox?lens_kind=inbox&limit=10"

# Trigger a sync
curl -X POST -H "Authorization: Bearer $TOKEN" \
     http://mxr.localhost:42829/api/v1/mail/sync

# Tail events with curl + a websocket client (websocat)
websocat "ws://mxr.localhost:42829/api/v1/events?token=$TOKEN"
```

### Browser fetch

```js
const TOKEN = await loadBridgeToken();
const res = await fetch("http://mxr.localhost:42829/api/v1/admin/status", {
  headers: { Authorization: `Bearer ${TOKEN}` },
});
```

### Browser WebSocket

```js
const ws = new WebSocket(
  "ws://mxr.localhost:42829/api/v1/events",
  ["bearer", TOKEN],
);
```

### Generate a typed client

The bridge ships an OpenAPI 3.1 spec. Any language with an
[OpenAPI Generator](https://openapi-generator.tech/) target works:

```bash
# TypeScript fetch client
npx openapi-typescript \
  http://mxr.localhost:42829/api/v1/openapi.json \
  -o src/api.ts

# Or with the dump example (no running daemon needed):
cargo run --quiet --example dump_openapi_spec -p mxr-web > spec.json
npx openapi-typescript spec.json -o src/api.ts
```

## Remote access

First-class public bridge hosting is reserved for a later TLS/auth design.
For now, keep the bridge loopback-bound and reach it through SSH,
Tailscale, or WireGuard.

SSH tunnel example:

```bash
ssh -L 42829:127.0.0.1:42829 user@vps.example.com
```

Then open `http://mxr.localhost:42829` on your laptop.

Manual remote-host mode exists for operators who have already configured
a public reverse proxy and token distribution:

```bash
mxr web --remote-host mxr.example.com
```

Requirements for manual remote-host mode:

- Terminate TLS in front of the bridge with Caddy, nginx, Cloudflare, or
  equivalent. Remote non-loopback access should use `https://`.
- Configure `[bridge]` `cors_allowlist` and `host_allowlist` for the
  public origin/host you expose.
- Copy the remote bridge token to the client machine at
  `~/.config/mxr/bridge-tokens/<host>.token` with mode `0600`.

`mxr web --remote-host mxr.example.com` reads that per-host token, opens
`https://mxr.example.com/#token=...&remote=mxr.example.com`, and the SPA
stores the remote origin for future API and WebSocket calls.

## DNS rebinding hardening

Loopback HTTP servers are exposed to **DNS rebinding** attacks: a
malicious page in the user's browser can resolve `attacker.com` to
`127.0.0.1` and issue same-origin requests to the bridge. The bridge
defends with three layers:

1. **Bearer auth required everywhere** (except `/health`) — even on
   loopback. A page without the token can't do anything useful.
2. **Host-header allowlist** — `localhost`, `mxr.localhost`, `127.0.0.1`,
   `[::1]` are always allowed; everything else returns 403. Mismatches
   are how DNS rebinding requests look from the server's side.
3. **CORS allowlist** — only loopback origins by default
   (`http://localhost:*`, `http://mxr.localhost:*`, and HTTPS forms).
   Cross-origin requests from other domains get rejected at the preflight.

Non-loopback binds (`bind = "0.0.0.0"`) are refused at startup unless
TLS termination is configured (deferred — see "Out of scope" below).

## Out of scope (deliberately)

- gRPC alongside REST — overhead not justified for a single-user
  personal-data daemon
- HTTP/3 / WebTransport — axum support is immature in 2026
- Multi-user / multi-tenant — mxr is single-user
- Auto-generated SDKs — provide the spec, let users generate themselves
- Rate limiting — single-user, loopback-default
- TLS termination — out of scope for v0.5; lands when
  non-loopback binds are first-class
