# HTTP Bridge

`mxr daemon` exposes an HTTP/WebSocket surface so non-terminal clients —
browser apps, mobile clients, agents, scripts — can drive the same
mailbox the TUI and CLI do. As of **v0.5.0** this is a first-class,
supported API with an OpenAPI 3.1 spec.

## TL;DR

```bash
# Bridge starts automatically with the daemon.
mxr daemon --foreground

# Discover endpoints.
curl http://127.0.0.1:7777/api/v1/openapi.json | jq .info

# Liveness probe (no auth needed).
curl http://127.0.0.1:7777/api/v1/health

# Anything else needs the bearer token.
TOKEN="$(cat ~/.config/mxr/bridge-token)"
curl -H "Authorization: Bearer $TOKEN" \
     http://127.0.0.1:7777/api/v1/admin/status

# Interactive docs.
open http://127.0.0.1:7777/api/v1/docs
```

## Architecture

Two ways to run the bridge — both serve the same router code:

| Mode | When to use |
|---|---|
| **Managed task** (default) | `mxr daemon` starts the bridge automatically. One PID to monitor, one config source, one auth source. |
| **Standalone** (`mxr web`) | Failure isolation — desktop app uses this so a bridge crash doesn't take down the daemon. |

The default port is **7777** (loopback). Configurable via
`~/.config/mxr/config.toml`:

```toml
[bridge]
enabled = true                      # default true
bind = "127.0.0.1"
port = 7777
cors_allowlist = []                 # additive to localhost defaults
host_allowlist = []                 # additive to loopback (only honoured on non-loopback binds)
# token_path = "..."                # default ~/.config/mxr/bridge-token
```

CLI overrides:

```bash
mxr daemon --no-bridge              # don't bind the bridge this run
mxr daemon --bridge-port 8080       # override port
```

## Authentication

Every route except `/api/v1/health` requires a bearer token. Token is in
`~/.config/mxr/bridge-token` (mode 0600, generated on first daemon
start). Rotate it by deleting that file and restarting the daemon.

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
     "http://127.0.0.1:7777/api/v1/mail/mailbox?lens_kind=inbox&limit=10"

# Trigger a sync
curl -X POST -H "Authorization: Bearer $TOKEN" \
     http://127.0.0.1:7777/api/v1/mail/sync

# Tail events with curl + a websocket client (websocat)
websocat "ws://127.0.0.1:7777/api/v1/events?token=$TOKEN"
```

### Browser fetch

```js
const TOKEN = await loadBridgeToken();
const res = await fetch("http://127.0.0.1:7777/api/v1/admin/status", {
  headers: { Authorization: `Bearer ${TOKEN}` },
});
```

### Browser WebSocket

```js
const ws = new WebSocket(
  "ws://127.0.0.1:7777/api/v1/events",
  ["bearer", TOKEN],
);
```

### Generate a typed client

The bridge ships an OpenAPI 3.1 spec. Any language with an
[OpenAPI Generator](https://openapi-generator.tech/) target works:

```bash
# TypeScript fetch client
npx openapi-typescript \
  http://127.0.0.1:7777/api/v1/openapi.json \
  -o src/api.ts

# Or with the dump example (no running daemon needed):
cargo run --quiet --example dump_openapi_spec -p mxr-web > spec.json
npx openapi-typescript spec.json -o src/api.ts
```

## DNS rebinding hardening

Loopback HTTP servers are exposed to **DNS rebinding** attacks: a
malicious page in the user's browser can resolve `attacker.com` to
`127.0.0.1` and issue same-origin requests to the bridge. The bridge
defends with three layers:

1. **Bearer auth required everywhere** (except `/health`) — even on
   loopback. A page without the token can't do anything useful.
2. **Host-header allowlist** — `localhost`, `127.0.0.1`, `[::1]` are
   always allowed; everything else returns 403. Mismatches are how DNS
   rebinding requests look from the server's side.
3. **CORS allowlist** — only loopback origins by default
   (`http://localhost:*`, `https://localhost:*`). Cross-origin requests
   from other domains get rejected at the preflight.

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
