# Implementation Plan: HTTP Bridge v0.5 + Landing Page Revamp

> Source conversation: bhekani/Claude session 2026-05-07.
> Target release: v0.5.0 (minor bump — new public API surface, breaking
> changes vs the existing experimental bridge endpoints).
> Estimated work: ~2-3 weeks at this scope.

## Context

mxr is a daemon + clients (TUI, CLI). A web crate (`crates/web/`)
exists today as an HTTP bridge — ~5,500 lines of axum, ~30 routes,
WebSocket event relay, token auth, used by the Electron desktop app
as a child process. It covers ~60% of the daemon's `Request` enum
(~80 variants), with permissive CORS, no integration tests, no
OpenAPI spec, and no formal release artifact.

This plan promotes the bridge to a first-class, supported surface
so non-terminal users (browser apps, mobile, agents) can build
against a stable contract. Optimised for **doing the right thing**,
not for shipping fast — the user explicitly authorised the larger
blast radius and longer timeline that follows from that.

## Pre-work: landing page revamp

Already drafted at `site/src/content/docs/index.mdx`. Lead with
user superpowers, not implementation details:

- "Your inbox, on your computer." (offline / local-first / airplane
  scenario)
- "Search like it's local. Because it is." (concrete numbers + use
  cases — `mxr search`, `mxr storage`, `mxr stale`, `mxr wrapped`)
- "Email your agent can fully operate." (kept the existing agent
  examples; reframed lead)
- "When Gmail dies, you don't." (Reader/Inbox/Hangouts/Stadia)
- "The same engine, the surface you want." (TUI/CLI/HTTP/Build-your-
  own — explicitly calls out the bridge as a first-class option)
- Install + lineage paragraph at the bottom

Killed: architecture ASCII diagram (selling the flower); the giant
comparison matrix (implementation-feature-led); the "missing email
CLI" framing (adversarial). Lineage now sits at the bottom as a
gracious credit, not the lead.

This doc covers the bridge work; the landing page is a separate
commit shipped in parallel.

## Architectural decisions (settled)

### Process model

The bridge becomes a **managed background task inside `mxr daemon`**,
alongside the snooze loop and contacts refresher.

- `mxr daemon` starts the bridge automatically at `127.0.0.1:7777`
  (configurable in `~/.config/mxr/config.toml`).
- Single PID to monitor, single config source, single auth source.
- Existing `mxr web` standalone subcommand stays for failure-
  isolation and ephemeral-child-process use cases (the desktop app
  continues to use it). Both paths exercise the same router code.

Default port: **7777**. Mnemonic, easy to type, low collision
risk.

### API shape

REST + WebSocket + SSE under `/api/v1/`. URL-versioned. Endpoints
organised by the IPC bucket classification from CLAUDE.md:

| Prefix | Purpose | Example |
|---|---|---|
| `/api/v1/mail/*` | core mail (read, search, mutate) | `GET /api/v1/mail/threads/{id}` |
| `/api/v1/platform/*` | accounts, rules, saved searches, semantic, subscriptions | `GET /api/v1/platform/rules` |
| `/api/v1/admin/*` | diagnostics, doctor, status, events log | `POST /api/v1/admin/doctor/rebuild-analytics` |
| `/api/v1/events` | WebSocket event stream (all `DaemonEvent` variants) | upgrade required |
| `/api/v1/events/stream` | SSE fallback for `curl`/scripts | `text/event-stream` |
| `/api/v1/openapi.json` | OpenAPI 3.1 spec | utoipa-generated |
| `/api/v1/docs` | Swagger UI | served from spec |

`client-specific` IPC bucket (pane/selection shaping) stays out of
the HTTP surface, per the same rule that keeps it out of IPC.

### Auth

- Bearer token from `~/.config/mxr/bridge-token` (mode 0600,
  generated on first daemon start; permissions enforced).
- **Required even on loopback**. Defends against DNS rebinding
  attacks where a malicious page in the user's browser can issue
  same-origin requests to `127.0.0.1`.
- Token via `Authorization: Bearer X` (preferred) or `?token=X`
  query string (fallback for situations where headers can't be
  set, e.g. EventSource which doesn't support custom headers).
- **Host-header allowlist** enforced: `localhost`, `127.0.0.1`,
  `[::1]` only. Mismatches → 403.
- **CORS**: explicit allowlist (default `http://localhost:*` and
  `https://localhost:*`), configurable. Permissive CORS is gone.
- Cookies are not used. Bearer tokens only. CSRF is therefore not
  applicable.

### Network exposure

- Default bind: `127.0.0.1` only.
- Opt-in to `0.0.0.0` requires explicit `--bind` config + `--tls-
  cert` + non-default token. The daemon refuses to start without
  all three when a non-loopback bind is configured.

### Discovery

`utoipa-axum`-generated OpenAPI 3.1 served at
`/api/v1/openapi.json`, plus Swagger UI at `/api/v1/docs`. Every
handler is annotated. Every `Request`/`Response` payload type
derives `ToSchema`. The spec drives:

- Third-party SDK generation via `openapi-generator`.
- Type-safe browser client code-gen.
- The desktop app's TS types (current hand-maintained
  `apps/desktop/src/shared/types.ts` becomes generated).
