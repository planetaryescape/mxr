---
title: Web app
description: How the mxr web SPA is served, how it auto-authenticates on the local machine, and how to point it at a remote bridge.
---

The mxr web app is a React SPA that talks to the daemon through the
same HTTP/WebSocket bridge as the TUI and CLI. It's embedded into the
`mxr` binary when built with `--features web-ui` and served at the
daemon's bridge port.

## Quick start

```bash
# In one terminal
mxr daemon --foreground

# In another
mxr web
```

`mxr web` starts or reopens the detached local bridge, opens your
default browser to `http://mxr.localhost:42829`, then returns control to the
terminal. Run `mxr web` again to reopen it, or `mxr web stop` to stop
the detached bridge. On the same machine the SPA auto-authenticates
against the daemon — **no token paste prompt**.

If you'd rather just see the URL, pass `--no-open` or `--print-url`.

## Demo mode in the web app

`mxr web` works transparently while demo mode is active. After running
`mxr demo`, the bridge binds to its own demo port (namespaced by
`MXR_CONFIG_DIR`, so the real and demo bridges can coexist on different
ports), and the web app's topbar shows a small amber **DEMO** pill next to
the breadcrumb. The pill stays visible on every route, so a recording always
shows which profile is being demoed.

Behind the scenes the SPA polls `/api/v1/admin/status` and reads the new
`is_demo` boolean (`true` when the daemon is bound to the `mxr-demo`
instance). Run `mxr demo stop` to exit demo mode; the next refresh hides the
pill and routes return to your real profile.

## How auto-authentication works

The bridge exposes `GET /api/v1/auth/local-token`, which returns the
bridge bearer token **only** to callers whose TCP peer is a loopback
IP. On first load the SPA calls that endpoint, stores the token in
`localStorage`, and proceeds. If the token in `localStorage` becomes
stale the SPA repeats the handshake automatically.

The endpoint returns `404` (not 401) when:

- `[bridge].auto_local_token = false` in the file printed by `mxr config path`, or
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

The bridge uses port **42829** for the stable local URL. On `EADDRINUSE`
it fails by default and prints best-effort process details for the
listener using that port. Pass `--auto-port` to try the next free port
(up to 32 attempts). The actual bound port is written
to `<config_dir>/bridge-port` for the active runtime identity so:

- The Vite dev proxy (`apps/web/`) reads it to know where to send `/api`.
- Scripts can read it instead of hardcoding `42829`.
- `mxr status` and `mxr web --print-url` reflect the actual port.

Detached `mxr web` also records `<data_dir>/web.pid`,
`<data_dir>/web.port`, and `<data_dir>/web.host` so later `mxr web`
runs reopen the same process and `mxr web stop` can terminate it.

Port conflicts fail fast unless you pass `mxr web --auto-port`.

## Remote access

First-class public bridge hosting is reserved for a later TLS/auth design.
For now, keep the bridge bound to loopback and reach it through a private
tunnel.

SSH tunnel example:

```bash
ssh -L 42829:127.0.0.1:42829 user@vps.example.com
```

Then open `http://mxr.localhost:42829` locally. Tailscale/WireGuard work
too, but keep the bridge itself loopback-bound on the host running mxr.

### Manual remote-host mode

When the daemon runs on a VPS, open the browser pointed at it:

```bash
mxr web --remote-host mxr.example.com
```

This **does not bind a local bridge**. It reads the per-host token from
`bridge-tokens/<host>.token` next to the active config file (mode 0600
— place it there yourself) and opens the browser to
`https://<host>/#token=<token>`.

This mode is for manually configured remote bridges only. Requirements on
the remote side:

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
the WebSocket to the bridge. It discovers the bridge port via the active
runtime identity's `<config_dir>/bridge-port`. By default a dev Vite
server looks at `mxr-dev`; set `MXR_INSTANCE=mxr` only when you
intentionally want it to talk to the installed runtime.

Set `MXR_BRIDGE_URL=http://127.0.0.1:9000` to override.

## Reader layout and links

The web reader follows the same first-class email shortcuts as the TUI:
`f` forwards the open message, while `F` toggles full reader layout.
Full reader hides the mail list so the message takes the available
reading width.

Set the default from **Settings → Reader → Default reader layout**.
This is a web preference, so it follows the browser profile rather than
the daemon TOML config.

When a thread was opened from the mail list, `Esc` closes the reader and
returns focus to the list unless a row selection is active. Links in
HTML, reader, and plain views are clickable; remote image loading is
still controlled separately by the remote-images toggle.

Thread summaries use the same daemon request as `mxr summarize`. Opening an
uncached thread schedules a silent debounced summary request; clicking
**Summary** or pressing `y` forces one immediately. Cached and newly generated
summaries render in the **AI overview** collapsible above the thread.

## Command palette and shared action registry

The web app's command palette (`⌘K`), global keymap (`g i`, `g a`, …),
help dialog (`?`), and **Settings → Keybindings** all read from a single
action registry at `apps/web/src/lib/actions/`. Adding a new feature
action means defining it once — the chord, palette entry, and help row
all light up together. Each feature owns its own `actions.ts` module
(compose, mailbox, diagnostics, analytics, rules, accounts).

`g a` is consistently **All Mail**, matching Gmail and the TUI. Analytics
moved to `g y` ("graphs / y-axis"); a one-time migration toast announces
this the first time you land on `/m/archive` after the parity-closure
update.

The numeric quick-nav (`1`–`0`) is registered as aliases on the
underlying navigation actions, so help and palette both show the verb
and the digit shortcut next to it.