- Inline API docs at `/api/v1/docs` for newcomers.

This unlocks the "build your own client" promise from the landing
page — without an OpenAPI spec, that promise is hand-waving.

### Coverage

**Full parity with the `Request` enum.** All ~80 variants get
routes. Per CLAUDE.md "wire both clients or wire neither" — no
exceptions, no gap-shipping.

Currently ~30 covered; ~50 to add. Notable gaps:

- Account lifecycle: `RemoveAccountConfig`, `DisableAccountConfig`,
  `RepairAccountConfig`.
- Batch operations beyond mutations.
- Diagnostics: `Doctor*` variants, semantic reindex details.
- Analytics surfaces shipped in v0.4.71/0.4.72 (`ListSubscriptions`,
  `Wrapped`, `ContactDecay`, `LargestMessages`, `RebuildAnalytics`,
  `RefreshContacts`).
- `GetEnvelope` (added in v0.4.72 for analytics drill-downs).

### Tests

Real integration tests against a daemon backed by `FakeProvider`.
Per CLAUDE.md "test with the real system, not just unit tests":

- Spin up daemon + bridge in one process.
- Hit each route end-to-end.
- Validate response shape against the OpenAPI spec (using the spec
  as the contract, so drift is detectable).
- WebSocket events: trigger an operation, assert the expected event
  sequence is broadcast.
- Auth: cover token-required, Host-header allowlist, CORS allowlist
  rejection paths.

### Versioning

`/api/v1/` URL-versioned (Sonarr / miniflux pattern). Additive
changes within v1 are non-breaking. Breaking changes go to v2 with
v1 maintained for one minor release cycle.

### Release plan

v0.5.0 minor bump. Breaking changes vs the experimental bridge that
shipped in v0.4.x:

- All routes URL-prefixed with `/api/v1/...`.
- Token required on loopback (was optional).
- CORS no longer permissive.
- Some payload shapes adjusted to match OpenAPI types.

The desktop app updates in lockstep — same release window. Existing
desktop installs prompt to update before they can talk to a v0.5
daemon (protocol-version check the desktop already does).

## Slice ladder

Each slice ships independently — green tests, clean commit, before
moving on. Total: ~2-3 weeks.

### Slice 1 — utoipa scaffold

- Add `utoipa`, `utoipa-axum`, `utoipa-swagger-ui` deps.
- Annotate the existing `Request`/`Response`/`DaemonEvent` payload
  types with `ToSchema` derives.
- Add OpenAPI scaffold to `crates/web/src/lib.rs`: `OpenApiRouter`
  with metadata (title, version, description, contact, license).
- Mount `/api/v1/openapi.json` and `/api/v1/docs`.
- Tests: spec is served, parses as valid OpenAPI 3.1, includes
  expected schemas.

### Slice 2 — URL versioning

- Move every existing route under `/api/v1/...` with the bucket
  prefix.
- Add a deprecation-shim layer that responds to old paths with
  `301 Moved Permanently` to the new path for the v0.5 cycle (helps
  old desktop installs notice).
- Update the desktop app to call new paths.
- Tests: every old path returns 301 with a Location header; every
  new path returns the same response as before.

### Slice 3 — Auth hardening

- Bridge-token-from-file (`~/.config/mxr/bridge-token`, mode 0600).
- Generated on first daemon start if missing.
- Token required even on loopback.
- `Authorization: Bearer X` preferred path; `?token=X` fallback.
- Host-header allowlist middleware.
- Strict CORS allowlist (default `http://localhost:*`,
  `https://localhost:*`; configurable list in
  `~/.config/mxr/config.toml`).
- Permissive CORS removed.
- Tests: missing token → 401, wrong token → 401, wrong host header
  → 403, cross-origin from disallowed origin → CORS rejection,
  EventSource with `?token=` works.

### Slice 4 — Move bridge into daemon as managed task

- Add `bridge_loop` task to `crates/daemon/src/loops.rs`, spawned
  from `mxr daemon` startup like the snooze loop.
- New config section in `~/.config/mxr/config.toml`:

  ```toml
  [bridge]
  enabled = true            # default true
  bind = "127.0.0.1"
  port = 7777
  cors_allowlist = []       # additive to localhost defaults
  ```

- `mxr daemon --no-bridge` flag to disable.
- `mxr daemon --bridge-port N` flag to override port.
- `mxr web` standalone subcommand stays as a separate code path
  using the same router; the desktop app continues to use it.
- Tests: daemon starts → bridge reachable; daemon stops → bridge
  port released; `--no-bridge` → bridge not bound.

### Slice 5 — Close the protocol coverage gap

- Wire all unmapped `Request` variants to routes. ~50 to add.
- Each route gets utoipa annotations.
- Group by IPC bucket: mail / platform / admin.
- Touch points:
  - Account lifecycle (Remove/Disable/Repair)
  - Analytics (ListSubscriptions, Wrapped, ContactDecay,
    LargestMessages, RebuildAnalytics with WebSocket progress
    streaming, RefreshContacts)
  - GetEnvelope (added in v0.4.72)
  - Doctor surfaces
  - Semantic reindex with progress events
- Tests: one route per Request variant, asserting status code and
  basic response shape against the spec.

### Slice 6 — Integration test harness

- New crate-level test module that spins up daemon + bridge.
- Runs against `FakeProvider`.
- One test per route hit; one test per WebSocket event class.
- Validates responses against the served OpenAPI spec (so drift
  fails CI).
- Auth path tests (token / host / CORS).

### Slice 7 — Update desktop app

- Regenerate TS types from the OpenAPI spec (replace hand-
  maintained `apps/desktop/src/shared/types.ts`).
- Update fetch calls to use `/api/v1/...` paths.
- Update auth header (already uses bearer; just confirm and lock
  in).
- Smoke-test the packaged desktop app against a v0.5 daemon.

### Slice 8 — Documentation

- New `docs/guides/http-bridge.md` covering:
  - Architecture (managed task vs standalone, when to use which)
  - Auth (token location, generating new tokens, rotation)
  - URL conventions and versioning
  - Discovery via OpenAPI / Swagger UI
  - Examples: `curl`, browser fetch, agent integration recipes
  - Migration notes from v0.4.x experimental bridge
- Update `docs/reference/cli.md` with new bridge subcommands.
- Update landing page section 5 ("HTTP bridge" card) with concrete
  capabilities.

### Slice 9 — Release v0.5.0

- Bump version (Cargo.toml, .release-please-manifest.json,
  apps/desktop/package.json).
- Cargo.lock refresh.
- Release notes: highlight the bridge promotion, breaking changes,
  desktop app update requirement, OpenAPI spec link.
- Tag, push, verify install surfaces (brew, cargo install --tag).

## Out of scope (deliberate)

- gRPC surface alongside REST (overhead not justified for a single-
  user personal-data daemon).
- WebTransport / HTTP/3 (axum support is immature).
- Self-hosted multi-user mode (mxr is single-user; multi-tenant is
  a different product).
- Auto-generated client SDKs (provide the OpenAPI spec; let users
  generate via `openapi-generator` themselves).
- Rate limiting (single-user, loopback-default; not needed).
- Audit logs (the daemon's existing event log captures mutations).

## Open questions

- Should the WebSocket also accept the bearer token via the
  `Sec-WebSocket-Protocol` subprotocol header (browser-friendly
  alternative when query string is undesirable)? Lean yes.
- Should there be a `/api/v1/health` unauthenticated endpoint for
  liveness probes? Lean yes — useful for desktop app to check
  bridge readiness without first acquiring the token.
- Default token rotation cadence — never (let user rotate
  manually), or auto-rotate annually? Lean never; user-controlled.

## Verification

Per CLAUDE.md "test with the real system":

1. `cargo test --workspace` — all green.
2. Start daemon: `mxr daemon --foreground` in one shell.
3. `curl -H "Authorization: Bearer $(cat ~/.config/mxr/bridge-token)" \
    http://127.0.0.1:7777/api/v1/openapi.json | jq .info.version`
4. Hit a representative route from each bucket; confirm 2xx + sane
   payload.
5. Connect a WebSocket to `/api/v1/events`, trigger a `mxr sync`,
   observe the `OperationStarted/Progress/Completed` event sequence.
6. Trigger `--bind 0.0.0.0` without `--tls-cert`; daemon must
   refuse to start with a clear error.
7. Smoke-test the packaged desktop app end-to-end.

## Resume rule

If this plan needs to continue across sessions: read this file's
**Slice ladder** section, find the lowest unchecked slice in
git log (`git log --oneline | grep -i bridge`), pick up from
there. Each slice produces a commit with a clear subject —
`feat(bridge): slice N — <topic>`.