## Compose: autocomplete and outbound undo

The compose pane fetches contact suggestions from
`GET /api/v1/mail/contacts/autocomplete?q=...` with a 200 ms debounce.
ArrowDown/ArrowUp move through the list, Enter commits the highlighted
contact as a chip.

Send is **deferred** by 5 seconds. Clicking Send dismisses the confirm
dialog and shows an undo toast; the actual `compose/session/send` API
call only fires when the toast auto-closes. Click **Undo** within the
window and the send is cancelled, no network request happens.

Draft-assist is wired into the right-rail panel: type an instruction,
the bridge calls `/mail/threads/draft-assist`, the generated body is
shown with a Copy button.

## Mailbox: label / move / unsubscribe / read-and-archive

The optimistic-mutation hook handles the shared mailbox actions:

- Apply a label or remove one (right-rail picker, choose label from the
  shell sidebar).
- Move to a label (right-rail picker; treated as destructive in the
  current view, then invalidated).
- Mark read and archive in one step.
- Unsubscribe the focused message via `mail/actions/unsubscribe`.

These appear in the command palette as `mail.label`, `mail.move`,
`mail.read-and-archive`, and `mail.unsubscribe`, with a `when`
predicate that requires either a focused thread or a non-empty selection.

## Saved-search management

The `/search` page has a "Manage saved searches" disclosure under the
results header. Each row exposes:

- A color swatch (overloaded onto the protocol's `icon` field; the web
  app stores `#RRGGBB`).
- A Pin / Unpin toggle (negative `position` floats the entry above the
  rest in the sidebar lens list).
- Delete with a `confirm()` guard.

All three call `POST /api/v1/platform/saved-searches/update` (added in
the parity-closure work — see the [bridge reference](/reference/bridge/#saved-searches)).

Search supports a Scope picker (Threads / Messages / Attachments) and
keeps the existing j/k result navigation with synced preview pane.

## Wrapped: story mode + share

The Wrapped dashboard has two modes:

- **Standard**: numeric overview + superlatives in a two-column grid.
- **Story mode**: large single-tile presentation; `j`/`k` (or arrow
  keys) cycles between Messages / Inbound / Outbound / Superlatives.

The **Share as image** button uses the browser's Web Share API where
available, falling back to clipboard. A real PNG export pass is tracked
as a follow-up; the current share-text is rich enough for most
short-form contexts.

## Accounts and screener

Account detail gains **Refresh** (invalidates the local query cache)
and **Repair** (`POST /platform/accounts/repair` — re-issues credentials
for an unhealthy keychain entry) next to the existing Test / Default /
Re-auth / Disable buttons.

The screener page is intentionally first-account-only. With multiple
accounts a notice appears below the header pointing to the CLI for
cross-account sweeps — same constraint as the TUI screener queue.

## Sender standalone route

`/sender/<email>` is a deep-linkable sender profile. It hits
`/mail/sender?account_id=...&email=...` against the first active
account and renders the recent-messages list plus the relationship
profile. The same right-rail panel still opens inside a thread.

## Deliveries page

The **Deliveries** entry in the sidebar opens `/deliveries` — the same tracked-packages list the [CLI and TUI](/guides/deliveries/) show. Each card has the merchant, carrier, status, ETA, tracking number, and:

- **Active / Delivered / All** filter tabs.
- A **resolve** (mark delivered) and **dismiss** (hide false positive) button per row.
- An **Open email** link to the source thread, plus a **Track** link when the carrier provides a tracking URL.

It reads `GET /mail/deliveries?filter=...` and posts to `/mail/deliveries/{id}/resolve` and `/dismiss` — see the [bridge reference](/reference/bridge/).

## Calendar invites page

The **Calendar invites** entry in the sidebar opens `/invites` — the same
detected-invites list the [CLI and TUI](/guides/calendar-invites/) show,
across all accounts. Each row is event-centric (summary, when, location,
organizer) and shows your current RSVP status, with:

- Inline **Accept** / **Tentative** / **Decline** buttons for invites that
  still need a response, and a row menu for the "with comment" variants.
- A short undo window before the RSVP is sent (matching the TUI and the
  in-thread invite card).
- Cancelled and updated invites flagged inline.

It reads `GET /mail/invites?limit=...` and reuses the existing invite-reply
action (`POST /mail/actions/invite/reply`) — see the
[bridge reference](/reference/bridge/).

## Comparing surfaces

| Surface | When you'd use it |
|---|---|
| **CLI** (`mxr ...`) | Scripts, automation, agents, one-off ops. |
| **TUI** (`mxr` no args) | Daily keyboard-driven mail triage in the terminal. |
| **Web app** (`mxr web`) | Multi-account mail in the browser, installable as a PWA — same daemon, vim-compatible compose editor, registry-backed keyboard model. |

The web app is the youngest surface; the CLI is the canonical one. If a
feature only exists in the web app it's incomplete by mxr's product
rules — see the [why-mxr guide](/guides/why-mxr/).

## See also

- [HTTP bridge reference](/reference/bridge/) — auth, endpoint table, OpenAPI spec.
- [Keybindings reference](/reference/keybindings/#web-app) — every web chord, including the `g a` / `g y` migration note.
- [`mxr web` CLI reference](/reference/cli/web/) — every flag and what it does.
- [Config reference](/reference/config/#bridge) — `[bridge]` keys including `auto_local_token` and `port`.
- [No native desktop app](/guides/no-native-desktop-app/) — why the web app is installable without an Electron shell.
